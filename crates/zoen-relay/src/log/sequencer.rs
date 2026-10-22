//! Per-Space ordering in the relay's memory, so a hot Space commits in batches (ADR 0023).
//!
//! Each active Space gets one queue and one worker task. The worker takes everything queued
//! (up to [`MAX_BATCH`] envelopes or [`MAX_BATCH_BYTES`]) and hands it to a [`Batcher`],
//! which sequences the batch in one atomic step. While one batch commits, the next one
//! fills, so the commit rate stays at one per round trip while appends per commit grow
//! with load (group commit).
//!
//! The order is the order envelopes reach the queue. A session waits for each publish
//! before it sends the next, so its own envelopes stay in order. Envelopes from
//! different sessions are concurrent, and any order between them is valid.
//!
//! A worker exits after [`IDLE`] without work. Its per-Space state is dropped with it, so
//! memory follows active Spaces, not all Spaces.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::ownership::Fence;
use async_trait::async_trait;
use protocol::Envelope;
use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore};

use super::{Reject, Sequencing};

/// Most envelopes in one batch. Past this point a bigger transaction stops saving
/// round trips and only adds latency to the batch behind it.
pub const MAX_BATCH: usize = 64;
/// Includes queued and executing jobs. Admission never waits for capacity.
pub const MAX_PENDING: usize = 4096;
pub const MAX_PENDING_BYTES: usize = 64 * 1024 * 1024;
pub const SPACE_QUEUE: usize = 128;
pub const MAX_ACTIVE_SPACES: usize = 4096;
/// A batch stops growing once its stored bytes reach this (FoundationDB's recommended
/// transaction size), so it ends at most one envelope (one frame) past it.
pub const MAX_BATCH_BYTES: usize = 1024 * 1024;
/// A Space with no appends for this long releases its worker and state.
pub const IDLE: Duration = Duration::from_secs(30);

/// One envelope waiting to be sequenced.
pub struct Pending {
    pub env: Envelope,
    pub target_known: bool,
    pub fence: Fence,
}

/// What a batch did: one result per envelope, in order, and how many attempts it took.
pub struct Applied {
    pub results: Vec<Result<Sequencing, Reject>>,
    pub attempts: u32,
}

/// Sequences a batch for one Space as one atomic step.
#[async_trait]
pub trait Batcher: Send + Sync + 'static {
    /// The worker keeps this between batches (a cache of the Space). Starts at `Default`.
    type State: Default + Send + 'static;

    async fn append_batch(
        &self,
        space: &str,
        batch: &[Pending],
        state: &mut Self::State,
    ) -> Applied;
}

/// What a caller learns about the batch its envelope joined (for its trace span).
#[derive(Clone, Copy, Debug)]
pub struct BatchStats {
    pub size: u32,
    pub attempts: u32,
}

type Reply = (Result<Sequencing, Reject>, BatchStats);

struct Job {
    pending: Pending,
    reply: oneshot::Sender<Reply>,
    _count: OwnedSemaphorePermit,
    _bytes: OwnedSemaphorePermit,
}

type Queues = Arc<Mutex<HashMap<String, mpsc::Sender<Job>>>>;

pub struct Sequencer<B: Batcher> {
    batcher: Arc<B>,
    queues: Queues,
    idle: Duration,
    count: Arc<Semaphore>,
    bytes: Arc<Semaphore>,
}

impl<B: Batcher> Sequencer<B> {
    pub fn new(batcher: Arc<B>) -> Self {
        Self {
            batcher,
            queues: Arc::default(),
            idle: IDLE,
            count: Arc::new(Semaphore::new(MAX_PENDING)),
            bytes: Arc::new(Semaphore::new(MAX_PENDING_BYTES)),
        }
    }

    /// Queues `env` behind its Space's earlier envelopes and waits for its result.
    pub async fn append(&self, env: Envelope, target_known: bool, fence: Fence) -> Reply {
        let busy = || {
            (
                Err(Reject::retry("sequencer capacity reached")),
                BatchStats {
                    size: 0,
                    attempts: 0,
                },
            )
        };
        let Ok(count) = self.count.clone().try_acquire_owned() else {
            return busy();
        };
        let size = env.retained_len().saturating_add(512);
        let Ok(bytes) = self
            .bytes
            .clone()
            .try_acquire_many_owned(size.min(u32::MAX as usize) as u32)
        else {
            return busy();
        };
        let space = env.space().to_string();
        let (reply, answer) = oneshot::channel();
        let mut job = Job {
            pending: Pending {
                env,
                target_known,
                fence,
            },
            reply,
            _count: count,
            _bytes: bytes,
        };
        // A send fails only when the worker just exited (idle, or a panic). It is already
        // gone from the map or is removed here, so the retry starts a fresh worker.
        for attempt in 0..2 {
            let Some(tx) = self.queue(&space) else {
                return busy();
            };
            match tx.try_send(job) {
                Ok(()) => break,
                Err(mpsc::error::TrySendError::Full(_)) => return busy(),
                Err(mpsc::error::TrySendError::Closed(back)) => {
                    if attempt == 1 {
                        return busy();
                    }
                    job = back;
                    let mut queues = self.queues.lock().expect("queues lock");
                    if queues.get(&space).is_some_and(|q| q.same_channel(&tx)) {
                        queues.remove(&space);
                    }
                }
            }
        }
        answer.await.unwrap_or((
            Err(Reject::unavailable()),
            BatchStats {
                size: 0,
                attempts: 0,
            },
        ))
    }

    /// Spaces with a live worker (for tests and metrics).
    pub fn active(&self) -> usize {
        self.queues.lock().expect("queues lock").len()
    }

    fn queue(&self, space: &str) -> Option<mpsc::Sender<Job>> {
        let mut queues = self.queues.lock().expect("queues lock");
        if let Some(tx) = queues.get(space) {
            return Some(tx.clone());
        }
        if queues.len() >= MAX_ACTIVE_SPACES {
            return None;
        }
        let (tx, rx) = mpsc::channel(SPACE_QUEUE);
        queues.insert(space.to_string(), tx.clone());
        tokio::spawn(work(
            self.batcher.clone(),
            space.to_string(),
            rx,
            tx.clone(),
            self.queues.clone(),
            self.idle,
        ));
        Some(tx)
    }
}

/// The worker for one Space. Every job holds process count and byte permits
/// until its transaction finishes, including after the caller disconnects.
async fn work<B: Batcher>(
    batcher: Arc<B>,
    space: String,
    mut rx: mpsc::Receiver<Job>,
    me: mpsc::Sender<Job>,
    queues: Queues,
    idle: Duration,
) {
    let mut state = B::State::default();
    loop {
        let first = match tokio::time::timeout(idle, rx.recv()).await {
            Ok(Some(job)) => job,
            Ok(None) => return,
            Err(_idle) => {
                // Leave the map under its lock only when nothing is queued. Then close, so
                // a sender that cloned `me` before the removal fails and starts a new
                // worker. Finish anything that slipped in before the close.
                {
                    let mut q = queues.lock().expect("queues lock");
                    if !rx.is_empty() {
                        continue;
                    }
                    if q.get(&space).is_some_and(|tx| tx.same_channel(&me)) {
                        q.remove(&space);
                    }
                }
                drop(me);
                rx.close();
                while let Some(batch) = take(&mut rx, None) {
                    run(&*batcher, &space, batch, &mut state).await;
                }
                return;
            }
        };
        let batch = take(&mut rx, Some(first)).expect("a first job");
        run(&*batcher, &space, batch, &mut state).await;
    }
}

/// `first`, then whatever is already queued, within the batch limits.
fn take(rx: &mut mpsc::Receiver<Job>, first: Option<Job>) -> Option<Vec<Job>> {
    let mut batch: Vec<Job> = first.into_iter().collect();
    let mut bytes: usize = batch.iter().map(|j| j.pending.env.retained_len()).sum();
    while batch.len() < MAX_BATCH && bytes < MAX_BATCH_BYTES {
        let Ok(job) = rx.try_recv() else { break };
        bytes += job.pending.env.retained_len();
        batch.push(job);
    }
    (!batch.is_empty()).then_some(batch)
}

async fn run<B: Batcher>(batcher: &B, space: &str, batch: Vec<Job>, state: &mut B::State) {
    let mut pending = Vec::with_capacity(batch.len());
    let mut replies = Vec::with_capacity(batch.len());
    let mut permits = Vec::with_capacity(batch.len());
    for job in batch {
        pending.push(job.pending);
        replies.push(job.reply);
        permits.push((job._count, job._bytes));
    }
    let applied = batcher.append_batch(space, &pending, state).await;
    #[cfg(debug_assertions)]
    fork_histories::observe(space, &pending, &applied);
    let stats = BatchStats {
        size: pending.len() as u32,
        attempts: applied.attempts,
    };
    drop(permits);
    debug_assert_eq!(applied.results.len(), replies.len());
    for (reply, result) in replies.into_iter().zip(applied.results) {
        // A caller that went away (its connection closed) doesn't need the answer.
        let _ = reply.send((result, stats));
    }
}

/// Debug histories for one Space. A dev-stack relay serves each device the
/// other device's successor, and can withhold the events a far checkpoint
/// names until the lane says to reach them.
///
/// Release builds omit this module, including [`SWITCH`]. Write
/// `$ZOEN_DEV_DIR/fork_histories`, one Space id per line. A line
/// `reach <space>` then delivers the withheld events. Blank lines, `#`
/// comments and the switch string are ignored. `check_causal_link` still
/// accepts or rejects every envelope. Only a causal rejection for a named
/// Space is kept, and sync serves it to the other members. Live fan-out of a
/// named Space is held so a connected device takes this view from sync.
#[cfg(debug_assertions)]
pub(crate) mod fork_histories;

#[cfg(test)]
mod tests;

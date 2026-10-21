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
pub(crate) mod fork_histories {
    use std::collections::{HashMap, HashSet};
    use std::path::Path;
    use std::sync::{LazyLock, Mutex};

    use protocol::{Envelope, Sequenced};
    use types::EventBody;

    use super::{Applied, Pending};

    /// The release binary must not contain this string.
    pub const SWITCH: &str = "zoen-fork-histories-v1";
    /// How many canonical events a debug sync folds into one view.
    pub const READ_LIMIT: usize = 10_000;
    const CAUSAL_REJECT: &str = "signed on a history this relay doesn't have";
    const FILE_NAME: &str = "fork_histories";

    static HELD: LazyLock<Mutex<HashMap<String, Vec<Envelope>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    #[cfg(test)]
    thread_local! {
        static SWITCH_OVERRIDE: std::cell::RefCell<Option<String>> =
            const { std::cell::RefCell::new(None) };
    }

    #[derive(Default)]
    struct Switch {
        spaces: HashSet<String>,
        reach: HashSet<String>,
    }

    pub fn armed(space: &str) -> bool {
        load().spaces.contains(space)
    }

    pub fn view(space: &str, reader: &str, from: u64, canonical: Vec<Sequenced>) -> Vec<Sequenced> {
        if !armed(space) {
            return canonical.into_iter().filter(|ev| ev.seq >= from).collect();
        }
        let reach = load().reach.contains(space);
        let mut chain = cross(reader, &canonical);
        append_held(space, reader, &mut chain);
        chain = apply_far(chain, reach);
        chain.into_iter().filter(|ev| ev.seq >= from).collect()
    }

    pub(crate) fn observe(space: &str, pending: &[Pending], applied: &Applied) {
        if !armed(space) {
            return;
        }
        let mut add = Vec::new();
        for (item, result) in pending.iter().zip(&applied.results) {
            match result {
                Err(reject) if reject.reason == CAUSAL_REJECT => add.push(item.env.clone()),
                _ => {}
            }
        }
        if add.is_empty() {
            return;
        }
        let mut held = HELD.lock().expect("fork histories");
        let slot = held.entry(space.to_string()).or_default();
        for env in add {
            if slot.iter().any(|kept| kept.client_id() == env.client_id()) {
                continue;
            }
            slot.push(env);
        }
    }

    fn load() -> Switch {
        #[cfg(test)]
        if let Some(text) = SWITCH_OVERRIDE.with(|slot| slot.borrow().clone()) {
            return parse(&text);
        }
        let Some(root) = std::env::var_os("ZOEN_DEV_DIR") else {
            return Switch::default();
        };
        read_dir(Path::new(&root))
    }

    fn read_dir(root: &Path) -> Switch {
        let path = root.join(FILE_NAME);
        match std::fs::read_to_string(path) {
            Ok(text) => parse(&text),
            Err(_) => Switch::default(),
        }
    }

    fn parse(text: &str) -> Switch {
        let mut out = Switch::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line == SWITCH {
                continue;
            }
            if let Some(rest) = line.strip_prefix("reach ") {
                let space = rest.trim();
                if !space.is_empty() {
                    out.spaces.insert(space.to_string());
                    out.reach.insert(space.to_string());
                }
                continue;
            }
            out.spaces.insert(line.to_string());
        }
        out
    }

    fn cross(reader: &str, events: &[Sequenced]) -> Vec<Sequenced> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < events.len() {
            let seen = events[i].env.seen();
            let mut end = i + 1;
            if seen.is_some() {
                while end < events.len() && events[end].env.seen() == seen {
                    end += 1;
                }
            }
            let group = &events[i..end];
            let reader_wrote = group.iter().any(|ev| ev.env.author() == reader);
            let someone_else = group.iter().any(|ev| ev.env.author() != reader);
            let drop_own = group.len() > 1 && reader_wrote && someone_else;
            for ev in group {
                if drop_own && ev.env.author() == reader {
                    continue;
                }
                push_linked(&mut out, ev);
            }
            i = end;
        }
        out
    }

    fn append_held(space: &str, reader: &str, chain: &mut Vec<Sequenced>) {
        let held = HELD
            .lock()
            .expect("fork histories")
            .get(space)
            .cloned()
            .unwrap_or_default();
        for env in held {
            if env.author() == reader {
                continue;
            }
            if chain.iter().any(|ev| ev.env.client_id() == env.client_id()) {
                continue;
            }
            push_linked(
                chain,
                &Sequenced {
                    seq: 0,
                    prev: String::new(),
                    hash: String::new(),
                    env,
                },
            );
        }
    }

    fn apply_far(chain: Vec<Sequenced>, reach: bool) -> Vec<Sequenced> {
        let Some(index) = chain.iter().position(is_far) else {
            return chain;
        };
        let checkpoint = &chain[index];
        let Some(seen) = checkpoint.env.seen() else {
            return chain;
        };
        let client = checkpoint.env.client_id();
        let mut out = Vec::new();
        for ev in chain.iter().filter(|ev| ev.seq <= seen.seq) {
            push_linked(&mut out, ev);
        }
        let already = out.last().is_some_and(|ev| ev.env.client_id() == client);
        if !already {
            push_linked(&mut out, checkpoint);
        }
        if reach {
            for ev in chain
                .iter()
                .filter(|ev| ev.seq > seen.seq && ev.env.client_id() != client)
            {
                push_linked(&mut out, ev);
            }
        }
        out
    }

    fn is_far(ev: &Sequenced) -> bool {
        let Some(seen) = ev.env.seen() else {
            return false;
        };
        match ev.env.body() {
            Some(EventBody::Checkpoint { upto, .. }) => upto.seq > seen.seq,
            _ => false,
        }
    }

    fn push_linked(out: &mut Vec<Sequenced>, ev: &Sequenced) {
        if linked(out.last(), ev) {
            out.push(ev.clone());
            return;
        }
        let seq = out.last().map(|prev| prev.seq + 1).unwrap_or(0);
        let prev = out
            .last()
            .map(|prev| prev.hash.clone())
            .unwrap_or_else(|| types::GENESIS_PREV.to_string());
        let hash = event_log::chain_hash(ev.env.space(), seq, &prev, &ev.env.wire_hash());
        out.push(Sequenced {
            seq,
            prev,
            hash,
            env: ev.env.clone(),
        });
    }

    fn linked(prev: Option<&Sequenced>, ev: &Sequenced) -> bool {
        match prev {
            None => ev.seq == 0 && ev.prev == types::GENESIS_PREV,
            Some(prev) => ev.seq == prev.seq + 1 && ev.prev == prev.hash,
        }
    }

    #[cfg(test)]
    mod tests {
        use std::sync::atomic::{AtomicU64, Ordering};

        use event_log::{chain_hash, Author, Signer};
        use protocol::Envelope;
        use types::{EventBody, Privacy, Seen, SpaceKind, GENESIS_PREV};

        use super::super::{Applied, Pending};
        use super::*;
        use crate::log::Reject;
        use crate::ownership::{partition_of, Fence};

        static DIR: AtomicU64 = AtomicU64::new(0);

        struct Armed;
        impl Drop for Armed {
            fn drop(&mut self) {
                SWITCH_OVERRIDE.with(|slot| *slot.borrow_mut() = None);
            }
        }

        fn arm(text: &str) -> Armed {
            SWITCH_OVERRIDE.with(|slot| *slot.borrow_mut() = Some(text.to_string()));
            Armed
        }

        fn message(text: &str) -> EventBody {
            EventBody::MessagePosted {
                message: text.into(),
                text: text.into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            }
        }

        fn place(
            author: &Author,
            space: &str,
            client: &str,
            prev: Option<&Sequenced>,
            seen: Option<Seen>,
            body: EventBody,
        ) -> Sequenced {
            let seq = prev.map(|ev| ev.seq + 1).unwrap_or(0);
            let prev_hash = prev
                .map(|ev| ev.hash.clone())
                .unwrap_or_else(|| GENESIS_PREV.to_string());
            let env =
                Envelope::plain(&author.sign_event(space, client, 1_700_000_000_000, seen, body));
            let hash = chain_hash(space, seq, &prev_hash, &env.wire_hash());
            Sequenced {
                seq,
                prev: prev_hash,
                hash,
                env,
            }
        }

        fn created(author: &Author, space: &str) -> Sequenced {
            place(
                author,
                space,
                "created",
                None,
                None,
                EventBody::SpaceCreated {
                    title: space.into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::Closed,
                },
            )
        }

        fn seen_of(ev: &Sequenced) -> Seen {
            Seen {
                seq: ev.seq,
                hash: ev.hash.clone(),
            }
        }

        fn linked_chain(chain: &[Sequenced]) {
            for (i, ev) in chain.iter().enumerate() {
                let prev = if i == 0 {
                    GENESIS_PREV.to_string()
                } else {
                    chain[i - 1].hash.clone()
                };
                assert_eq!(ev.seq, i as u64);
                assert_eq!(ev.prev, prev);
                assert_eq!(
                    ev.hash,
                    chain_hash(ev.env.space(), ev.seq, &ev.prev, &ev.env.wire_hash())
                );
            }
        }

        fn ids(chain: &[Sequenced]) -> Vec<&str> {
            chain.iter().map(|ev| ev.env.client_id()).collect()
        }

        #[test]
        fn fork_histories_file_names_one_space() {
            let n = DIR.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("u18-fork-file-{n}"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join(FILE_NAME),
                "# lane\nzoen-fork-histories-v1\nsp-a\nreach sp-b\n\n",
            )
            .unwrap();
            let parsed = read_dir(&dir);
            assert!(parsed.spaces.contains("sp-a"));
            assert!(parsed.reach.contains("sp-b"));
            assert!(parsed.spaces.contains("sp-b"));
            assert!(!parsed.spaces.contains("zoen-fork-histories-v1"));
            let missing = read_dir(&dir.join("absent"));
            assert!(missing.spaces.is_empty());
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn fork_histories_crosses_siblings_for_the_author_only() {
            let _armed = arm("sp-cross\n");
            let owner = Author::root(Signer::generate());
            let a = Author::root(Signer::generate());
            let b = Author::root(Signer::generate());
            let space = "sp-cross";
            let head = created(&owner, space);
            let from_a = place(
                &a,
                space,
                "from-a",
                Some(&head),
                Some(seen_of(&head)),
                message("a"),
            );
            let from_b = place(
                &b,
                space,
                "from-b",
                Some(&from_a),
                Some(seen_of(&head)),
                message("b"),
            );
            let canonical = vec![head, from_a, from_b];
            let for_a = view(space, a.identity.as_str(), 0, canonical.clone());
            let for_b = view(space, b.identity.as_str(), 0, canonical.clone());
            let for_owner = view(space, owner.identity.as_str(), 0, canonical.clone());
            assert_eq!(ids(&for_a), ["created", "from-b"]);
            assert_eq!(ids(&for_b), ["created", "from-a"]);
            assert_eq!(ids(&for_owner), ["created", "from-a", "from-b"]);
            linked_chain(&for_a);
            linked_chain(&for_b);
            linked_chain(&for_owner);
            let other = view("sp-other", a.identity.as_str(), 0, canonical);
            assert_eq!(ids(&other), ["created", "from-a", "from-b"]);
        }

        #[test]
        fn fork_histories_holds_a_causal_reject_for_the_other_member() {
            let _armed = arm("sp-held\n");
            let owner = Author::root(Signer::generate());
            let other = Author::root(Signer::generate());
            let space = "sp-held";
            let head = created(&owner, space);
            let rejected = Envelope::plain(&other.sign_event(
                space,
                "held-b",
                1_700_000_000_000,
                Some(Seen {
                    seq: 4,
                    hash: "12".repeat(32),
                }),
                message("b"),
            ));
            let pending = vec![Pending {
                env: rejected,
                target_known: true,
                fence: Fence {
                    partition: partition_of(space),
                    owner: "test".into(),
                    token: 1,
                },
            }];
            observe(
                space,
                &pending,
                &Applied {
                    results: vec![Err(Reject::no(CAUSAL_REJECT))],
                    attempts: 1,
                },
            );
            let for_owner = view(space, owner.identity.as_str(), 0, vec![head.clone()]);
            let for_author = view(space, other.identity.as_str(), 0, vec![head]);
            assert_eq!(ids(&for_owner), ["created", "held-b"]);
            assert_eq!(ids(&for_author), ["created"]);
            linked_chain(&for_owner);
            observe(
                "sp-other",
                &pending,
                &Applied {
                    results: vec![Err(Reject::no(CAUSAL_REJECT))],
                    attempts: 1,
                },
            );
            observe(
                space,
                &pending,
                &Applied {
                    results: vec![Err(Reject::no("sealed event of an unknown kind"))],
                    attempts: 1,
                },
            );
            assert!(!HELD
                .lock()
                .expect("fork histories")
                .contains_key("sp-other"));
        }

        #[test]
        fn fork_histories_withholds_a_far_checkpoint_until_reach() {
            let owner = Author::root(Signer::generate());
            let space = "sp-far";
            let head = created(&owner, space);
            let fill_1 = place(
                &owner,
                space,
                "fill-1",
                Some(&head),
                Some(seen_of(&head)),
                message("1"),
            );
            let fill_2 = place(
                &owner,
                space,
                "fill-2",
                Some(&fill_1),
                Some(seen_of(&fill_1)),
                message("2"),
            );
            let checkpoint = place(
                &owner,
                space,
                "far-cp",
                Some(&fill_2),
                Some(seen_of(&head)),
                EventBody::Checkpoint {
                    upto: seen_of(&fill_2),
                    epoch: 1,
                    digest: "ab".repeat(32),
                },
            );
            let canonical = vec![head, fill_1, fill_2, checkpoint];
            let early = {
                let _armed = arm("sp-far\n");
                view(space, "reader", 0, canonical.clone())
            };
            assert_eq!(ids(&early), ["created", "far-cp"]);
            linked_chain(&early);
            let upto = match early[1].env.body() {
                Some(EventBody::Checkpoint { upto, .. }) => upto.seq,
                other => panic!("checkpoint, got {other:?}"),
            };
            assert!(upto > early.last().unwrap().seq);
            let late = {
                let _armed = arm("sp-far\nreach sp-far\n");
                view(space, "reader", 0, canonical.clone())
            };
            assert_eq!(ids(&late), ["created", "far-cp", "fill-1", "fill-2"]);
            linked_chain(&late);
            let tail = {
                let _armed = arm("sp-far\nreach sp-far\n");
                view(space, "reader", 2, canonical)
            };
            assert_eq!(ids(&tail), ["fill-1", "fill-2"]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use event_log::{Author, Signer};
    use types::EventBody;

    /// Records each batch's client ids and answers each envelope with its own id. Each
    /// batch waits for a permit, so a test can hold the worker while a backlog builds.
    struct Recorder {
        batches: Mutex<Vec<(String, Vec<String>)>>,
        gate: Semaphore,
        entered: std::sync::atomic::AtomicUsize,
    }

    impl Default for Recorder {
        fn default() -> Self {
            Self {
                batches: Mutex::default(),
                gate: Semaphore::new(0),
                entered: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl Batcher for Recorder {
        type State = u32;

        async fn append_batch(&self, space: &str, batch: &[Pending], runs: &mut u32) -> Applied {
            self.entered
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.gate.acquire().await.expect("gate").forget();
            *runs += 1;
            let ids: Vec<String> = batch
                .iter()
                .map(|p| p.env.client_id().to_string())
                .collect();
            self.batches
                .lock()
                .unwrap()
                .push((space.to_string(), ids.clone()));
            Applied {
                results: ids.into_iter().map(|id| Err(Reject::no(id))).collect(),
                attempts: *runs,
            }
        }
    }

    fn fence(space: &str) -> Fence {
        Fence {
            partition: crate::ownership::partition_of(space),
            owner: "test".into(),
            token: 1,
        }
    }

    fn envelope(author: &Author, space: &str, i: u64) -> Envelope {
        let body = EventBody::MessagePosted {
            message: format!("m{i}"),
            text: format!("text {i}"),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        };
        Envelope::plain(&author.sign_event(space, &format!("c{i:04}"), 0, None, body))
    }

    fn reason(r: Result<Sequencing, Reject>) -> String {
        match r {
            Err(r) => r.reason,
            Ok(_) => panic!("the recorder rejects everything"),
        }
    }

    fn ids(n: u64) -> Vec<String> {
        (0..n).map(|i| format!("c{i:04}")).collect()
    }

    #[tokio::test]
    async fn backlog_commits_in_order_in_capped_batches() {
        let rec = Arc::new(Recorder::default());
        let seq = Arc::new(Sequencer::new(rec.clone()));
        let a = Author::root(Signer::generate());
        // Queue 1 + 120 envelopes while the first batch is held, in a known order.
        let first = tokio::spawn({
            let (seq, env) = (seq.clone(), envelope(&a, "s1", 0));
            async move { seq.append(env, true, fence("s1")).await }
        });
        while seq.active() == 0 {
            tokio::task::yield_now().await;
        }
        tokio::task::yield_now().await;
        let mut rest = Vec::new();
        for i in 1..=120 {
            let (seq, env) = (seq.clone(), envelope(&a, "s1", i));
            rest.push(tokio::spawn(async move {
                seq.append(env, true, fence("s1")).await
            }));
            tokio::task::yield_now().await;
        }
        rec.gate.add_permits(100);

        let (r, stats) = first.await.unwrap();
        assert_eq!(reason(r), "c0000", "each caller gets its own result");
        assert_eq!(stats.size, 1);
        for (i, h) in rest.into_iter().enumerate() {
            let (r, _) = h.await.unwrap();
            assert_eq!(reason(r), format!("c{:04}", i + 1));
        }
        let batches = rec.batches.lock().unwrap().clone();
        let sizes: Vec<usize> = batches.iter().map(|(_, b)| b.len()).collect();
        assert_eq!(sizes, [1, MAX_BATCH, 120 - MAX_BATCH]);
        let order: Vec<String> = batches.into_iter().flat_map(|(_, b)| b).collect();
        assert_eq!(order, ids(121), "queue order is commit order");
    }

    #[tokio::test]
    async fn spaces_have_their_own_workers_and_idle_ones_exit() {
        let rec = Arc::new(Recorder::default());
        rec.gate.add_permits(100);
        let seq = Sequencer {
            idle: Duration::from_millis(50),
            ..Sequencer::new(rec.clone())
        };
        let a = Author::root(Signer::generate());
        let (one, two) = tokio::join!(
            seq.append(envelope(&a, "s1", 0), true, fence("s1")),
            seq.append(envelope(&a, "s2", 1), true, fence("s2"))
        );
        assert_eq!(seq.active(), 2);
        assert_eq!(
            (one.1.attempts, two.1.attempts),
            (1, 1),
            "state is per Space"
        );

        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(seq.active(), 0, "idle workers leave");
        let (_, again) = seq.append(envelope(&a, "s1", 2), true, fence("s1")).await;
        assert_eq!(again.attempts, 1, "a new worker starts from fresh state");
        assert_eq!(seq.active(), 1);
    }
    #[tokio::test]
    async fn slow_space_rejects_excess_and_keeps_capacity_until_commit() {
        use futures_util::FutureExt;
        let rec = Arc::new(Recorder::default());
        let seq = Arc::new(Sequencer::new(rec.clone()));
        let a = Author::root(Signer::generate());
        let first = tokio::spawn({
            let (seq, env) = (seq.clone(), envelope(&a, "slow", 0));
            async move { seq.append(env, true, fence("slow")).await }
        });
        while rec.entered.load(std::sync::atomic::Ordering::Relaxed) == 0 {
            tokio::task::yield_now().await;
        }
        // Drop the callers: pending jobs still own their capacity until committed.
        for i in 1..=SPACE_QUEUE {
            assert!(seq
                .append(envelope(&a, "slow", i as u64), true, fence("slow"))
                .now_or_never()
                .is_none());
        }
        let refused = seq
            .append(envelope(&a, "slow", 999), true, fence("slow"))
            .now_or_never()
            .unwrap()
            .0
            .unwrap_err();
        assert!(!refused.permanent);
        assert_eq!(seq.count.available_permits(), MAX_PENDING - SPACE_QUEUE - 1);
        rec.gate.add_permits(100);
        let _ = first.await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while seq.count.available_permits() != MAX_PENDING {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(seq.bytes.available_permits(), MAX_PENDING_BYTES);
        assert_eq!(
            rec.batches
                .lock()
                .unwrap()
                .iter()
                .map(|(_, b)| b.len())
                .sum::<usize>(),
            SPACE_QUEUE + 1
        );
    }

    #[tokio::test]
    async fn unsigned_invites_consume_bytes_while_batches_and_callers_are_stalled() {
        use futures_util::FutureExt;
        let rec = Arc::new(Recorder::default());
        let author = Author::root(Signer::generate());
        let mut env = envelope(&author, "one", 0);
        env.invite = Some("x".repeat(1024 * 1024 - 1024));
        assert!(env.verify().is_ok(), "invite bytes are unsigned");
        assert!(env.stored_len() < 1024);
        assert!(!env.valid_invite());
        let size = env.retained_len() + 512;
        let seq = Sequencer {
            bytes: Arc::new(Semaphore::new(size * 2)),
            ..Sequencer::new(rec.clone())
        };
        for space in ["one", "two"] {
            let mut queued = envelope(&author, space, 0);
            queued.invite = env.invite.clone();
            assert!(seq
                .append(queued, true, fence(space))
                .now_or_never()
                .is_none());
        }
        assert_eq!(
            seq.bytes.available_permits(),
            0,
            "abandoned callers cannot release job bytes"
        );
        let r = seq
            .append(env, true, fence("one"))
            .now_or_never()
            .unwrap()
            .0
            .unwrap_err();
        assert!(!r.permanent && r.reason.contains("capacity"));
        assert_eq!(seq.count.available_permits(), MAX_PENDING - 2);
        rec.gate.add_permits(2);
        tokio::time::timeout(Duration::from_secs(2), async {
            while seq.count.available_permits() != MAX_PENDING {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(seq.bytes.available_permits(), size * 2);
    }

    #[tokio::test]
    async fn routing_across_spaces_still_obeys_process_count_and_byte_limits() {
        use futures_util::FutureExt;
        let rec = Arc::new(Recorder::default());
        let a = Author::root(Signer::generate());
        let seq = Sequencer {
            count: Arc::new(Semaphore::new(2)),
            ..Sequencer::new(rec.clone())
        };
        for space in ["one", "two"] {
            assert!(seq
                .append(envelope(&a, space, 0), true, fence(space))
                .now_or_never()
                .is_none());
        }
        let r = seq
            .append(envelope(&a, "three", 0), true, fence("three"))
            .now_or_never()
            .unwrap()
            .0
            .unwrap_err();
        assert!(!r.permanent);
        assert_eq!(seq.active(), 2);
        let seq = Sequencer {
            bytes: Arc::new(Semaphore::new(1)),
            ..Sequencer::new(rec.clone())
        };
        let r = seq
            .append(envelope(&a, "bytes", 0), true, fence("bytes"))
            .now_or_never()
            .unwrap()
            .0
            .unwrap_err();
        assert!(!r.permanent);
        assert_eq!(seq.active(), 0);
        assert_eq!(seq.count.available_permits(), MAX_PENDING);
        rec.gate.add_permits(100);
    }
}

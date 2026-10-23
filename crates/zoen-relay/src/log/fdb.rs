//! FoundationDB implementation of [`LogStore`] (ADR 0008).
//!
//! Keys, tuple-encoded under `("zoen", cell)`:
//!
//! ```text
//! ("s", space, "meta")                 -> (kind, privacy, created_by)
//! ("s", space, "head")                 -> (seq, hash)
//! ("s", space, "log", seq)             -> Sequenced (the wire bytes)
//! ("s", space, "dedupe", author, cid)  -> seq
//! ("s", space, "m", identity)          -> role
//! ("s", space, "gone", identity)       -> seq of their removal (until they're added back)
//! ("s", space, "mls")                  -> (next commit epoch, device of the last commit)
//! ("s", space, "ck", identity, device) -> (seq a member device holds up to, when it said so
//!                                         in ms) (end-to-end only; device "*" = added, no
//!                                         checkpoint yet)
//! ("s", space, "pruned")               -> first seq not pruned
//! ("i", identity, space)               -> ""            membership by identity
//! ("inv", code_hash)                   -> (space, role, created_by, expires_ms, max_uses, uses)
//! ("ainv", code_hash)                  -> (agent, created_by, expires_ms, max_uses, uses)
//! ("aidx", agent)                      -> code          the agent's one live share link
//! ```
//!
//! Appends to one Space go through its in-memory queue ([`super::sequencer`], ADR 0023).
//! The queue's worker commits each batch as one transaction. The transaction reads the
//! head and the batch's dedupe and invite keys in one round trip, then admits the envelopes
//! in order against a [`SpaceState`]: the meta, members and recent hashes as of a head. It
//! writes each entry, its dedupe key and any membership change, then the final head once.
//!
//! The cache is safe because every write under `("s", space)` also writes that Space's
//! head, so the head names the whole Space state. Reading the head without `snapshot` is
//! the fence: if another relay (or a retry) moved it, the transaction conflicts or
//! finds a different head and reloads. That lets every other read under the Space be a
//! snapshot read. A commit with an unknown result retries into the dedupe keys and answers
//! `Duplicate`.
//!
//! Pruning (ADR 0026) is the one rewrite: once every current member has checkpointed, a
//! sealed entry below the lowest checkpoint is held by every member device, and its stored
//! bytes become a stub without the MLS data (same header, same chain hash). It happens in
//! the transaction of the checkpoint that moves the floor, a bounded batch at a time.

use async_trait::async_trait;
use event_log::chain_hash;
use foundationdb::{
    future::FdbValues,
    options::StreamingMode,
    tuple::{pack, unpack, Subspace},
    Database, FdbBindingError, RangeOption, Transaction,
};
use futures_util::future::{try_join3, try_join_all};
use protocol::{Envelope, InviteCreated, Sequenced};
use relay_admission::mls_projection::{self, PRUNE_BATCH};
use types::{EventBody, Privacy, Role, SpaceKind, GENESIS_PREV};

fn extend_co_members(
    out: &mut Vec<String>,
    seen: &mut HashSet<String>,
    who: &str,
    members: impl IntoIterator<Item = String>,
) {
    for member in members {
        if member != who && seen.insert(member.clone()) {
            out.push(member);
        }
    }
}

#[cfg(test)]
#[path = "co_member_dedup_tests.rs"]
mod co_member_dedup_tests;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use crate::ownership::{self, Fence, NodeOwner};

use super::{
    admission::{self, admit, Effect, Facts},
    sequencer::{Applied, Batcher, Pending, Sequencer},
    InviteInfo, LogStore, Reject, Sequencing, StoreError, VisiblePage,
};

#[cfg(debug_assertions)]
mod conflicts;

mod runtime;
pub use runtime::{RuntimeHead, RuntimeSource, RuntimeSourceBatch};

pub struct FdbLog {
    cell: Arc<Cell>,
    sequencer: Sequencer<Cell>,
    pub owner: Arc<NodeOwner>,
    forwarding: std::sync::OnceLock<ownership::forward::Forwarder>,
    routing_count: Arc<tokio::sync::Semaphore>,
    routing_bytes: Arc<tokio::sync::Semaphore>,
}

/// One cell's keyspace: the transactions behind [`FdbLog`].
struct Cell {
    db: Arc<Database>,
    root: Subspace,
}

/// One Space as of `head`: what admission needs, kept by the Space's worker between
/// batches. It holds only while the stored head still equals `head`.
#[derive(Default)]
struct SpaceState {
    head: Option<(u64, String)>,
    kind: Option<SpaceKind>,
    privacy: Option<Privacy>,
    creator: String,
    mls: Option<(u64, String)>,
    members: BTreeMap<String, Role>,
    /// End-to-end only: what each member device holds, by (identity, device), and when
    /// it said so (ms).
    holds: BTreeMap<(String, String), (u64, i64)>,
    /// First seq not pruned.
    pruned: u64,
    /// Hashes of the newest entries, by seq, so `seen` checks need no read.
    recent: BTreeMap<u64, String>,
}

/// Recent hashes kept per Space. Clients mostly cite the newest entries they saw.
const RECENT: usize = 1024;

/// How long a device's hold counts after its last checkpoint (ADR 0026): 30 days. Devices
/// in use checkpoint at least daily, so one past this never came back; it stops holding
/// pruning back and, if it returns, rejoins from a new Welcome. `ZOEN_PRUNE_CEILING_SECS`
/// changes it (journeys use seconds).
fn prune_ceiling_ms() -> i64 {
    static CEILING: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *CEILING.get_or_init(|| {
        mls_projection::ceiling_ms(std::env::var("ZOEN_PRUNE_CEILING_SECS").ok().as_deref())
    })
}

/// The seqs an envelope's admission needs the chain hash of.
fn cited(env: &Envelope) -> impl Iterator<Item = u64> {
    let upto = match env.body() {
        Some(EventBody::Checkpoint { upto, .. }) => Some(upto.seq),
        _ => None,
    };
    env.seen().map(|s| s.seq).into_iter().chain(upto)
}

impl SpaceState {
    /// Where pruning may go up to (exclusive): the lowest seq a current member device
    /// holds, once every current member holds something. `None` while anyone is missing.
    /// A hold older than the ceiling (`now_ms - ceiling_ms`) no longer counts.
    fn prune_floor(&self, now_ms: i64, ceiling_ms: i64) -> Option<u64> {
        mls_projection::prune_floor(self.privacy, &self.members, &self.holds, now_ms, ceiling_ms)
    }

    fn advance(&mut self, seq: u64, hash: &str) {
        self.head = Some((seq, hash.to_string()));
        self.recent.insert(seq, hash.to_string());
        while self.recent.len() > RECENT {
            self.recent.pop_first();
        }
    }
}

fn word<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn parse<T: serde::de::DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
}

pub use relay_admission::code_hash;

fn random_code() -> String {
    const A: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut b = [0u8; 10];
    getrandom::getrandom(&mut b).expect("entropy");
    b.iter().map(|x| A[(*x & 31) as usize] as char).collect()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn custom(e: impl Into<String>) -> FdbBindingError {
    FdbBindingError::CustomError(Box::new(StoreError(e.into())))
}

fn store_err(e: FdbBindingError) -> StoreError {
    StoreError(e.to_string())
}

type Invite = (String, String, String, i64, i64, i64);
/// Agent share link: (agent_id, created_by, expires_ms, max_uses, uses).
type AgentInvite = (String, String, i64, i64, i64);

/// Who already spent a use of an agent link (one use per person per code).
fn redeemers(root: &Subspace, code: &str) -> Subspace {
    root.subspace(&("ainvr", code_hash(code)))
}

fn agent_info(inv: AgentInvite) -> InviteInfo {
    InviteInfo {
        role: Role::Member,
        space: String::new(),
        inviter: inv.1,
        title: String::new(),
        members: 0,
        agent: Some(inv.0),
    }
}

impl FdbLog {
    /// `cluster_file` = `None` uses the default (`FDB_CLUSTER_FILE` or the system file).
    pub fn open(cluster_file: Option<&str>, cell: &str) -> anyhow::Result<Self> {
        Self::open_as(
            cluster_file,
            cell,
            &crate::fanout::new_node_id(),
            NodeOwner::ttl_from_env()?,
        )
    }

    pub fn open_as(
        cluster_file: Option<&str>,
        cell: &str,
        node: &str,
        ttl: std::time::Duration,
    ) -> anyhow::Result<Self> {
        let cell = Arc::new(Cell {
            db: Arc::new(Database::new(cluster_file)?),
            root: Subspace::all().subspace(&("zoen", cell)),
        });
        let owner = Arc::new(NodeOwner::new(
            cell.db.clone(),
            cell.root.clone(),
            node.to_string(),
            ttl,
        ));
        Ok(Self {
            sequencer: Sequencer::new(cell.clone()),
            owner,
            forwarding: std::sync::OnceLock::new(),
            routing_count: Arc::new(tokio::sync::Semaphore::new(super::sequencer::MAX_PENDING)),
            routing_bytes: Arc::new(tokio::sync::Semaphore::new(
                super::sequencer::MAX_PENDING_BYTES,
            )),
            cell,
        })
    }

    pub async fn enable_forwarding(
        self: &Arc<Self>,
        url: &str,
        cell: &str,
        pool: sqlx::PgPool,
    ) -> anyhow::Result<()> {
        let forwarder = ownership::forward::Forwarder::connect(self, url, cell, pool).await?;
        self.forwarding
            .set(forwarder)
            .map_err(|_| anyhow::anyhow!("forwarding already initialized"))?;
        Ok(())
    }

    pub fn start_renewal(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        NodeOwner::spawn(&self.owner, move || {
            weak.upgrade().is_some_and(|log| log.forwarding_available())
        });
    }

    fn forwarding_available(&self) -> bool {
        self.forwarding.get().is_none_or(|f| f.connected())
    }

    /// The receiving owner never re-routes a forwarded request. Its supplied generation
    /// must survive the conflict read in the transaction that stores the batch.
    pub async fn append_fenced(
        &self,
        env: &Envelope,
        target_known: bool,
        fence: Fence,
    ) -> Result<Sequencing, Reject> {
        if !env.valid_invite() {
            return Err(Reject::no("invalid invite code"));
        }
        if fence.owner != self.owner.node || fence.partition != ownership::partition_of(env.space())
        {
            return Err(Reject::retry("stale partition owner"));
        }
        let (result, stats) = self
            .sequencer
            .append(env.clone(), target_known, fence)
            .await;
        let span = tracing::Span::current();
        span.record("batch", stats.size);
        span.record("attempts", stats.attempts);
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_invite_fenced(
        &self,
        who: &str,
        space: &str,
        role: Role,
        max_uses: u32,
        ttl_secs: u64,
        code: &str,
        fence: Fence,
    ) -> Result<InviteCreated, String> {
        let (max, ttl_ms) = relay_admission::invite::invite_limits(max_uses, ttl_secs);
        let expires = now_ms() + ttl_ms;
        let key = self.cell.root.pack(&("inv", code_hash(code)));
        let fence = &fence;
        let r = self
            .cell
            .db
            .run(|trx, _| {
                let key = key.clone();
                async move {
                    ownership::bound_transaction(&trx)?;
                    let version = trx.get_read_version().await?;
                    let lease = ownership::lease_in(&trx, &self.cell.root, fence.partition).await?;
                    if fence.owner != self.owner.node
                        || !ownership::accepts(lease.as_ref(), fence, space, version)
                    {
                        return Ok(Err("stale partition owner"));
                    }
                    if !protocol::valid_invite_code(code) {
                        return Ok(Err("invalid invite code"));
                    }
                    let mine = self.cell.role_in(&trx, space, who).await?;
                    if let Err(reason) = relay_admission::invite::authorize_inviter(mine, role) {
                        return Ok(Err(reason));
                    }
                    let kind = match trx.get(&self.cell.space_key(space, "meta"), false).await? {
                        Some(m) => {
                            unpack::<(String, String, String)>(&m)
                                .map_err(|e| custom(e.to_string()))?
                                .0
                        }
                        None => return Ok(Err("unknown space")),
                    };
                    if let Some(kind) = parse::<SpaceKind>(&kind) {
                        if let Err(reason) = relay_admission::invite::authorize_invite_kind(kind) {
                            return Ok(Err(reason));
                        }
                    }
                    if let Some(existing) = trx.get(&key, false).await? {
                        let inv: Invite = unpack(&existing).map_err(|e| custom(e.to_string()))?;
                        if inv.0 != space || inv.1 != word(&role) || inv.2 != who || inv.4 != max {
                            return Ok(Err("invite code collision"));
                        }
                    } else {
                        trx.set(&key, &pack(&(space, word(&role), who, expires, max, 0i64)));
                    }
                    let stored = trx
                        .get(&key, true)
                        .await?
                        .ok_or_else(|| custom("invite missing after mint"))?;
                    let inv: Invite = unpack(&stored).map_err(|e| custom(e.to_string()))?;
                    Ok(Ok(inv.3))
                }
            })
            .await
            .map_err(|e| e.to_string())?;
        let expires = r.map_err(str::to_string)?;
        Ok(InviteCreated {
            code: code.to_string(),
            expires_at_ms: expires,
        })
    }
    /// Clears every key of this cell (decommissioning a cell, throwaway test cells).
    /// Ops (and tests): ends an agent link now, as if its lifetime had run out.
    pub async fn expire_agent_invite(&self, code: &str) -> Result<bool, String> {
        let key = self.cell.root.pack(&("ainv", code_hash(code)));
        self.cell
            .db
            .run(|trx, _| {
                let key = key.clone();
                async move {
                    let Some(v) = trx.get(&key, false).await? else {
                        return Ok(false);
                    };
                    let mut inv: AgentInvite = unpack(&v).map_err(|e| custom(e.to_string()))?;
                    inv.2 = now_ms() - 1;
                    trx.set(&key, &pack(&inv));
                    Ok(true)
                }
            })
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn drop_cell(&self) -> Result<(), StoreError> {
        let (begin, end) = self.cell.root.range();
        self.cell
            .db
            .run(|trx, _| {
                let (begin, end) = (begin.clone(), end.clone());
                async move {
                    trx.clear_range(&begin, &end);
                    Ok(())
                }
            })
            .await
            .map_err(store_err)
    }
}

mod apply;
mod cell;
mod store;

/// Append transactions run again after a conflict (two writers on one Space head) or a
/// retryable error, since start: the contention signal for hot Spaces (ADR 0022).
pub static APPEND_RETRIES: AtomicU64 = AtomicU64::new(0);

/// Append transactions run (one per batch). Appends per batch is the group-commit gain.
pub static APPEND_BATCHES: AtomicU64 = AtomicU64::new(0);

#[async_trait]
impl Batcher for Cell {
    /// `None` until loaded, and again after a failed batch.
    type State = Option<SpaceState>;

    async fn append_batch(
        &self,
        space: &str,
        batch: &[Pending],
        state: &mut Option<SpaceState>,
    ) -> Applied {
        // A retry reruns the closure, so the cache moves into the first attempt and comes
        // back only with a committed result. Later attempts reload from the head they read.
        let cached = Mutex::new(state.take());
        let attempts = AtomicU32::new(0);
        let started = std::time::Instant::now();
        #[cfg(debug_assertions)]
        let hooks = conflicts::Diagnostics::new(self, space);
        #[cfg(not(debug_assertions))]
        let hooks = foundationdb::NoopHooks;
        let r = self
            .db
            .run_with_hooks(&hooks, |trx, _committed| {
                attempts.fetch_add(1, Ordering::Relaxed);
                let cached = cached.lock().expect("cache lock").take();
                async move {
                    ownership::bound_transaction(&trx)?;
                    #[cfg(debug_assertions)]
                    conflicts::Diagnostics::configure(&trx)?;
                    self.apply(&trx, space, batch, cached).await
                }
            })
            .await;
        let attempts = attempts.into_inner();
        APPEND_RETRIES.fetch_add(u64::from(attempts.saturating_sub(1)), Ordering::Relaxed);
        APPEND_BATCHES.fetch_add(1, Ordering::Relaxed);
        #[cfg(debug_assertions)]
        {
            let counts = hooks.counts();
            if counts.commits > 0 || counts.closure_errors > 0 {
                tracing::debug!(target: "zoen_relay::log::fdb::conflicts", attempts, elapsed_ms = started.elapsed().as_millis() as u64,
                    commit_errors = counts.commits, head_conflicts = counts.head, lease_conflicts = counts.lease,
                    other_conflicts = counts.other, unreported_conflicts = counts.unreported,
                    closure_errors = counts.closure_errors, report_errors = counts.report_errors,
                    failed = r.is_err(), "append transaction conflict diagnosis");
            }
        }
        let results = match r {
            Ok((results, fresh)) => {
                *state = Some(fresh);
                results
            }
            Err(e) => {
                tracing::error!(error = %e, size = batch.len(), attempts,
                    elapsed_ms = started.elapsed().as_millis() as u64, "log store error while sequencing");
                batch.iter().map(|_| Err(Reject::unavailable())).collect()
            }
        };
        Applied { results, attempts }
    }
}

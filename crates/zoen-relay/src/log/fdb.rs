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
//! ("s", space, "ck", identity, device) -> seq a member device holds up to (end-to-end only;
//!                                         device "*" = added, no checkpoint yet)
//! ("s", space, "pruned")               -> first seq not pruned
//! ("i", identity, space)               -> ""            membership by identity
//! ("inv", code_hash)                   -> (space, role, created_by, expires_ms, max_uses, uses)
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
use foundationdb::{
    future::FdbValues,
    options::StreamingMode,
    tuple::{pack, unpack, Subspace},
    Database, FdbBindingError, RangeOption, Transaction,
};
use futures_util::future::{try_join3, try_join_all};
use roda_log::chain_hash;
use roda_proto::{Envelope, InviteCreated, Sequenced};
use roda_types::{EventBody, Privacy, Role, SpaceKind, GENESIS_PREV};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use super::{
    admission::{admit, Effect, Facts},
    sequencer::{Applied, Batcher, Pending, Sequencer},
    InviteInfo, LogStore, Reject, Sequencing, StoreError,
};

pub struct FdbLog {
    cell: Arc<Cell>,
    sequencer: Sequencer<Cell>,
}

/// One cell's keyspace: the transactions behind [`FdbLog`].
struct Cell {
    db: Database,
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
    /// End-to-end only: what each member device holds, by (identity, device).
    holds: BTreeMap<(String, String), u64>,
    /// First seq not pruned.
    pruned: u64,
    /// Hashes of the newest entries, by seq, so `seen` checks need no read.
    recent: BTreeMap<u64, String>,
}

/// Recent hashes kept per Space. Clients mostly cite the newest entries they saw.
const RECENT: usize = 1024;

/// The device name of a member's hold from when they were added until they checkpoint.
const ADDED: &str = "*";

/// Entries one transaction prunes at most; the next checkpoint goes on from there.
const PRUNE_BATCH: usize = 256;

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
    fn prune_floor(&self) -> Option<u64> {
        if self.privacy != Some(Privacy::EndToEnd) || self.members.is_empty() {
            return None;
        }
        let all_hold = self.members.keys().all(|m| {
            self.holds
                .range((m.clone(), String::new())..)
                .next()
                .is_some_and(|((who, _), _)| who == m)
        });
        all_hold
            .then(|| self.holds.values().copied().min())
            .flatten()
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

pub fn code_hash(code: &str) -> String {
    hex::encode(Sha256::digest(code.trim().to_uppercase().as_bytes()))
}

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

impl FdbLog {
    /// `cluster_file` = `None` uses the default (`FDB_CLUSTER_FILE` or the system file).
    pub fn open(cluster_file: Option<&str>, cell: &str) -> anyhow::Result<Self> {
        let cell = Arc::new(Cell {
            db: Database::new(cluster_file)?,
            root: Subspace::all().subspace(&("zoen", cell)),
        });
        Ok(Self {
            sequencer: Sequencer::new(cell.clone()),
            cell,
        })
    }

    /// Clears every key of this cell (decommissioning a cell, throwaway test cells).
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

impl Cell {
    fn space_key<T: foundationdb::tuple::TuplePack>(&self, space: &str, rest: T) -> Vec<u8> {
        self.root.subspace(&("s", space)).pack(&rest)
    }

    fn space_range(&self, space: &str, part: &str) -> Subspace {
        self.root.subspace(&("s", space, part))
    }

    async fn all(
        trx: &Transaction,
        range: &Subspace,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, FdbBindingError> {
        Self::scan(trx, range.range(), usize::MAX, false).await
    }

    /// Up to `limit` pairs of `[begin, end)`, following FoundationDB's partial batches.
    /// `snapshot` reads add no conflict range: for reads no decision depends on.
    async fn scan(
        trx: &Transaction,
        (begin, end): (Vec<u8>, Vec<u8>),
        limit: usize,
        snapshot: bool,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, FdbBindingError> {
        let mut out = Vec::new();
        let mut opt = RangeOption::from((begin, end));
        opt.mode = StreamingMode::WantAll;
        let mut iteration = 1;
        while out.len() < limit {
            opt.limit = (limit != usize::MAX).then(|| limit - out.len());
            let page: FdbValues = trx.get_range(&opt, iteration, snapshot).await?;
            let more = page.more();
            for kv in page.iter() {
                out.push((kv.key().to_vec(), kv.value().to_vec()));
            }
            match (more, out.last()) {
                (true, Some((last, _))) => {
                    opt.begin = foundationdb::KeySelector::first_greater_than(last.clone())
                }
                _ => break,
            }
            iteration += 1;
        }
        Ok(out)
    }

    async fn role_in(
        &self,
        trx: &Transaction,
        space: &str,
        who: &str,
    ) -> Result<Option<Role>, FdbBindingError> {
        Ok(trx
            .get(&self.space_key(space, ("m", who)), false)
            .await?
            .and_then(|v| unpack::<String>(&v).ok())
            .and_then(|s| parse(&s)))
    }

    async fn members_in(
        &self,
        trx: &Transaction,
        space: &str,
        snapshot: bool,
    ) -> Result<Vec<(String, Role)>, FdbBindingError> {
        let range = self.space_range(space, "m");
        Ok(Self::scan(trx, range.range(), usize::MAX, snapshot)
            .await?
            .into_iter()
            .filter_map(|(k, v)| {
                let (who,): (String,) = range.unpack(&k).ok()?;
                Some((who, parse(&unpack::<String>(&v).ok()?)?))
            })
            .collect())
    }

    async fn entry(
        &self,
        trx: &Transaction,
        space: &str,
        seq: u64,
        snapshot: bool,
    ) -> Result<Option<Sequenced>, FdbBindingError> {
        match trx
            .get(&self.space_key(space, ("log", seq as i64)), snapshot)
            .await?
        {
            Some(v) => Ok(Some(
                Sequenced::decode(&v).map_err(|e| custom(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    /// The Space as of `head`, read with snapshot reads: the caller's head read is the fence.
    async fn load(
        &self,
        trx: &Transaction,
        space: &str,
        head: Option<(u64, String)>,
    ) -> Result<SpaceState, FdbBindingError> {
        let mut state = SpaceState::default();
        let Some((seq, hash)) = head else {
            return Ok(state);
        };
        if let Some(meta) = trx.get(&self.space_key(space, "meta"), true).await? {
            let (kind, privacy, creator): (String, String, String) =
                unpack(&meta).map_err(|e| custom(e.to_string()))?;
            state.kind = parse(&kind);
            state.privacy = parse(&privacy);
            state.creator = creator;
        }
        if let Some(v) = trx.get(&self.space_key(space, "mls"), true).await? {
            let (next, device): (i64, String) = unpack(&v).map_err(|e| custom(e.to_string()))?;
            state.mls = Some((next as u64, device));
        }
        state.members = self
            .members_in(trx, space, true)
            .await?
            .into_iter()
            .collect();
        let holds = self.space_range(space, "ck");
        for (k, v) in Self::scan(trx, holds.range(), usize::MAX, true).await? {
            let (who, device): (String, String) =
                holds.unpack(&k).map_err(|e| custom(e.to_string()))?;
            let at: i64 = unpack(&v).map_err(|e| custom(e.to_string()))?;
            state.holds.insert((who, device), at as u64);
        }
        if let Some(v) = trx.get(&self.space_key(space, "pruned"), true).await? {
            let at: i64 = unpack(&v).map_err(|e| custom(e.to_string()))?;
            state.pruned = at as u64;
        }
        state.advance(seq, &hash);
        Ok(state)
    }

    /// Sequences `batch` in order inside `trx`. Returns one result per envelope, plus the
    /// Space state as of the new head (the cache once the commit lands).
    async fn apply(
        &self,
        trx: &Transaction,
        space: &str,
        batch: &[Pending],
        cached: Option<SpaceState>,
    ) -> Result<(Vec<Result<Sequencing, Reject>>, SpaceState), FdbBindingError> {
        let head_key = self.space_key(space, "head");
        let dedupe_keys: Vec<Vec<u8>> = batch
            .iter()
            .map(|p| self.space_key(space, ("dedupe", p.env.author(), p.env.client_id())))
            .collect();
        let invite_keys: Vec<Option<Vec<u8>>> = batch
            .iter()
            .map(|p| {
                p.env
                    .invite
                    .as_deref()
                    .map(|code| self.root.pack(&("inv", code_hash(code))))
            })
            .collect();

        // One round trip for every point read the batch needs. Dedupe keys are snapshot
        // reads under the head fence. Invites also change outside appends
        // (`create_invite`), so they keep their conflict ranges.
        let (head, dedupes, invite_values) = try_join3(
            trx.get(&head_key, false),
            try_join_all(dedupe_keys.iter().map(|k| trx.get(k, true))),
            try_join_all(invite_keys.iter().flatten().map(|k| trx.get(k, false))),
        )
        .await?;
        let head: Option<(u64, String)> = head
            .map(|v| unpack::<(i64, String)>(&v).map(|(seq, hash)| (seq as u64, hash)))
            .transpose()
            .map_err(|e| custom(e.to_string()))?;
        let mut invites: HashMap<&[u8], Invite> = HashMap::new();
        for (k, v) in invite_keys.iter().flatten().zip(invite_values) {
            if let Some(v) = v {
                invites.insert(k, unpack(&v).map_err(|e| custom(e.to_string()))?);
            }
        }

        let mut state = match cached {
            Some(state) if state.head == head => state,
            _ => self.load(trx, space, head.clone()).await?,
        };

        // Chain hashes the batch cites (`seen`, a checkpoint's `upto`) older than the cache
        // keeps, fetched together.
        let committed = head.as_ref().map(|(seq, _)| *seq);
        let mut old_seen: Vec<u64> = batch
            .iter()
            .flat_map(|p| cited(&p.env))
            .filter(|seq| committed.is_some_and(|h| *seq <= h) && !state.recent.contains_key(seq))
            .collect();
        old_seen.sort_unstable();
        old_seen.dedup();
        let old_hashes: HashMap<u64, String> = try_join_all(
            old_seen
                .iter()
                .map(|seq| self.entry(trx, space, *seq, true)),
        )
        .await?
        .into_iter()
        .flatten()
        .map(|e| (e.seq, e.hash))
        .collect();

        // Where each new entry of this batch sits in `results`, by dedupe key.
        let mut fresh: HashMap<&[u8], usize> = HashMap::new();
        let mut results = Vec::with_capacity(batch.len());
        for (i, p) in batch.iter().enumerate() {
            let env = &p.env;
            let dedupe = dedupe_keys[i].as_slice();
            if let Some(&at) = fresh.get(dedupe) {
                let Ok(Sequencing::New { ev, .. }) = &results[at] else {
                    unreachable!("fresh points at a new entry")
                };
                results.push(Ok(Sequencing::Duplicate { ev: ev.clone() }));
                continue;
            }
            if let Some(seq) = &dedupes[i] {
                let seq: i64 = unpack(seq).map_err(|e| custom(e.to_string()))?;
                let ev = self
                    .entry(trx, space, seq as u64, true)
                    .await?
                    .ok_or_else(|| custom("dedupe points at a missing entry"))?;
                results.push(Ok(Sequencing::Duplicate { ev }));
                continue;
            }

            let invite_key = invite_keys[i].as_deref();
            let invite = invite_key
                .and_then(|k| invites.get(k))
                .filter(|inv| inv.3 > now_ms() && inv.5 < inv.4);
            let mut facts = Facts {
                head: state.head.clone(),
                target_known: p.target_known,
                invite: invite.and_then(|inv| parse(&inv.1).map(|role| (inv.0.clone(), role))),
                ..Facts::default()
            };
            if let Some((h, _)) = &state.head {
                let target = match env.body() {
                    Some(
                        EventBody::MemberAdded { identity, .. }
                        | EventBody::MemberRemoved { identity },
                    ) => Some(identity),
                    _ => None,
                };
                facts.kind = state.kind;
                facts.author_role = state.members.get(env.author()).copied();
                facts.target_role = target.and_then(|t| state.members.get(&t).copied());
                facts.member_count = state.members.len() as u32;
                facts.privacy = state.privacy;
                facts.mls = state.mls.clone();
                let hash_at = |seq: u64| {
                    (seq <= *h)
                        .then(|| {
                            state
                                .recent
                                .get(&seq)
                                .or_else(|| old_hashes.get(&seq))
                                .cloned()
                        })
                        .flatten()
                };
                facts.seen_hash = env.seen().and_then(|s| hash_at(s.seq));
                facts.upto_hash = match env.body() {
                    Some(EventBody::Checkpoint { upto, .. }) => hash_at(upto.seq),
                    _ => None,
                };
            }
            let effect = match admit(env, &facts) {
                Ok(e) => e,
                Err(r) => {
                    results.push(Err(r));
                    continue;
                }
            };

            let (seq, prev) = match &state.head {
                Some((s, h)) => (s + 1, h.clone()),
                None => (0, GENESIS_PREV.to_string()),
            };
            let hash = chain_hash(space, seq, &prev, &env.wire_hash());
            let mut stored = env.clone();
            stored.invite = None;
            let ev = Sequenced {
                seq,
                prev,
                hash: hash.clone(),
                env: stored,
            };
            trx.set(&self.space_key(space, ("log", seq as i64)), &ev.encode());
            trx.set(dedupe, &pack(&(seq as i64)));
            state.advance(seq, &hash);

            let mut joined = None;
            let mut removed = None;
            match effect {
                Effect::Create { kind, privacy } => {
                    trx.set(
                        &self.space_key(space, "meta"),
                        &pack(&(word(&kind), word(&privacy), env.author())),
                    );
                    state.kind = Some(kind);
                    state.privacy = Some(privacy);
                    state.creator = env.author().to_string();
                    if privacy == Privacy::EndToEnd {
                        self.put_mls(trx, space, &mut state, 0, String::new());
                        self.put_hold(trx, space, &mut state, env.author(), ADDED, seq);
                    }
                    self.put_member(trx, space, env.author(), Role::Owner);
                    state.members.insert(env.author().to_string(), Role::Owner);
                }
                Effect::Add {
                    identity,
                    role,
                    by_invite,
                } => {
                    if let (true, Some(k)) = (by_invite, invite_key) {
                        if let Some(inv) = invites.get_mut(k) {
                            inv.5 += 1;
                            trx.set(k, &pack(inv));
                        }
                    }
                    self.put_member(trx, space, &identity, role);
                    trx.clear(&self.space_key(space, ("gone", identity.as_str())));
                    state.members.insert(identity.clone(), role);
                    if state.privacy == Some(Privacy::EndToEnd) {
                        // Their Welcome comes after this: nothing from here on is pruned
                        // until they checkpoint.
                        self.put_hold(trx, space, &mut state, &identity, ADDED, seq);
                    }
                    joined = Some(identity);
                }
                Effect::Remove { identity } => {
                    trx.clear(&self.space_key(space, ("m", identity.as_str())));
                    trx.clear(&self.root.pack(&("i", identity.as_str(), space)));
                    trx.set(
                        &self.space_key(space, ("gone", identity.as_str())),
                        &pack(&(seq as i64)),
                    );
                    state.members.remove(&identity);
                    self.drop_holds(trx, space, &mut state, &identity);
                    removed = Some(identity);
                }
                Effect::Encrypt => {
                    let kind = state.kind.unwrap_or(SpaceKind::Group);
                    trx.set(
                        &self.space_key(space, "meta"),
                        &pack(&(
                            word(&kind),
                            word(&Privacy::EndToEnd),
                            state.creator.as_str(),
                        )),
                    );
                    state.privacy = Some(Privacy::EndToEnd);
                    self.put_mls(trx, space, &mut state, 0, String::new());
                    let members: Vec<String> = state.members.keys().cloned().collect();
                    for m in members {
                        self.put_hold(trx, space, &mut state, &m, ADDED, seq);
                    }
                }
                Effect::Commit { epoch, device } => {
                    self.put_mls(trx, space, &mut state, epoch + 1, device);
                }
                Effect::Checkpoint { device, upto } => {
                    if state.privacy == Some(Privacy::EndToEnd) && !device.is_empty() {
                        let who = env.author().to_string();
                        let had = state.holds.get(&(who.clone(), device.clone())).copied();
                        if had.is_none_or(|h| h < upto) {
                            self.put_hold(trx, space, &mut state, &who, &device, upto);
                        }
                        if state
                            .holds
                            .remove(&(who.clone(), ADDED.to_string()))
                            .is_some()
                        {
                            trx.clear(&self.space_key(space, ("ck", who.as_str(), ADDED)));
                        }
                    }
                }
                Effect::Nothing => {}
            }
            let mut audience: Vec<String> = state.members.keys().cloned().collect();
            audience.extend(removed);
            fresh.insert(dedupe, results.len());
            results.push(Ok(Sequencing::New {
                ev,
                audience,
                joined,
            }));
        }
        if state.head != head {
            if let Some((seq, hash)) = &state.head {
                trx.set(&head_key, &pack(&(*seq as i64, hash.as_str())));
            }
        }
        self.prune(trx, space, &mut state).await?;
        Ok((results, state))
    }

    /// Prunes sealed entries below the floor every current member device has reached, a
    /// batch at a time. Stubs keep the header and the chain hash, so `seen` and checkpoint
    /// checks and dedupe answers still work, and a later joiner can still link the chain.
    async fn prune(
        &self,
        trx: &Transaction,
        space: &str,
        state: &mut SpaceState,
    ) -> Result<(), FdbBindingError> {
        let Some(floor) = state.prune_floor() else {
            return Ok(());
        };
        if floor <= state.pruned {
            return Ok(());
        }
        let begin = self.space_key(space, ("log", state.pruned as i64));
        let end = self.space_key(space, ("log", floor as i64));
        let rows = Self::scan(trx, (begin, end), PRUNE_BATCH, true).await?;
        let mut next = floor;
        if rows.len() == PRUNE_BATCH {
            let last = rows.last().map(|(k, _)| k.clone()).unwrap_or_default();
            let log = self.space_range(space, "log");
            let (seq,): (i64,) = log.unpack(&last).map_err(|e| custom(e.to_string()))?;
            next = seq as u64 + 1;
        }
        for (k, v) in rows {
            let ev = Sequenced::decode(&v).map_err(|e| custom(e.to_string()))?;
            if let Some(stub) = ev.env.pruned() {
                let stub = Sequenced { env: stub, ..ev };
                trx.set(&k, &stub.encode());
            }
        }
        trx.set(&self.space_key(space, "pruned"), &pack(&(next as i64)));
        state.pruned = next;
        Ok(())
    }

    fn put_hold(
        &self,
        trx: &Transaction,
        space: &str,
        state: &mut SpaceState,
        who: &str,
        device: &str,
        at: u64,
    ) {
        trx.set(
            &self.space_key(space, ("ck", who, device)),
            &pack(&(at as i64)),
        );
        state
            .holds
            .insert((who.to_string(), device.to_string()), at);
    }

    fn drop_holds(&self, trx: &Transaction, space: &str, state: &mut SpaceState, who: &str) {
        let range = self.root.subspace(&("s", space, "ck", who));
        trx.clear_subspace_range(&range);
        state.holds.retain(|(w, _), _| w != who);
    }

    fn put_mls(
        &self,
        trx: &Transaction,
        space: &str,
        state: &mut SpaceState,
        next: u64,
        device: String,
    ) {
        trx.set(
            &self.space_key(space, "mls"),
            &pack(&(next as i64, device.as_str())),
        );
        state.mls = Some((next, device));
    }

    fn put_member(&self, trx: &Transaction, space: &str, who: &str, role: Role) {
        trx.set(&self.space_key(space, ("m", who)), &pack(&word(&role)));
        trx.set(&self.root.pack(&("i", who, space)), b"");
    }
}

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
        let r = self
            .db
            .run(|trx, _committed| {
                attempts.fetch_add(1, Ordering::Relaxed);
                let cached = cached.lock().expect("cache lock").take();
                async move { self.apply(&trx, space, batch, cached).await }
            })
            .await;
        let attempts = attempts.into_inner();
        APPEND_RETRIES.fetch_add(u64::from(attempts.saturating_sub(1)), Ordering::Relaxed);
        APPEND_BATCHES.fetch_add(1, Ordering::Relaxed);
        let results = match r {
            Ok((results, fresh)) => {
                *state = Some(fresh);
                results
            }
            Err(e) => {
                tracing::error!(error = %e, size = batch.len(), "log store error while sequencing");
                batch.iter().map(|_| Err(Reject::unavailable())).collect()
            }
        };
        Applied { results, attempts }
    }
}

#[async_trait]
impl LogStore for FdbLog {
    /// Waits in the Space's queue, then commits with its batch. The span covers both.
    #[tracing::instrument(
        name = "fdb.append",
        skip_all,
        fields(batch = tracing::field::Empty, attempts = tracing::field::Empty)
    )]
    async fn append(&self, env: &Envelope, target_known: bool) -> Result<Sequencing, Reject> {
        let (result, stats) = self.sequencer.append(env.clone(), target_known).await;
        let span = tracing::Span::current();
        span.record("batch", stats.size);
        span.record("attempts", stats.attempts);
        result
    }

    async fn read(
        &self,
        space: &str,
        from: u64,
        limit: usize,
    ) -> Result<Vec<Sequenced>, StoreError> {
        let begin = self.cell.space_key(space, ("log", from as i64));
        let end = self.cell.space_range(space, "log").range().1;
        self.cell
            .db
            .run(|trx, _| {
                let (begin, end) = (begin.clone(), end.clone());
                async move {
                    Cell::scan(&trx, (begin, end), limit, true)
                        .await?
                        .into_iter()
                        .map(|(_, v)| Sequenced::decode(&v).map_err(|e| custom(e.to_string())))
                        .collect::<Result<Vec<_>, _>>()
                }
            })
            .await
            .map_err(store_err)
    }

    async fn role(&self, space: &str, who: &str) -> Result<Option<Role>, StoreError> {
        self.cell
            .db
            .run(|trx, _| async move { self.cell.role_in(&trx, space, who).await })
            .await
            .map_err(store_err)
    }

    async fn removed_at(&self, space: &str, who: &str) -> Result<Option<u64>, StoreError> {
        let key = self.cell.space_key(space, ("gone", who));
        self.cell
            .db
            .run(|trx, _| {
                let key = key.clone();
                async move {
                    Ok(trx
                        .get(&key, true)
                        .await?
                        .and_then(|v| unpack::<i64>(&v).ok())
                        .map(|seq| seq as u64))
                }
            })
            .await
            .map_err(store_err)
    }

    async fn members(&self, space: &str) -> Result<Vec<(String, Role)>, StoreError> {
        self.cell
            .db
            .run(|trx, _| async move { self.cell.members_in(&trx, space, false).await })
            .await
            .map_err(store_err)
    }

    async fn spaces_of(&self, who: &str) -> Result<Vec<String>, StoreError> {
        let range = self.cell.root.subspace(&("i", who));
        self.cell
            .db
            .run(|trx, _| {
                let range = range.clone();
                async move {
                    Ok(Cell::all(&trx, &range)
                        .await?
                        .into_iter()
                        .filter_map(|(k, _)| range.unpack::<(String,)>(&k).ok().map(|(s,)| s))
                        .collect())
                }
            })
            .await
            .map_err(store_err)
    }

    async fn co_members(&self, who: &str) -> Result<Vec<String>, StoreError> {
        let mut out: Vec<String> = Vec::new();
        for space in self.spaces_of(who).await? {
            for (m, _) in self.members(&space).await? {
                if m != who && !out.contains(&m) {
                    out.push(m);
                }
            }
        }
        Ok(out)
    }

    async fn create_invite(
        &self,
        who: &str,
        space: &str,
        role: Role,
        max_uses: u32,
        ttl_secs: u64,
    ) -> Result<InviteCreated, String> {
        let code = random_code();
        let expires = now_ms() + ttl_secs.clamp(60, 30 * 24 * 3600) as i64 * 1000;
        let max = max_uses.clamp(1, 10_000) as i64;
        let key = self.cell.root.pack(&("inv", code_hash(&code)));
        let r = self
            .cell
            .db
            .run(|trx, _| {
                let key = key.clone();
                async move {
                    let mine = self.cell.role_in(&trx, space, who).await?;
                    if !matches!(mine, Some(Role::Owner | Role::Admin)) {
                        return Ok(Err("only owners and admins create invites"));
                    }
                    if role <= Role::Admin && mine != Some(Role::Owner) {
                        return Ok(Err("only the owner invites admins"));
                    }
                    let kind = match trx.get(&self.cell.space_key(space, "meta"), false).await? {
                        Some(m) => {
                            unpack::<(String, String, String)>(&m)
                                .map_err(|e| custom(e.to_string()))?
                                .0
                        }
                        None => return Ok(Err("unknown space")),
                    };
                    if parse::<SpaceKind>(&kind) == Some(SpaceKind::Direct) {
                        return Ok(Err("direct chats don't take invites"));
                    }
                    trx.set(&key, &pack(&(space, word(&role), who, expires, max, 0i64)));
                    Ok(Ok(()))
                }
            })
            .await
            .map_err(|e| e.to_string())?;
        r.map_err(str::to_string)?;
        Ok(InviteCreated {
            code,
            expires_at_ms: expires,
        })
    }

    async fn preview_invite(&self, code: &str) -> Result<InviteInfo, String> {
        let key = self.cell.root.pack(&("inv", code_hash(code)));
        self.cell
            .db
            .run(|trx, _| {
                let key = key.clone();
                async move {
                    let Some(v) = trx.get(&key, false).await? else {
                        return Ok(Err("invite expired or already used"));
                    };
                    let inv: Invite = unpack(&v).map_err(|e| custom(e.to_string()))?;
                    if inv.3 <= now_ms() || inv.5 >= inv.4 {
                        return Ok(Err("invite expired or already used"));
                    }
                    let title = match self
                        .cell
                        .entry(&trx, &inv.0, 0, false)
                        .await?
                        .and_then(|e| e.env.body())
                    {
                        Some(EventBody::SpaceCreated { title, .. }) => title,
                        _ => String::new(),
                    };
                    let members = self.cell.members_in(&trx, &inv.0, false).await?.len() as u32;
                    Ok(Ok(InviteInfo {
                        role: parse(&inv.1).unwrap_or(Role::Member),
                        space: inv.0,
                        inviter: inv.2,
                        title,
                        members,
                    }))
                }
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(str::to_string)
    }
}

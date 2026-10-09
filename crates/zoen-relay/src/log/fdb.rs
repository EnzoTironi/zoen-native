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
use roda_types::{EventBody, Role, SpaceKind, GENESIS_PREV};
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
    members: BTreeMap<String, Role>,
    /// Hashes of the newest entries, by seq, so `seen` checks need no read.
    recent: BTreeMap<u64, String>,
}

/// Recent hashes kept per Space. Clients mostly cite the newest entries they saw.
const RECENT: usize = 1024;

impl SpaceState {
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
            let (kind, _, _): (String, String, String) =
                unpack(&meta).map_err(|e| custom(e.to_string()))?;
            state.kind = parse(&kind);
        }
        state.members = self
            .members_in(trx, space, true)
            .await?
            .into_iter()
            .collect();
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

        // `seen` hashes older than the cache keeps, fetched together.
        let committed = head.as_ref().map(|(seq, _)| *seq);
        let mut old_seen: Vec<u64> = batch
            .iter()
            .filter_map(|p| p.env.seen().map(|s| s.seq))
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
                facts.seen_hash = env.seen().filter(|s| s.seq <= *h).and_then(|s| {
                    state
                        .recent
                        .get(&s.seq)
                        .or_else(|| old_hashes.get(&s.seq))
                        .cloned()
                });
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
                    state.members.insert(identity.clone(), role);
                    joined = Some(identity);
                }
                Effect::Remove { identity } => {
                    trx.clear(&self.space_key(space, ("m", identity.as_str())));
                    trx.clear(&self.root.pack(&("i", identity.as_str(), space)));
                    state.members.remove(&identity);
                    removed = Some(identity);
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
        Ok((results, state))
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

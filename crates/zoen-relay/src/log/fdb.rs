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
//! One append is one transaction: it reads the head and the facts the rules need, then
//! writes the entry, the head, the dedupe key and any membership change. FoundationDB's
//! conflict detection serializes appends to one Space, so no sequencer exists; a commit with
//! an unknown result retries into the dedupe key and answers `Duplicate`.

use async_trait::async_trait;
use foundationdb::{
    future::FdbValues,
    options::StreamingMode,
    tuple::{pack, unpack, Subspace},
    Database, FdbBindingError, RangeOption, Transaction,
};
use roda_log::chain_hash;
use roda_proto::{Envelope, InviteCreated, Sequenced};
use roda_types::{EventBody, Role, SpaceKind, GENESIS_PREV};
use sha2::{Digest, Sha256};

use super::{
    admission::{admit, Effect, Facts},
    InviteInfo, LogStore, Reject, Sequencing, StoreError,
};

pub struct FdbLog {
    db: Database,
    root: Subspace,
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
        let db = Database::new(cluster_file)?;
        Ok(Self {
            db,
            root: Subspace::all().subspace(&("zoen", cell)),
        })
    }

    /// Clears every key of this cell (decommissioning a cell, throwaway test cells).
    pub async fn drop_cell(&self) -> Result<(), StoreError> {
        let (begin, end) = self.root.range();
        self.db
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
    ) -> Result<Vec<(String, Role)>, FdbBindingError> {
        let range = self.space_range(space, "m");
        Ok(Self::all(trx, &range)
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
    ) -> Result<Option<Sequenced>, FdbBindingError> {
        match trx
            .get(&self.space_key(space, ("log", seq as i64)), false)
            .await?
        {
            Some(v) => Ok(Some(
                Sequenced::decode(&v).map_err(|e| custom(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    async fn append_in(
        &self,
        trx: &Transaction,
        env: &Envelope,
        target_known: bool,
    ) -> Result<Result<Sequencing, Reject>, FdbBindingError> {
        let space = env.space();
        let author = env.author();
        let dedupe = self.space_key(space, ("dedupe", author, env.client_id()));
        if let Some(seq) = trx.get(&dedupe, false).await? {
            let seq: i64 = unpack(&seq).map_err(|e| custom(e.to_string()))?;
            let ev = self
                .entry(trx, space, seq as u64)
                .await?
                .ok_or_else(|| custom("dedupe points at a missing entry"))?;
            return Ok(Ok(Sequencing::Duplicate { ev }));
        }

        let head_key = self.space_key(space, "head");
        let head: Option<(u64, String)> = match trx.get(&head_key, false).await? {
            Some(v) => {
                let (seq, hash): (i64, String) = unpack(&v).map_err(|e| custom(e.to_string()))?;
                Some((seq as u64, hash))
            }
            None => None,
        };
        let body = env.body();
        let target = match &body {
            Some(
                EventBody::MemberAdded { identity, .. } | EventBody::MemberRemoved { identity },
            ) => Some(identity.clone()),
            _ => None,
        };
        let mut facts = Facts {
            head: head.clone(),
            target_known,
            ..Facts::default()
        };
        if head.is_some() {
            if let Some(meta) = trx.get(&self.space_key(space, "meta"), false).await? {
                let (kind, _, _): (String, String, String) =
                    unpack(&meta).map_err(|e| custom(e.to_string()))?;
                facts.kind = parse(&kind);
            }
            facts.author_role = self.role_in(trx, space, author).await?;
            if let Some(t) = &target {
                facts.target_role = self.role_in(trx, space, t).await?;
            }
            if facts.kind == Some(SpaceKind::Direct) {
                facts.member_count = self.members_in(trx, space).await?.len() as u32;
            }
            if let Some(seen) = env.seen() {
                if head.as_ref().is_some_and(|(h, _)| seen.seq <= *h) {
                    facts.seen_hash = self.entry(trx, space, seen.seq).await?.map(|e| e.hash);
                }
            }
        }
        let invite_key = env
            .invite
            .as_deref()
            .map(|code| self.root.pack(&("inv", code_hash(code))));
        let mut invite: Option<Invite> = None;
        if let Some(k) = &invite_key {
            if let Some(v) = trx.get(k, false).await? {
                let inv: Invite = unpack(&v).map_err(|e| custom(e.to_string()))?;
                if inv.3 > now_ms() && inv.5 < inv.4 {
                    facts.invite = parse(&inv.1).map(|role| (inv.0.clone(), role));
                    invite = Some(inv);
                }
            }
        }

        let effect = match admit(env, &facts) {
            Ok(e) => e,
            Err(r) => return Ok(Err(r)),
        };

        let (seq, prev) = match head {
            Some((s, h)) => (s + 1, h),
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
        trx.set(&head_key, &pack(&(seq as i64, hash.as_str())));
        trx.set(&dedupe, &pack(&(seq as i64)));

        let mut joined = None;
        let mut removed = None;
        match effect {
            Effect::Create { kind, privacy } => {
                trx.set(
                    &self.space_key(space, "meta"),
                    &pack(&(word(&kind), word(&privacy), author)),
                );
                self.put_member(trx, space, author, Role::Owner);
            }
            Effect::Add {
                identity,
                role,
                by_invite,
            } => {
                if by_invite {
                    if let (Some(k), Some(mut inv)) = (&invite_key, invite) {
                        inv.5 += 1;
                        trx.set(k, &pack(&inv));
                    }
                }
                self.put_member(trx, space, &identity, role);
                joined = Some(identity);
            }
            Effect::Remove { identity } => {
                trx.clear(&self.space_key(space, ("m", identity.as_str())));
                trx.clear(&self.root.pack(&("i", identity.as_str(), space)));
                removed = Some(identity);
            }
            Effect::Nothing => {}
        }
        let mut audience: Vec<String> = self
            .members_in(trx, space)
            .await?
            .into_iter()
            .map(|(m, _)| m)
            .collect();
        if let Some(r) = removed {
            audience.push(r);
        }
        Ok(Ok(Sequencing::New {
            ev,
            audience,
            joined,
        }))
    }

    fn put_member(&self, trx: &Transaction, space: &str, who: &str, role: Role) {
        trx.set(&self.space_key(space, ("m", who)), &pack(&word(&role)));
        trx.set(&self.root.pack(&("i", who, space)), b"");
    }
}

#[async_trait]
impl LogStore for FdbLog {
    #[tracing::instrument(name = "fdb.append", skip_all, fields(attempts = tracing::field::Empty))]
    async fn append(&self, env: &Envelope, target_known: bool) -> Result<Sequencing, Reject> {
        let attempts = std::sync::atomic::AtomicU32::new(0);
        let r = self
            .db
            .run(|trx, _committed| {
                attempts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                async move { self.append_in(&trx, env, target_known).await }
            })
            .await;
        tracing::Span::current().record("attempts", i64::from(attempts.into_inner()));
        r.unwrap_or_else(|e| {
            tracing::error!(error = %e, "log store error while sequencing");
            Err(Reject::unavailable())
        })
    }

    async fn read(
        &self,
        space: &str,
        from: u64,
        limit: usize,
    ) -> Result<Vec<Sequenced>, StoreError> {
        let begin = self.space_key(space, ("log", from as i64));
        let end = self.space_range(space, "log").range().1;
        self.db
            .run(|trx, _| {
                let (begin, end) = (begin.clone(), end.clone());
                async move {
                    Self::scan(&trx, (begin, end), limit, true)
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
        self.db
            .run(|trx, _| async move { self.role_in(&trx, space, who).await })
            .await
            .map_err(store_err)
    }

    async fn members(&self, space: &str) -> Result<Vec<(String, Role)>, StoreError> {
        self.db
            .run(|trx, _| async move { self.members_in(&trx, space).await })
            .await
            .map_err(store_err)
    }

    async fn spaces_of(&self, who: &str) -> Result<Vec<String>, StoreError> {
        let range = self.root.subspace(&("i", who));
        self.db
            .run(|trx, _| {
                let range = range.clone();
                async move {
                    Ok(Self::all(&trx, &range)
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
        let key = self.root.pack(&("inv", code_hash(&code)));
        let r = self
            .db
            .run(|trx, _| {
                let key = key.clone();
                async move {
                    let mine = self.role_in(&trx, space, who).await?;
                    if !matches!(mine, Some(Role::Owner | Role::Admin)) {
                        return Ok(Err("only owners and admins create invites"));
                    }
                    if role <= Role::Admin && mine != Some(Role::Owner) {
                        return Ok(Err("only the owner invites admins"));
                    }
                    let kind = match trx.get(&self.space_key(space, "meta"), false).await? {
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
        let key = self.root.pack(&("inv", code_hash(code)));
        self.db
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
                        .entry(&trx, &inv.0, 0)
                        .await?
                        .and_then(|e| e.env.body())
                    {
                        Some(EventBody::SpaceCreated { title, .. }) => title,
                        _ => String::new(),
                    };
                    let members = self.members_in(&trx, &inv.0).await?.len() as u32;
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

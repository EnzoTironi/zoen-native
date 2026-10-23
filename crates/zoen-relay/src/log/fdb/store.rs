use super::*;

#[async_trait]
impl LogStore for FdbLog {
    async fn ready(&self) -> bool {
        self.owner.ready().await && self.forwarding.get().is_none_or(|f| f.connected())
    }
    /// Waits in the Space's queue, then commits with its batch. The span covers both.
    #[tracing::instrument(
        name = "fdb.append",
        target = "zoen_relay::log::fdb",
        skip_all,
        fields(batch = tracing::field::Empty, attempts = tracing::field::Empty)
    )]
    async fn append(&self, env: &Envelope, target_known: bool) -> Result<Sequencing, Reject> {
        if !env.valid_invite() {
            return Err(Reject::no("invalid invite code"));
        }
        if !self.forwarding_available() {
            return Err(Reject::retry("cell forwarding unavailable"));
        }
        let _count = self
            .routing_count
            .clone()
            .try_acquire_owned()
            .map_err(|_| Reject::retry("relay routing capacity reached"))?;
        let _bytes = self
            .routing_bytes
            .clone()
            .try_acquire_many_owned(
                env.retained_len()
                    .saturating_add(1024)
                    .min(u32::MAX as usize) as u32,
            )
            .map_err(|_| Reject::retry("relay routing capacity reached"))?;
        tokio::time::timeout(std::time::Duration::from_secs(6), async {
            // At most one retry after a stale route or uncertain RPC result. The same
            // envelope/client_id reaches durable dedupe on either owner.
            for attempt in 0..2 {
                let fence = self
                    .owner
                    .resolve(env.space(), || self.forwarding_available())
                    .await?;
                let result = if fence.owner == self.owner.node {
                    self.append_fenced(env, target_known, fence).await
                } else if let Some(forwarder) = self.forwarding.get() {
                    forwarder.append(env, &fence).await
                } else {
                    return Err(Reject::retry(
                        "partition owner requires the cell forwarding bus",
                    ));
                };
                match result {
                    Err(r) if !r.permanent && attempt == 0 => continue,
                    result => return result,
                }
            }
            Err(Reject::retry("partition ownership is changing"))
        })
        .await
        .unwrap_or_else(|_| Err(Reject::retry("relay routing deadline exceeded")))
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

    async fn read_visible(
        &self,
        space: &str,
        who: &str,
        from: u64,
        limit: usize,
    ) -> Result<Option<VisiblePage>, StoreError> {
        let from = i64::try_from(from).map_err(|_| StoreError("invalid history cursor".into()))?;
        self.cell
            .db
            .run(|trx, _| async move {
                // Snapshot reads share one FDB read version: a removal committed
                // before this version clamps the range, while an earlier admitted
                // page cannot include events committed after that read version.
                let through = if self.cell.role_in(&trx, space, who).await?.is_some() {
                    None
                } else {
                    let Some(value) = trx
                        .get(&self.cell.space_key(space, ("gone", who)), true)
                        .await?
                    else {
                        return Ok(None);
                    };
                    let last = unpack::<i64>(&value).map_err(|error| custom(error.to_string()))?;
                    if last < 0 {
                        return Err(custom("invalid removal boundary"));
                    }
                    Some(last)
                };
                if through.is_some_and(|last| from > last) {
                    return Ok(Some(VisiblePage {
                        events: Vec::new(),
                        through: through.map(|v| v as u64),
                    }));
                }
                let begin = self.cell.space_key(space, ("log", from));
                let end = match through.and_then(|last| last.checked_add(1)) {
                    Some(after) => self.cell.space_key(space, ("log", after)),
                    None => self.cell.space_range(space, "log").range().1,
                };
                let events = Cell::scan(&trx, (begin, end), limit, true)
                    .await?
                    .into_iter()
                    .map(|(_, bytes)| {
                        Sequenced::decode(&bytes).map_err(|error| custom(error.to_string()))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Some(VisiblePage {
                    events,
                    through: through.map(|v| v as u64),
                }))
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

    async fn call_pair(&self, space: &str, who: &str, peer: &str) -> Result<bool, StoreError> {
        if who == peer {
            return Ok(false);
        }
        self.cell
            .db
            .run(|trx, _| async move {
                let Some(meta) = trx.get(&self.cell.space_key(space, "meta"), false).await? else {
                    return Ok(false);
                };
                let (kind, privacy, _): (String, String, String) =
                    unpack(&meta).map_err(|error| custom(error.to_string()))?;
                if parse::<SpaceKind>(&kind) != Some(SpaceKind::Direct)
                    || parse::<Privacy>(&privacy) != Some(Privacy::EndToEnd)
                {
                    return Ok(false);
                }
                let range = self.cell.space_range(space, "m");
                // Never allocate a group-sized list before deciding it is not a DM.
                let members = Cell::scan(&trx, range.range(), 3, false).await?;
                if members.len() != 2 {
                    return Ok(false);
                }
                let mut found = (false, false);
                for (key, value) in members {
                    let (id,): (String,) = range
                        .unpack(&key)
                        .map_err(|error| custom(error.to_string()))?;
                    let role = unpack::<String>(&value)
                        .ok()
                        .and_then(|role| parse::<Role>(&role));
                    if !matches!(role, Some(Role::Owner | Role::Admin | Role::Member)) {
                        return Ok(false);
                    }
                    if id == who {
                        found.0 = true;
                    }
                    if id == peer {
                        found.1 = true;
                    }
                }
                Ok(found.0 && found.1)
            })
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
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        // Bound read fanout while overlapping independent membership transactions.
        // try_join_all preserves space order, so first-seen output order is unchanged.
        let spaces = self.spaces_of(who).await?;
        for batch in spaces.chunks(16) {
            for members in try_join_all(batch.iter().map(|space| self.members(space))).await? {
                extend_co_members(
                    &mut out,
                    &mut seen,
                    who,
                    members.into_iter().map(|(member, _)| member),
                );
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
        if !self.forwarding_available() {
            return Err("cell forwarding unavailable".into());
        }
        let _count = self
            .routing_count
            .clone()
            .try_acquire_owned()
            .map_err(|_| "relay routing capacity reached".to_string())?;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(6);
        tokio::time::timeout_at(deadline, async {
            let code = random_code().map_err(|error| error.to_string())?;
            for attempt in 0..2 {
                let fence = invite_routing::resolve(
                    deadline,
                    || self.owner.resolve(space, || self.forwarding_available()),
                    || self.forwarding_available(),
                )
                .await?;
                let result = if fence.owner == self.owner.node {
                    self.create_invite_fenced(who, space, role, max_uses, ttl_secs, &code, fence)
                        .await
                } else if let Some(forwarder) = self.forwarding.get() {
                    forwarder
                        .invite(who, space, role, max_uses, ttl_secs, &code, &fence)
                        .await
                } else {
                    return Err("partition owner requires the cell forwarding bus".into());
                };
                match result {
                    Err(ref e)
                        if attempt == 0
                            && (e == "stale partition owner"
                                || e.starts_with("partition owner")) =>
                    {
                        continue
                    }
                    result => return result,
                }
            }
            Err("partition ownership is changing".into())
        })
        .await
        .unwrap_or_else(|_| Err("relay routing deadline exceeded".into()))
    }

    async fn create_agent_invite(
        &self,
        who: &str,
        agent: &str,
        max_uses: u32,
        ttl_secs: u64,
    ) -> Result<InviteCreated, String> {
        if agent.is_empty() || who.is_empty() {
            return Err("agent invite needs an agent and an owner".into());
        }
        let ttl_ms = ttl_secs.clamp(60, 365 * 24 * 3600) as i64 * 1000;
        let max = max_uses.clamp(1, 10_000) as i64;
        let root = &self.cell.root;
        let idx = root.pack(&("aidx", agent));
        let r = self
            .cell
            .db
            .run(|trx, _| {
                let idx = idx.clone();
                let (who, agent) = (who.to_string(), agent.to_string());
                async move {
                    let now = now_ms();
                    if let Some(v) = trx.get(&idx, false).await? {
                        let code: String = unpack(&v).map_err(|e| custom(e.to_string()))?;
                        let key = root.pack(&("ainv", code_hash(&code)));
                        if let Some(v) = trx.get(&key, false).await? {
                            let inv: AgentInvite = unpack(&v).map_err(|e| custom(e.to_string()))?;
                            if inv.2 > now && inv.4 < inv.3 {
                                return Ok(Ok((code, inv.2)));
                            }
                            trx.clear(&key);
                            let (b, e) = redeemers(root, &code).range();
                            trx.clear_range(&b, &e);
                        }
                    }
                    let code = random_code().map_err(|error| custom(error.to_string()))?;
                    let key = root.pack(&("ainv", code_hash(&code)));
                    if trx.get(&key, false).await?.is_some() {
                        return Ok(Err("invite code collision"));
                    }
                    let expires = now + ttl_ms;
                    trx.set(&key, &pack(&(agent, who, expires, max, 0i64)));
                    trx.set(&idx, &pack(&code));
                    Ok(Ok((code, expires)))
                }
            })
            .await
            .map_err(|e| e.to_string())?;
        let (code, expires_at_ms) = r.map_err(str::to_string)?;
        Ok(InviteCreated {
            code,
            expires_at_ms,
        })
    }

    async fn redeem_agent_invite(&self, code: &str, who: &str) -> Result<InviteInfo, String> {
        let root = &self.cell.root;
        let key = root.pack(&("ainv", code_hash(code)));
        let mine = redeemers(root, code).pack(&who);
        let r = self
            .cell
            .db
            .run(|trx, _| {
                let (key, mine) = (key.clone(), mine.clone());
                async move {
                    let Some(v) = trx.get(&key, false).await? else {
                        return Ok(Err("invite expired or already used"));
                    };
                    let mut inv: AgentInvite = unpack(&v).map_err(|e| custom(e.to_string()))?;
                    if inv.2 <= now_ms() {
                        return Ok(Err("invite expired or already used"));
                    }
                    // One use per person: someone who already redeemed it opens it again free.
                    if trx.get(&mine, false).await?.is_some() {
                        return Ok(Ok(inv));
                    }
                    if inv.4 >= inv.3 {
                        return Ok(Err("invite expired or already used"));
                    }
                    inv.4 += 1;
                    trx.set(&key, &pack(&inv));
                    trx.set(&mine, &[]);
                    Ok(Ok(inv))
                }
            })
            .await
            .map_err(|e| e.to_string())?;
        let inv = r.map_err(str::to_string)?;
        Ok(agent_info(inv))
    }

    async fn revoke_agent_invite(&self, agent: &str) -> Result<(), String> {
        let root = &self.cell.root;
        let idx = root.pack(&("aidx", agent));
        self.cell
            .db
            .run(|trx, _| {
                let idx = idx.clone();
                async move {
                    if let Some(v) = trx.get(&idx, false).await? {
                        let code: String = unpack(&v).map_err(|e| custom(e.to_string()))?;
                        trx.clear(&root.pack(&("ainv", code_hash(&code))));
                        let (b, e) = redeemers(root, &code).range();
                        trx.clear_range(&b, &e);
                        trx.clear(&idx);
                    }
                    Ok(())
                }
            })
            .await
            .map_err(|e| e.to_string())
    }

    async fn preview_invite(&self, code: &str) -> Result<InviteInfo, String> {
        let space_key = self.cell.root.pack(&("inv", code_hash(code)));
        let agent_key = self.cell.root.pack(&("ainv", code_hash(code)));
        self.cell
            .db
            .run(|trx, _| {
                let space_key = space_key.clone();
                let agent_key = agent_key.clone();
                async move {
                    if let Some(v) = trx.get(&space_key, false).await? {
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
                        return Ok(Ok(InviteInfo {
                            role: parse(&inv.1).unwrap_or(Role::Member),
                            space: inv.0,
                            inviter: inv.2,
                            title,
                            members,
                            agent: None,
                        }));
                    }
                    let Some(v) = trx.get(&agent_key, false).await? else {
                        return Ok(Err("invite expired or already used"));
                    };
                    let inv: AgentInvite = unpack(&v).map_err(|e| custom(e.to_string()))?;
                    if inv.2 <= now_ms() || inv.4 >= inv.3 {
                        return Ok(Err("invite expired or already used"));
                    }
                    Ok(Ok(agent_info(inv)))
                }
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(str::to_string)
    }
}

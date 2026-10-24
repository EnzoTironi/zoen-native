use super::*;

impl Cell {
    /// Sequences `batch` in order inside `trx`. Returns one result per envelope, plus the
    /// Space state as of the new head (the cache once the commit lands).
    pub(super) async fn apply(
        &self,
        trx: &Transaction,
        space: &str,
        batch: &[Pending],
        cached: Option<SpaceState>,
    ) -> Result<(Vec<Result<Sequencing, Reject>>, SpaceState), FdbBindingError> {
        let version = trx.get_read_version().await?;
        let lease = ownership::lease_in(trx, &self.root, ownership::partition_of(space)).await?;
        let valid: Vec<bool> = batch
            .iter()
            .map(|p| ownership::accepts(lease.as_ref(), &p.fence, space, version))
            .collect();
        if valid.iter().all(|v| !v) {
            return Ok((
                batch
                    .iter()
                    .map(|_| Err(Reject::retry("stale partition owner")))
                    .collect(),
                SpaceState::default(),
            ));
        }
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
        let unsupported_members = state
            .members
            .keys()
            .any(|who| !super::admission::identity_key(who));
        for (i, p) in batch.iter().enumerate() {
            if !valid[i] {
                results.push(Err(Reject::retry("stale partition owner")));
                continue;
            }
            let env = &p.env;
            let dedupe = dedupe_keys[i].as_slice();
            let managed = match self.managed_publication(trx, p).await? {
                Ok(value) => value,
                Err(reason) => {
                    results.push(Err(reason));
                    continue;
                }
            };
            if let Some(&at) = fresh.get(dedupe) {
                let Some(Ok(Sequencing::New { ev, .. })) = results.get(at) else {
                    return Err(custom("inconsistent fresh append index"));
                };
                if let Some(managed) = &managed {
                    if let Err(reason) = managed.record(trx, ev, env) {
                        results.push(Err(reason));
                        continue;
                    }
                }
                results.push(Ok(Sequencing::Duplicate {
                    ev: ev.clone(),
                    audience: Vec::new(),
                    joined: None,
                }));
                continue;
            }
            if let Some(seq) = &dedupes[i] {
                let seq: i64 = unpack(seq).map_err(|e| custom(e.to_string()))?;
                let ev = self
                    .entry(trx, space, seq as u64, true)
                    .await?
                    .ok_or_else(|| custom("dedupe points at a missing entry"))?;
                if let Some(managed) = &managed {
                    if let Err(reason) = managed.record(trx, &ev, env) {
                        results.push(Err(reason));
                        continue;
                    }
                }
                results.push(Ok(Sequencing::Duplicate {
                    ev,
                    audience: Vec::new(),
                    joined: None,
                }));
                continue;
            }

            if managed.as_ref().is_some_and(|value| !value.allows_new()) {
                results.push(Err(Reject::retry(
                    "original managed publication is uncertain",
                )));
                continue;
            }

            if unsupported_members {
                results.push(Err(Reject::no(
                    "stored membership contains an unsupported identity",
                )));
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
            let mls_writes = match mls_projection::transition(
                &effect,
                state.privacy,
                env.author(),
                seq,
                now_ms(),
                &state.members,
                &state.holds,
            ) {
                Ok(writes) => writes,
                Err(reason) => {
                    results.push(Err(Reject::no(reason)));
                    continue;
                }
            };
            let hash = chain_hash(space, seq, &prev, &env.wire_hash())
                .map_err(|error| custom(error.to_string()))?;
            let mut stored = env.clone();
            stored.invite = None;
            let ev = Sequenced {
                seq,
                prev,
                hash: hash.clone(),
                env: stored,
            };
            if let Some(managed) = &managed {
                if let Err(reason) = managed.record(trx, &ev, env) {
                    results.push(Err(reason));
                    continue;
                }
            }
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
                }
                Effect::Commit { .. } | Effect::Checkpoint { .. } | Effect::Hold { .. } => {}
                Effect::Nothing => {}
            }
            for write in mls_writes {
                match write {
                    mls_projection::Write::Epoch { next, device } => {
                        self.put_mls(trx, space, &mut state, next, device)
                    }
                    mls_projection::Write::Hold {
                        identity,
                        device,
                        seq,
                        when_ms,
                    } => self.put_hold(trx, space, &mut state, &identity, &device, (seq, when_ms)),
                    mls_projection::Write::ClearHold { identity, device } => {
                        state.holds.remove(&(identity.clone(), device.clone()));
                        trx.clear(
                            &self.space_key(space, ("ck", identity.as_str(), device.as_str())),
                        );
                    }
                    mls_projection::Write::ClearIdentity { identity } => {
                        self.drop_holds(trx, space, &mut state, &identity)
                    }
                }
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
        // Use final membership even if a removal followed a duplicate in this
        // same batch. Oversized/invalid legacy rosters keep author-only replay.
        if state.members.len() <= super::admission::MAX_MEMBERS as usize && !unsupported_members {
            for result in &mut results {
                if let Ok(Sequencing::Duplicate {
                    ev,
                    audience,
                    joined,
                }) = result
                {
                    *audience = state.members.keys().cloned().collect();
                    *joined = match ev.env.body() {
                        Some(EventBody::MemberAdded { identity, .. })
                            if state.members.contains_key(&identity) =>
                        {
                            Some(identity)
                        }
                        _ => None,
                    };
                }
            }
        }
        if state.head != head {
            if let Some((seq, hash)) = &state.head {
                trx.set(&head_key, &pack(&(*seq as i64, hash.as_str())));
            }
        }
        self.prune(trx, space, &mut state).await?;
        Ok((results, state))
    }
}

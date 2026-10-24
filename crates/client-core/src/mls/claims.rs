//! Durable key-package claims and membership commits.
use super::{
    claim_meta, claim_operation, identity_set, join_set, leaf_name, mls_err, reconciliation,
    rewelcome_meta, split_leaf, storage, tracing_like, welcome_meta, BTreeSet, ClaimOperation,
    CoreError, Duration, Engine, EventBody, IdentityId, Instant, KeyPackageClaimClock,
    KeyPackageRecord, Leaf, Owed, Role, SealedKind, SpaceId, ADMIN_STAGGER, CLAIM_RETRY,
    KEY_PACKAGE_CLAIM_EXPIRED, R,
};

impl Engine {
    /// The next group that owes a commit adding someone: claim their key packages. Groups
    /// that only owe removals commit right here.
    pub fn mls_to_claim(
        &mut self,
        claim_clock: Option<KeyPackageClaimClock>,
    ) -> R<Option<(SpaceId, Vec<IdentityId>, String)>> {
        let now = Instant::now();
        let ready: Vec<SpaceId> = self
            .net
            .mls
            .retry_at
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(s, _)| s.clone())
            .collect();
        for s in ready {
            self.net.mls.retry_at.remove(&s);
            self.net.mls.mark_group_dirty(s);
        }
        let turn: Vec<SpaceId> = self
            .net
            .mls
            .turn_at
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(s, _)| s.clone())
            .collect();
        for s in turn {
            self.net.mls.turn_at.remove(&s);
            self.net.mls.waited.insert(s.clone());
            self.net.mls.mark_group_dirty(s);
        }
        let dirty: Vec<SpaceId> = self.net.mls.dirty.iter().cloned().collect();
        let mut unsupported = false;
        for space in dirty {
            self.mls_siblings_in(&space)?;
            self.settle_rewelcome(&space);
            self.settle_joins(&space);
            let owed = match self.owed(&space) {
                Ok(Some(owed)) => owed,
                Ok(None) => {
                    self.net.mls.waited.remove(&space);
                    self.net.mls.dirty.remove(&space);
                    continue;
                }
                Err(error) => {
                    self.net.mls.waited.remove(&space);
                    self.net.mls.dirty.remove(&space);
                    tracing_like(&format!("group check in {space}: {error}"));
                    continue;
                }
            };
            if let Some(turn) = self.admin_turn(&space) {
                self.net.mls.dirty.remove(&space);
                self.net.mls.turn_at.insert(space, now + turn);
                continue;
            }
            if owed.adds_nobody() {
                self.net.mls.dirty.remove(&space);
                if let Err(e) = self.commit_now(&space, &[], &owed.remove) {
                    tracing_like(&format!("removal commit in {space}: {e}"));
                    self.net
                        .mls
                        .retry_at
                        .insert(space.clone(), now + CLAIM_RETRY);
                }
                continue;
            }
            let Some(clock) = claim_clock else {
                unsupported = true;
                continue;
            };
            // Resolve recovery recipients before consuming one-shot key
            // packages. A pending agreement lookup must not spend a package
            // and then fail commit_now before staging the handshake.
            if self.recovery_recipients(&space).is_err() {
                continue;
            }
            let device = self
                .account()
                .ok_or_else(|| storage("no account"))?
                .device
                .clone();
            let epoch = self.device()?.epoch(&space).map_err(mls_err)?;
            let saved = self.load_claim(&space)?;
            let claim = match saved {
                Some(c) if c.device == device && c.epoch == epoch && c.intent == owed => c,
                _ => {
                    let c = ClaimOperation {
                        operation_id: claim_operation(clock)?,
                        device,
                        epoch,
                        targets: owed.to_claim(),
                        intent: owed,
                        records: None,
                    };
                    self.save_claim(&space, &c)?;
                    c
                }
            };
            self.net.mls.dirty.remove(&space);
            if let Some(records) = claim.records {
                self.mls_claimed(&space, &claim.operation_id, Ok(records));
                continue;
            }
            self.net.mls.claiming.insert(space.clone());
            return Ok(Some((space, claim.targets, claim.operation_id)));
        }
        if unsupported {
            return Err(CoreError::Invalid {
                reason: "This relay does not support durable key-package claims with a current server clock; upgrade or reconnect before adding chat members.".into(),
            });
        }
        Ok(None)
    }

    pub(super) fn load_claim(&self, space: &str) -> R<Option<ClaimOperation>> {
        self.store
            .meta(&claim_meta(space))?
            .map(|data| serde_json::from_str(&data).map_err(storage))
            .transpose()
    }

    pub(super) fn save_claim(&self, space: &str, claim: &ClaimOperation) -> R<()> {
        self.store.set_meta(
            &claim_meta(space),
            &serde_json::to_string(claim).map_err(storage)?,
        )?;
        Ok(())
    }

    /// A socket may close after a committed claim and before its reply. Only its volatile
    /// in-flight marker is cleared; the next session retries the persisted operation.
    pub(crate) fn mls_claims_disconnected(&mut self) {
        let interrupted: Vec<_> = self.net.mls.claiming.drain().collect();
        for space in interrupted {
            self.net.mls.mark_group_dirty(space);
        }
    }

    /// How long this device waits before committing an owed change in `space`: one stagger
    /// per admin ahead of it in identity order. Called after determining an owed change;
    /// `None` once it has waited or when it is first.
    pub(super) fn admin_turn(&mut self, space: &str) -> Option<Duration> {
        if self.net.mls.waited.contains(space) || self.net.mls.turn_at.contains_key(space) {
            return None;
        }
        let me = self.me.as_deref()?;
        let roster = self.net.mls.rosters.get(space)?;
        let ahead = roster
            .iter()
            .filter(|(id, r)| matches!(r, Role::Owner | Role::Admin) && id.as_str() < me)
            .count() as u32;
        (ahead > 0).then(|| ADMIN_STAGGER * ahead)
    }

    /// What `space`'s group owes, when this device may commit and has nothing out
    /// already. Admins commit for everyone; any member commits for its own devices.
    pub(super) fn owed(&self, space: &str) -> R<Option<Owed>> {
        let me = self.me_id()?;
        let Some(roster) = self.net.mls.rosters.get(space) else {
            return Ok(None);
        };
        let admin = match roster.get(&me) {
            Some(Role::Owner | Role::Admin) => true,
            Some(_) => false,
            None => return Ok(None),
        };
        let joins = self.joins(space);
        let stranded = identity_set(self.store.meta(&rewelcome_meta(space))?);
        let revoked = self.revoked_devices();
        let device = self.device()?;
        let reconciling = device.needs_reconciliation(space);
        let for_me = |name: &String| admin || split_leaf(name).is_some_and(|(id, _)| id == me);
        if !admin
            && !reconciling
            && !joins.keys().any(for_me)
            && !stranded.iter().any(for_me)
            && revoked.is_empty()
        {
            return Ok(None);
        }
        if self.net.mls.claiming.contains(space) || self.net.mls.retry_at.contains_key(space) {
            return Ok(None);
        }
        // Membership changes of ours still on their way: commit them all at once, after
        // the relay has ordered them (a commit must never get ahead of its log entries).
        let changing = self.net.pending.values().any(|s| s == space)
            && self.store.outbox()?.iter().any(|p| {
                p.event.space == space
                    && !p.failed
                    && matches!(
                        p.event.body,
                        EventBody::MemberAdded { .. } | EventBody::MemberRemoved { .. }
                    )
            });
        if changing {
            return Ok(None);
        }
        // A commit or Welcome of ours still on its way: the next commit waits for it, or a
        // newcomer could join from a Welcome that lands after a later commit and never
        // see that commit.
        let handshaking = self.store.outbox()?.iter().any(|p| {
            p.event.space == space
                && !p.failed
                && matches!(&p.event.body, EventBody::Sealed { kind }
                    if kind == SealedKind::Commit.name() || kind == SealedKind::Welcome.name())
        });
        if handshaking {
            return Ok(None);
        }
        let Some(leaves) = device.committed_leaves(space).map_err(mls_err)? else {
            return Ok(None);
        };
        Ok(reconciliation::plan(reconciliation::Input {
            me: &me,
            admin,
            reconciling,
            listed: &roster.keys().cloned().collect(),
            leaves: &leaves,
            joins: &joins,
            stranded: &stranded,
            revoked: &revoked,
        }))
    }

    /// Key packages for a group's newcomers came back: commit them (and any removals).
    pub fn mls_claimed(
        &mut self,
        space: &str,
        operation: &str,
        result: Result<Vec<KeyPackageRecord>, String>,
    ) {
        self.net.mls.wake_recovery(space);
        self.net.mls.claiming.remove(space);
        let mut claim = match self.load_claim(space) {
            Ok(Some(c)) if c.operation_id == operation => c,
            _ => {
                self.net.mls.mark_group_dirty(space.to_string());
                return;
            }
        };
        let records = match result {
            Ok(records) => {
                claim.records = Some(records.clone());
                if let Err(error) = self.save_claim(space, &claim) {
                    tracing_like(&format!("saving key-package claim in {space}: {error}"));
                    self.net
                        .mls
                        .retry_at
                        .insert(space.to_string(), Instant::now() + CLAIM_RETRY);
                    return;
                }
                records
            }
            Err(error) => {
                if error == KEY_PACKAGE_CLAIM_EXPIRED {
                    let _ = self.store.meta_delete(&claim_meta(space));
                }
                tracing_like(&format!("key-package claim in {space}: {error}"));
                self.net
                    .mls
                    .retry_at
                    .insert(space.to_string(), Instant::now() + CLAIM_RETRY);
                return;
            }
        };
        let owed = match self.owed(space) {
            Ok(Some(o)) => o,
            _ => {
                self.net.mls.mark_group_dirty(space.to_string());
                return;
            }
        };
        if owed != claim.intent {
            // A membership change while the reply was in flight is a new operation.
            self.net.mls.mark_group_dirty(space.to_string());
            return;
        }
        let Owed {
            add,
            add_devices,
            remove,
        } = owed;
        // Only packages whose verified leaf is one of the people (or devices) we asked for,
        // and never a device whose old leaf is still in (it goes first).
        let records: Vec<KeyPackageRecord> = records
            .into_iter()
            .filter(|r| {
                ::mls::key_package_leaf(&r.data).is_ok_and(|l: Leaf| {
                    l.identity == r.identity
                        && l.device == r.device
                        && claim.targets.contains(&l.identity)
                        && (add.contains(&l.identity)
                            || add_devices.contains(&(l.identity.clone(), l.device.clone())))
                        && !remove.contains(&leaf_name(&l.identity, &l.device))
                })
            })
            .collect();
        let found: BTreeSet<&IdentityId> = records.iter().map(|r| &r.identity).collect();
        if add
            .iter()
            .any(|who| claim.targets.contains(who) && !found.contains(who))
        {
            // A group may continue for its existing readers while a newcomer has no
            // packages. Direct chats still wait for all identities in ready_epoch_in.
            self.net.mls.stuck.insert(space.to_string());
        }
        let devices_found: BTreeSet<(&str, &str)> = records
            .iter()
            .map(|r| (r.identity.as_str(), r.device.as_str()))
            .collect();
        let devices_missing = add_devices.iter().any(|(id, dev)| {
            claim.targets.contains(id) && !devices_found.contains(&(id.as_str(), dev.as_str()))
        });
        let packages: Vec<Vec<u8>> = records.into_iter().map(|r| r.data).collect();
        if packages.is_empty() && remove.is_empty() {
            // A recorded empty response will never acquire a newly published package.
            let _ = self.store.meta_delete(&claim_meta(space));
            self.net
                .mls
                .retry_at
                .insert(space.to_string(), Instant::now() + CLAIM_RETRY);
            return;
        }
        if let Err(e) = self.commit_with_claim(space, &packages, &remove, Some(operation)) {
            tracing_like(&format!("commit in {space}: {e}"));
            self.net
                .mls
                .retry_at
                .insert(space.to_string(), Instant::now() + CLAIM_RETRY);
        } else if devices_missing {
            // A joining device with no key packages yet (it hasn't come online): ask again.
            self.net
                .mls
                .retry_at
                .insert(space.to_string(), Instant::now() + CLAIM_RETRY);
        }
    }

    pub(crate) fn commit_now(
        &mut self,
        space: &str,
        add: &[Vec<u8>],
        remove: &BTreeSet<IdentityId>,
    ) -> R<()> {
        self.commit_with_claim(space, add, remove, None)
    }

    pub(super) fn commit_with_claim(
        &mut self,
        space: &str,
        add: &[Vec<u8>],
        remove: &BTreeSet<IdentityId>,
        operation: Option<&str>,
    ) -> R<()> {
        let recipients = self.recovery_recipients(space)?;
        let tx = self.store.conn().unchecked_transaction().map_err(storage)?;
        let c = self
            .device_on(&tx)?
            .commit(space, add, remove)
            .map_err(mls_err)?;
        let events = self.store_recovery_commit(space, c, recipients.as_deref())?;
        if let Some(welcome) = events.iter().find(
            |e| matches!(&e.body, EventBody::Sealed { kind } if kind == SealedKind::Welcome.name()),
        ) {
            let newcomers: BTreeSet<String> = add
                .iter()
                .filter_map(|kp| ::mls::key_package_leaf(kp).ok())
                .map(|l| leaf_name(&l.identity, &l.device))
                .collect();
            self.store
                .set_meta(&welcome_meta(&welcome.client_id), &join_set(&newcomers))?;
        }
        if let Some(operation) = operation {
            if self
                .load_claim(space)?
                .is_some_and(|c| c.operation_id == operation)
            {
                self.store.meta_delete(&claim_meta(space))?;
            }
        }
        tx.commit().map_err(storage)?;
        self.queued_recovery_commit(events);
        Ok(())
    }
}

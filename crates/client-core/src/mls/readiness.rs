//! MLS device, roster and ready-epoch projections.
use super::{
    fold_roster, fork_meta, mls_err, state_key, t, tracing_like, BTreeMap, BTreeSet, Connection,
    CoreError, Device, Engine, Event, EventBody, IdentityId, Privacy, Role, SealedKind, SpaceId,
    SpaceKind, META_PUBLISHED, R,
};

/// Closed readiness facts, shared with the normal sealing gate. No keys or IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadinessRejection {
    Credential,
    Group,
    Pending,
    Reconciliation,
    Storage,
    Welcome,
    Membership,
    Behind,
    Adding,
}

impl Engine {
    pub(crate) fn mls_work_observation(&self, space: &str) -> (bool, bool, bool) {
        (
            self.net.mls.dirty.contains(space),
            self.net.mls.claiming.contains(space),
            self.net.mls.retry_at.contains_key(space),
        )
    }
    /// Whether `space` is relay-ordered and end-to-end encrypted.
    pub(crate) fn is_e2e(&self, space: &str) -> bool {
        self.net.synced.contains(space)
            && self
                .state
                .spaces
                .get(space)
                .is_some_and(|s| s.privacy == Privacy::EndToEnd && s.kind != SpaceKind::Personal)
    }

    /// This device as an MLS client over `conn` (the store, or a transaction on it).
    pub(crate) fn device_on<'c>(&self, conn: &'c Connection) -> R<Device<'c>> {
        let (Some(author), Some(acct)) = (&self.net.author, &self.net.account) else {
            return Err(CoreError::Forbidden {
                reason: t("Entre na sua conta primeiro.", "Sign in first."),
            });
        };
        let secret = author.key.secret();
        Device::new(
            conn,
            state_key(&secret).map_err(storage)?,
            &acct.identity,
            secret,
            &acct.cert,
        )
        .map_err(mls_err)
    }

    pub(crate) fn device(&self) -> R<Device<'_>> {
        self.device_on(self.store.conn())
    }

    /// Creates this device's tables for MLS state. Runs at every open.
    pub(crate) fn migrate_mls(&mut self) -> R<()> {
        ::mls::migrate(self.store.conn_mut()).map_err(mls_err)
    }

    /// Rebuilds the membership cache from the verified logs and re-checks every group.
    pub(crate) fn reload_mls(&mut self) {
        let spaces: Vec<SpaceId> = self
            .net
            .synced
            .iter()
            .filter(|s| self.is_e2e(s))
            .cloned()
            .collect();
        self.net.mls.rosters.clear();
        for space in spaces {
            let roster = self
                .logs
                .get(&space)
                .map(|l| fold_roster(l.events().iter()))
                .unwrap_or_default();
            self.net.mls.rosters.insert(space.clone(), roster);
            if let Ok(Some(reason)) = self.store.meta(&fork_meta(&space)) {
                self.show_fork(&space, reason);
            }
            self.net.mls.mark_group_dirty(space);
        }
        let pending: Vec<SpaceId> = self.logs.keys().cloned().collect();
        for space in pending {
            let _ = self.settle_pending_prefixes(&space);
        }
    }

    pub(crate) fn mls_roster(&self, space: &str) -> Option<&BTreeMap<IdentityId, Role>> {
        self.net.mls.rosters.get(space)
    }

    /// The group's leaves as (identity, device), if this device is in it.
    pub(crate) fn mls_leaves(&self, space: &str) -> Option<BTreeSet<(IdentityId, String)>> {
        let d = self.device().ok()?;
        if !d.has_group(space) {
            return None;
        }
        d.leaves(space).ok().map(|l| l.into_keys().collect())
    }

    pub(crate) fn roster(&self, space: &str) -> BTreeSet<IdentityId> {
        self.net
            .mls
            .rosters
            .get(space)
            .map(|r| r.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// A membership event landed in an end-to-end Space's confirmed log.
    pub(crate) fn mls_membership_changed(&mut self, e: &Event) {
        if !self.is_e2e(&e.space) {
            return;
        }
        let roster = self.net.mls.rosters.entry(e.space.clone()).or_default();
        match &e.body {
            EventBody::SpaceCreated { .. } => {
                roster.insert(e.author.clone(), Role::Owner);
            }
            EventBody::MemberAdded { identity, role } => {
                roster.insert(identity.clone(), *role);
                self.net.mls.stuck.remove(&e.space);
            }
            EventBody::MemberRemoved { identity } => {
                roster.remove(identity);
                if self.me.as_deref() == Some(identity.as_str()) {
                    // The relay stops sending this Space here, so the commit removing this
                    // device never arrives: forget the group now.
                    if let Err(err) = self
                        .device()
                        .and_then(|d| d.forget(&e.space).map_err(mls_err))
                    {
                        tracing_like(&format!("forgetting the group of {}: {err}", e.space));
                    }
                }
            }
            EventBody::SpaceEncrypted => {
                *roster = self
                    .logs
                    .get(&e.space)
                    .map(|l| fold_roster(l.events().iter()))
                    .unwrap_or_default();
                if self.me.as_deref() == Some(e.author.as_str()) {
                    let started = self.device().and_then(|d| match d.has_group(&e.space) {
                        true => Ok(()),
                        false => d.create_group(&e.space).map_err(mls_err),
                    });
                    if let Err(err) = started {
                        tracing_like(&format!("starting the group of {}: {err}", e.space));
                    }
                }
            }
            _ => return,
        }
        self.net.mls.mark_group_dirty(e.space.clone());
    }

    /// The group for a Space this device just created (it's the only member).
    pub(crate) fn create_mls_group(&mut self, space: &str) -> R<()> {
        self.device()?.create_group(space).map_err(mls_err)?;
        self.net.mls.mark_group_dirty(space.to_string());
        Ok(())
    }

    /// The epoch to seal `space`'s messages at, once its group is ready for them.
    pub(crate) fn ready_epoch(&self, space: &str) -> Option<u64> {
        self.ready_epoch_in(space, &self.state)
    }

    pub(crate) fn ready_epoch_in(&self, space: &str, state: &crate::engine::State) -> Option<u64> {
        self.ready_epoch_checked(space, state).ok()
    }

    pub(crate) fn ready_epoch_checked(
        &self,
        space: &str,
        state: &crate::engine::State,
    ) -> std::result::Result<u64, ReadinessRejection> {
        use ReadinessRejection as E;
        let device = self.device().map_err(|_| E::Credential)?;
        match device.group_state(space).map_err(|_| E::Storage)? {
            ::mls::GroupState::Missing => return Err(E::Group),
            ::mls::GroupState::Pending => return Err(E::Pending),
            ::mls::GroupState::Ready => {}
        }
        if device.pending(space) {
            return Err(E::Pending);
        }
        if device.needs_reconciliation(space) {
            return Err(E::Reconciliation);
        }
        // The commit can land before its Welcome is accepted (e.g. under a publish
        // limit). The new member cannot open messages that overtake that Welcome.
        if self
            .store
            .outbox_handshakes()
            .map_err(|_| E::Storage)?
            .iter()
            .any(|(_, s, kind)| s == space && kind == SealedKind::Welcome.name())
        {
            return Err(E::Welcome);
        }
        let me = self.me.as_deref().ok_or(E::Membership)?;
        let s = state.spaces.get(space).ok_or(E::Membership)?;
        let group = device.roster(space).map_err(|_| E::Storage)?;
        // Nobody seals for a group that still holds someone the log removed: wait for the
        // commit that takes them out.
        if group.iter().any(|g| !s.members.iter().any(|(m, _)| m == g)) {
            return Err(E::Membership);
        }
        // An owner or admin about to add someone (including adds still on their way to
        // the relay) sends after the commit, so the newcomer reads it.
        let runs_it = s
            .members
            .iter()
            .any(|(m, r)| m == me && matches!(r, Role::Owner | Role::Admin));
        let m = &self.net.mls;
        let epoch = device.epoch(space).map_err(|_| E::Storage)?;
        if m.behind.get(space) == Some(&epoch) {
            return Err(E::Behind);
        }
        let missing_member = s.members.iter().any(|(who, _)| !group.contains(who));
        // An unavailable package must not turn a direct message into a message only
        // its sender can open. A later Welcome cannot recover that earlier epoch.
        // Extra devices of an identity already in the group do not change this gate.
        if s.kind == SpaceKind::Direct && missing_member {
            return Err(E::Membership);
        }
        let adding = m.claiming.contains(space) || missing_member;
        if runs_it && adding && !m.stuck.contains(space) {
            return Err(E::Adding);
        }
        device.epoch(space).map_err(|_| E::Storage)
    }

    /// Nothing MLS-side left to do right now (the CLI waits for this before exiting).
    pub fn mls_settled(&self) -> bool {
        let m = &self.net.mls;
        let published = self.net.account.as_ref().is_none_or(|a| {
            self.store.meta(META_PUBLISHED).ok().flatten().as_deref() == Some(a.device.as_str())
        });
        published
            && m.publishing.is_none()
            && m.top_up == 0
            && m.dirty.is_empty()
            && m.claiming.is_empty()
            && m.retry_at.is_empty()
            && m.turn_at.is_empty()
            && m.checkpoint_due.is_empty()
            && m.rejoin.is_empty()
            && !m.has_recovery_work()
    }

    /// The group as this device has it: epoch, checkpoint digest and members.
    pub fn mls_status(&self, space: &str) -> Option<(u64, String, Vec<IdentityId>)> {
        let device = self.device().ok()?;
        let (epoch, digest) = device.checkpoint(space).ok()?;
        let members = device.roster(space).ok()?.into_iter().collect();
        Some((epoch, digest, members))
    }
}

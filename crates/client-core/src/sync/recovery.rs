//! Confirmed-history reload and optimistic approval overlays.

use super::*;

impl Engine {
    /// Logs from an older event format can't be re-verified under the current one: start over (pre-launch only; ADR 0010).
    pub(crate) fn migrate_event_format(&mut self) -> R<()> {
        let fmt = self.store.meta(EVENT_FORMAT_META)?;
        if fmt.as_deref() != Some(EVENT_FORMAT_VALUE) {
            if self.store.event_count()? > 0 {
                self.store.wipe()?;
                self.store.wipe_sync()?;
            }
            self.store.set_meta(EVENT_FORMAT_META, EVENT_FORMAT_VALUE)?;
        }
        Ok(())
    }

    /// Part of `reload`: account, relay-ordered Spaces and the outbox on top.
    pub(crate) fn reload_sync(&mut self) -> R<()> {
        self.net.account = match self.store.meta("account")? {
            Some(j) => serde_json::from_str(&j).ok(),
            None => None,
        };
        if self.net.account.as_ref().map(|a| a.identity.as_str())
            != self.net.author.as_ref().map(|a| a.identity.as_str())
        {
            self.net.author = None;
        }
        self.net.synced = self.store.synced_spaces()?.into_iter().collect();
        self.net.pending.clear();
        self.net.optimistic_approvals.clear();
        self.net.failed.clear();
        self.net.profiles.forget_shared();

        let confirmed: HashSet<_> = self
            .net
            .synced
            .iter()
            .filter_map(|s| self.logs.get(s))
            .flat_map(|l| l.events().iter().map(PendingCommitment::of))
            .collect();
        let outbox = self.store.outbox()?;
        for p in outbox {
            let e = p.event;
            if confirmed.contains(&PendingCommitment::of(&e)) {
                // Crashed between storing the confirmed copy and clearing the outbox.
                self.store.outbox_remove(&e.client_id)?;
                continue;
            }
            if p.failed {
                // Only refused messages stay visible (with a retry/delete affordance).
                if is_message_send(&e.body) && self.space_order.contains(&e.space) {
                    self.net
                        .failed
                        .insert(e.client_id.clone(), p.last_error.unwrap_or_default());
                }
                continue;
            }
            if matches!(e.body, EventBody::SpaceCreated { .. })
                && !self.space_order.contains(&e.space)
            {
                self.space_order.push(e.space.clone());
            }
            self.net
                .pending
                .insert(e.client_id.clone(), e.space.clone());
            if matches!(e.body, EventBody::RequestResolved { .. }) {
                self.net.optimistic_approvals.insert(e.space.clone());
            }
        }
        self.reproject_approval_overlay()?;
        self.restore_peer_receipts()?;
        self.recompute_unknown();
        self.reload_mls();
        Ok(())
    }

    pub(crate) fn approval_projection_needs_replay(&self, e: &Event) -> bool {
        if self.can_append_ordinary_page(e) {
            return false;
        }
        self.approval_replay_needed
            || self.state.waiting_spaces.contains(&e.space)
            || matches!(
                e.body,
                EventBody::RequestOpened { .. }
                    | EventBody::RequestResolved { .. }
                    | EventBody::MemberAdded { .. }
                    | EventBody::MemberRemoved { .. }
                    | EventBody::GrantIssued { .. }
                    | EventBody::GrantRevoked { .. }
                    | EventBody::ItemCreated { .. }
                    | EventBody::ItemVersioned { .. }
                    | EventBody::ItemReverted { .. }
            )
    }

    /// A fresh ordinary page cannot invalidate a previous decision when there is
    /// no decision or optimistic/recovery overlay. Fold only its authenticated
    /// frontier through the SAME checked projector, as other incremental events do.
    /// A missing/corrupt outbox read must retain the original replay/failure path.
    fn can_append_ordinary_page(&self, event: &Event) -> bool {
        matches!(&event.body, EventBody::ItemCreated {
            item,
            kind: ItemKind::Page,
            content: ItemContent::Page(PageContent { manifest: None, .. }),
            ..
        } if !self.state.items.contains_key(item))
            && !self.approval_replay_needed
            && self.state.waiting_spaces.is_empty()
            && self.state.requests.is_empty()
            && self.net.optimistic_approvals.is_empty()
            // Include failed rows: they can reserve visible recovery state even
            // though the transport's pending count intentionally excludes them.
            && self.store.outbox().is_ok_and(|rows| rows.is_empty())
    }

    pub(crate) fn apply_confirmed_event(&mut self, event: &Event) -> R<Projection> {
        let end = self
            .logs
            .get(&event.space)
            .and_then(|log| log.len().checked_sub(1))
            .ok_or_else(|| CoreError::Invalid {
                reason: "missing confirmed frontier".into(),
            })?;
        let scoped = self
            .space_projections
            .entry(event.space.clone())
            .or_default();
        let context = PlanReplay::new(&self.store, &self.logs).before(
            scoped,
            &self.identities,
            event,
            end,
        )?;
        let outcome = scoped.apply_checked(event, &self.identities, &context);
        self.state = Self::merge_space_projections(&self.space_projections, &self.space_order);
        Ok(outcome)
    }

    /// Rebuild authority into temporary state before publishing an optimistic overlay.
    /// Use the verified log cache; do not reload keys, MLS state or open page sessions.
    pub(crate) fn reproject_approval_overlay(&mut self) -> R<()> {
        self.approval_replay_needed = true;
        let mut order = self.store.space_ids()?;
        let pending = self.store.outbox()?;
        // These source events stay in the one existing outbox. References below are
        // display-only recovery facts, never a second durable or writer authority.
        let sources: HashMap<_, _> = pending
            .iter()
            .filter_map(|row| match &row.event.body {
                EventBody::ItemCreated {
                    item,
                    kind: types::ItemKind::File,
                    ..
                } => Some((
                    (
                        row.event.space.as_str(),
                        item.as_str(),
                        row.event.author.as_str(),
                    ),
                    &row.event,
                )),
                _ => None,
            })
            .collect();
        let mut recovery_sources: HashMap<&str, Vec<(&Event, &Event)>> = HashMap::new();
        if let Some(local) = self.me.as_deref() {
            for row in pending.iter().filter(|row| row.failed) {
                if let EventBody::MessagePosted {
                    attaches: Some(item),
                    ..
                } = &row.event.body
                {
                    if let Some(source) =
                        sources.get(&(row.event.space.as_str(), item.as_str(), local))
                    {
                        recovery_sources
                            .entry(row.event.space.as_str())
                            .or_default()
                            .push((&row.event, source));
                    }
                }
            }
        }
        let mut projections = HashMap::new();
        let mut confirmed = HashSet::new();
        let mut plans = PlanReplay::new(&self.store, &self.logs);
        for space in &order {
            let mut scoped = State::default();
            if let (Some(local), Some(recovery)) =
                (self.me.as_deref(), recovery_sources.get(space.as_str()))
            {
                for (event, source) in recovery {
                    scoped.reserve_failed_attachment(event, local, source);
                }
            }
            if let Some(log) = self.logs.get(space) {
                for (end, e) in log.events().iter().enumerate() {
                    #[cfg(test)]
                    super::replay_probe::folded();
                    confirmed.insert(PendingCommitment::of(e));
                    let context = plans.before(&scoped, &self.identities, e, end)?;
                    scoped.apply_checked(e, &self.identities, &context);
                }
            } else if let Some(broken) = self.broken_spaces.get(space) {
                let mut broken = broken.clone();
                broken.entries.clear();
                scoped.spaces.insert(space.clone(), broken);
            }
            projections.insert(space.clone(), scoped);
        }
        let mut optimistic = HashSet::new();
        for p in &pending {
            let e = &p.event;
            if confirmed.contains(&PendingCommitment::of(e)) {
                continue;
            }
            if p.failed {
                if is_message_send(&e.body)
                    && projections
                        .get(&e.space)
                        .is_some_and(|scoped| scoped.spaces.contains_key(&e.space))
                {
                    let scoped = projections.get_mut(&e.space).expect("checked above");
                    let retained = if let EventBody::MessagePosted {
                        attaches: Some(item),
                        ..
                    } = &e.body
                    {
                        self.me.as_deref().is_some_and(|local| {
                            scoped.retain_failed_attachment(
                                e,
                                local,
                                sources
                                    .get(&(e.space.as_str(), item.as_str(), local))
                                    .copied(),
                            )
                        })
                    } else {
                        false
                    };
                    // Preserve normal replay for text and canonical non-File
                    // cards. Recovery reservations still fence the checked fold.
                    if !retained && matches!(e.body, EventBody::MessagePosted { media: None, .. }) {
                        let context = plans.pending(scoped, &self.identities, e)?;
                        scoped.apply_checked(e, &self.identities, &context);
                    } else if !matches!(e.body, EventBody::MessagePosted { .. }) {
                        scoped.retain_failed_voice(e);
                    }
                }
                continue;
            }
            if matches!(e.body, EventBody::SpaceCreated { .. }) && !order.contains(&e.space) {
                order.push(e.space.clone());
            }
            if matches!(e.body, EventBody::RequestResolved { .. }) {
                optimistic.insert(e.space.clone());
            }
            let scoped = projections.entry(e.space.clone()).or_default();
            let context = plans.pending(scoped, &self.identities, e)?;
            scoped.apply_checked(e, &self.identities, &context);
        }
        let state = Self::merge_space_projections(&projections, &order);
        self.state = state;
        self.space_projections = projections;
        self.space_order = order;
        self.net.optimistic_approvals = optimistic;
        self.approval_replay_needed = false;
        self.index_dirty = true;
        // The rebuild starts from an empty projection, so a stored fork has to
        // be written back before the next send.
        self.restore_recorded_forks();
        self.recompute_unknown();
        Ok(())
    }
}

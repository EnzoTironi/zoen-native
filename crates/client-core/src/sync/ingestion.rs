//! Authenticated relay admission and confirmation of pending intents.

use super::*;

impl Engine {
    /// What a new event in `space` says its author had seen: the relay-ordered head, or,
    /// for a Space this device created and the relay hasn't confirmed yet, the genesis link
    /// it will get (the creator's `SpaceCreated` always lands at seq 0, so its chain hash is
    /// known before it's sent).
    pub fn causal_head(&self, space: &str) -> R<Option<Seen>> {
        if let Some(head) = self.logs.get(space).and_then(SpaceLog::head) {
            return Ok(Some(head));
        }
        let genesis = self
            .store
            .outbox()?
            .into_iter()
            .map(|p| p.event)
            .find(|e| e.space == space && matches!(e.body, EventBody::SpaceCreated { .. }));
        Ok(genesis.map(|g| Seen {
            seq: 0,
            hash: chain_hash(space, 0, GENESIS_PREV, &content_hash_of(&g)),
        }))
    }

    /// Writes to a relay-ordered Space: sign, queue, project.
    pub(crate) fn append_synced(
        &mut self,
        space: &str,
        signer: &Author,
        at_ms: i64,
        body: EventBody,
    ) -> R<Event> {
        if self.approval_replay_needed {
            self.reproject_approval_overlay()?;
        }
        let creating = matches!(body, EventBody::SpaceCreated { .. });
        if !creating && !self.state.spaces.contains_key(space) {
            return Err(CoreError::NotFound {
                what: t("Espaço", "Space"),
            });
        }
        let client_id = new_ulid(at_ms);
        let seen = if creating {
            None
        } else {
            self.causal_head(space)?
        };
        let e = signer.sign_event(space, &client_id, at_ms, seen, body);
        let empty = State::default();
        let scoped = self.space_projections.get(space).unwrap_or(&empty);
        let context =
            PlanReplay::new(&self.store, &self.logs).pending(scoped, &self.identities, &e)?;
        if scoped.approval_event_valid(&e, &self.identities, &context) != Projection::Applied {
            return Err(CoreError::Invalid {
                reason: t(
                    "pedido, decisão ou uso não autorizado",
                    "unauthorized request, decision or usage",
                ),
            });
        }
        self.store.outbox_put(&e)?;
        if creating && !self.space_order.contains(&space.to_string()) {
            self.space_order.push(space.to_string());
        }
        self.net.pending.insert(client_id, space.to_string());
        if matches!(
            e.body,
            EventBody::RequestResolved { .. } | EventBody::ToolReviewRequested { .. }
        ) {
            self.net.optimistic_approvals.insert(space.to_string());
        }
        self.space_projections
            .entry(space.to_owned())
            .or_default()
            .apply_checked(&e, &self.identities, &context);
        if self.approval_replay_needed
            || !self.state.waiting_spaces.is_empty()
            || !self.state.requests.is_empty()
            || !matches!(e.body, EventBody::ItemCreated { .. })
        {
            self.state = Self::merge_space_projections(&self.space_projections, &self.space_order);
        } else {
            self.state.apply_checked(&e, &self.identities, &context);
        }
        self.index_dirty = true;
        self.note_unknown(&e);
        self.note_profile_event(&e);
        self.net.wake();
        Ok(e)
    }

    /// An event the relay sequenced.
    pub fn ingest(&mut self, ev: Sequenced) -> Ingest {
        if ev.env.is_sealed() {
            return self.ingest_sealed(ev);
        }
        let Some(e) = ev.plain_event() else {
            return Ingest::Invalid(t("evento ilegível", "unreadable event"));
        };
        let r = self.ingest_plain(e.clone());
        if matches!(r, Ingest::Applied | Ingest::Confirmed) {
            self.mls_membership_changed(&e);
            if let Err(err) = self.compare_checkpoint(&e) {
                return Ingest::Invalid(err.to_string());
            }
            self.mls_device_joining(&e);
        }
        r
    }

    pub(crate) fn pending_for_confirmation(&self, space: &str, id: &str) -> R<Option<Event>> {
        if self.net.pending.get(id).map(String::as_str) != Some(space) {
            return Ok(None);
        }
        // A read failure must not become either a guessed receipt or a removal.
        Ok(self
            .store
            .outbox_get(id)?
            .filter(|pending| pending.space == space))
    }

    pub(super) fn ingest_plain(&mut self, e: Event) -> Ingest {
        let space = e.space.clone();
        let next = self.logs.get(&space).map(|l| l.next_seq()).unwrap_or(0);
        if e.seq < next {
            if self.approval_replay_needed {
                if let Err(error) = self.reproject_approval_overlay() {
                    return Ingest::Invalid(error.to_string());
                }
            }
            return Ingest::Duplicate;
        }
        if e.seq > next {
            return Ingest::Gap { next };
        }
        let own = match self.pending_for_confirmation(&space, &e.client_id) {
            Ok(pending) => pending
                .as_ref()
                .is_some_and(|pending| PendingCommitment::of(pending) == PendingCommitment::of(&e)),
            Err(error) => return Ingest::Invalid(error.to_string()),
        };
        let accept_err = {
            let log = self
                .logs
                .entry(space.clone())
                .or_insert_with(|| SpaceLog::new(space.clone()));
            log.accept(e.clone()).err()
        };
        if let Some(err) = accept_err {
            return self.refuse_unaccepted(&space, &err, &e);
        }
        // The confirmed event, its catch-up scope and an own pending copy describe
        // one receipt. Never publish a confirmation whose bookkeeping failed.
        let stored = (|| -> crate::engine::R<()> {
            let tx = self
                .store
                .conn()
                .unchecked_transaction()
                .map_err(store::StoreError::from)?;
            self.store.append_event(&e)?;
            self.store.set_synced(&space)?;
            if own {
                self.confirm_owner_policy_welcome(&e)?;
                self.store.outbox_remove(&e.client_id)?;
            }
            tx.commit().map_err(store::StoreError::from)?;
            Ok(())
        })();
        if let Err(err) = stored {
            // Rollback precedes reload, so memory sees the same pending/history state.
            let _ = self.reload();
            return Ingest::Invalid(err.to_string());
        }
        self.net.synced.insert(space.clone());
        if !self.space_order.contains(&space) {
            self.space_order.push(space.clone());
        }
        let replay_approval = self.approval_projection_needs_replay(&e);
        if own {
            self.net.pending.remove(&e.client_id);
        }
        if replay_approval {
            if own && self.confirmed_item_already_projected(&e) {
                if !self.note_own_confirmed(&space, &e.client_id, e.seq, &e.hash) {
                    self.state =
                        Self::merge_space_projections(&self.space_projections, &self.space_order);
                }
                return Ingest::Confirmed;
            }
            if let Err(err) = self.reproject_approval_overlay() {
                return Ingest::Invalid(err.to_string());
            }
            self.note_profile_event(&e);
            return if own {
                Ingest::Confirmed
            } else {
                Ingest::Applied
            };
        }
        if own {
            let joining = matches!(e.body, EventBody::MemberAdded { .. })
                && !self.is_member_of(&space, &e.author);
            let had = self.note_own_confirmed(&space, &e.client_id, e.seq, &e.hash);
            if had && !joining {
                return Ingest::Confirmed;
            }
            self.state = Self::merge_space_projections(&self.space_projections, &self.space_order);
            // A join we sent before we had the Space's history: project it now.
            if !self.state.spaces.contains_key(&space) || joining {
                if let Err(err) = self.apply_confirmed_event(&e) {
                    return Ingest::Invalid(err.to_string());
                }
            }
            return Ingest::Confirmed;
        }
        if let Err(err) = self.apply_confirmed_event(&e) {
            return Ingest::Invalid(err.to_string());
        }
        self.index_dirty = true;
        self.note_unknown(&e);
        self.note_profile_event(&e);
        Ingest::Applied
    }

    /// True when this own item is already projected and no approval overlay is in flight.
    pub(crate) fn confirmed_item_already_projected(&self, e: &Event) -> bool {
        if self.approval_replay_needed
            || !self.state.waiting_spaces.is_empty()
            || !self.net.optimistic_approvals.is_empty()
            || !self.state.requests.is_empty()
        {
            return false;
        }
        matches!(
            e.body,
            EventBody::ItemCreated { .. }
                | EventBody::ItemVersioned { .. }
                | EventBody::ItemReverted { .. }
        )
    }

    /// Writes the relay seq onto the optimistic entry. Returns whether the Space is on screen.
    pub(crate) fn note_own_confirmed(
        &mut self,
        space: &str,
        client_id: &str,
        seq: u64,
        hash: &str,
    ) -> bool {
        let stamp = |entries: &mut Vec<Entry>| {
            for entry in entries.iter_mut().filter(|entry| {
                entry.hash.is_empty()
                    && (entry.client_id == client_id
                        || entry.client_id.strip_suffix(":result") == Some(client_id))
            }) {
                entry.seq = seq;
                entry.hash = hash.to_string();
            }
        };
        if let Some(space_state) = self
            .space_projections
            .get_mut(space)
            .and_then(|scoped| scoped.spaces.get_mut(space))
        {
            stamp(&mut space_state.entries);
        }
        let Some(space_state) = self.state.spaces.get_mut(space) else {
            return false;
        };
        stamp(&mut space_state.entries);
        true
    }

    pub(super) fn is_member_of(&self, space: &str, who: &str) -> bool {
        self.state
            .spaces
            .get(space)
            .map(|s| s.members.iter().any(|(m, _)| m == who))
            .unwrap_or(false)
    }
}

//! Atomic authenticated sealed/pruned event ingestion.
use super::*;

impl Engine {
    /// A sealed entry the relay sequenced: opened (or kept sealed) and logged in one
    /// transaction with the group state it changes.
    pub(crate) fn ingest_sealed(&mut self, ev: Sequenced) -> Ingest {
        let space = ev.env.space().to_string();
        let next = self.logs.get(&space).map(|l| l.next_seq()).unwrap_or(0);
        if ev.seq < next {
            if self.approval_replay_needed {
                if let Err(error) = self.reproject_approval_overlay() {
                    return Ingest::Invalid(error.to_string());
                }
            }
            return Ingest::Duplicate;
        }
        if ev.seq > next {
            return Ingest::Gap { next };
        }
        // Opened/own events persist their inner signature. Authenticate the outer
        // envelope too before replacing it with that inner event or changing MLS state.
        if let Err(err) = ev.env.verify() {
            return Ingest::Invalid(err.to_string());
        }
        if ev.env.is_pruned() {
            return self.ingest_pruned(ev);
        }
        let Some((kind, data)) = ev.env.sealed_data().map(|(k, d)| (k, d.to_vec())) else {
            return Ingest::Invalid("not sealed".into());
        };
        let client_id = ev.env.client_id().to_string();
        let own_account = self.net.account.as_ref().is_some_and(|account| {
            account.identity == ev.env.author() && ev.env.device() == Some(account.device.as_str())
        });
        let pending = if own_account {
            match self.pending_for_confirmation(&space, &client_id) {
                Ok(pending) => pending,
                Err(error) => return Ingest::Invalid(error.to_string()),
            }
        } else {
            None
        };
        let own_pending = pending.filter(|pending| {
            pending.author == ev.env.author()
                && pending.device.as_deref() == ev.env.device()
                && pending.at_ms == ev.env.at_ms()
                && pending.seen == ev.env.seen()
                && if kind == SealedKind::Application {
                    // A valid older publication can arrive after maintenance
                    // resealed this same signed intent. Its authenticated author,
                    // device and header bind the original pending application;
                    // the current ciphertext cache is not an ACK authority.
                    must_seal(&pending.body) && !matches!(&pending.body, EventBody::Sealed { .. })
                } else {
                    pending.content == ev.env.content()
                }
        });
        let own = own_pending.is_some();
        let roster = self.roster(&space);
        let admitted_sender = ::mls::Leaf {
            identity: ev.env.author().into(),
            device: ev.env.device().unwrap_or(ev.env.author()).into(),
            cert: ev.env.cert.clone().unwrap_or_default(),
        };
        let own_copy = if own && kind == SealedKind::Application {
            own_pending
        } else {
            None
        };

        let conn = self.store.conn();
        let tx = match conn.unchecked_transaction() {
            Ok(tx) => tx,
            Err(e) => return Ingest::Invalid(e.to_string()),
        };
        let device = match self.device_on(&tx) {
            Ok(d) => d,
            Err(e) => return Ingest::Invalid(e.to_string()),
        };
        let mut opened: Option<Event> = own_copy;
        let mut opening = if opened.is_some() {
            crate::mls_opening::OpeningOutcome::OwnRetainedEcho
        } else {
            crate::mls_opening::OpeningOutcome::Opaque
        };
        let mut landed: Option<(u64, bool)> = None; // (epoch, joined)
        match kind {
            SealedKind::Application if opened.is_none() => {
                match device.open(&space, &data, &roster, &admitted_sender) {
                    Ok(Opened::Application { plaintext, from }) => {
                        match inner_event(&ev, &plaintext, &from) {
                            Some(e) => {
                                opening =
                                    crate::mls_opening::OpeningOutcome::from_application(&e, &from);
                                opened = Some(e);
                            }
                            None => tracing_like(&format!(
                                "sealed message in {space} doesn't match its envelope; kept sealed"
                            )),
                        }
                    }
                    Ok(_) => {}
                    Err(e @ ::mls::MlsError::Storage(_)) => {
                        return Ingest::Invalid(e.to_string());
                    }
                    Err(e) => tracing_like(&format!("can't open a message in {space}: {e}")),
                }
            }
            SealedKind::Application => {}
            SealedKind::Commit => match if device.recovery_pending(&space) && own {
                device.confirm_recovery(&space, &data, &roster)
            } else {
                // A winning external commit can replace a tentative group at the same
                // epoch. Discard the loser before retaining the authenticated new context.
                if device.recovery_pending(&space) {
                    if let Err(err) = device.abandon_recovery(&space) {
                        return Ingest::Invalid(err.to_string());
                    }
                }
                device.open(&space, &data, &roster, &admitted_sender)
            } {
                Ok(Opened::Commit { epoch } | Opened::Reconcile { epoch }) => {
                    landed = Some((epoch, false))
                }
                Ok(_) => {}
                Err(e)
                    if own && device.recovery_pending(&space)
                        || matches!(e, ::mls::MlsError::Storage(_)) =>
                {
                    return Ingest::Invalid(e.to_string());
                }
                Err(e) => tracing_like(&format!("refused a commit in {space}: {e}")),
            },
            SealedKind::Welcome if !device.has_group(&space) => {
                match device.join(&space, &data, &roster) {
                    Ok(true) => match device.epoch(&space) {
                        Ok(epoch) => landed = Some((epoch, true)),
                        Err(e @ ::mls::MlsError::Storage(_)) => {
                            return Ingest::Invalid(e.to_string());
                        }
                        Err(e) => {
                            tracing_like(&format!("can't load a joined group in {space}: {e}"))
                        }
                    },
                    Ok(false) => {}
                    Err(e @ ::mls::MlsError::Storage(_)) => {
                        return Ingest::Invalid(e.to_string());
                    }
                    Err(e) => tracing_like(&format!("refused a welcome in {space}: {e}")),
                }
            }
            SealedKind::Welcome | SealedKind::Unspecified => {}
        }
        let digest = if landed.is_some() {
            match device.checkpoint(&space) {
                Ok(checkpoint) => Some(checkpoint),
                Err(e @ ::mls::MlsError::Storage(_)) => return Ingest::Invalid(e.to_string()),
                Err(_) => None,
            }
        } else {
            None
        };
        drop(device);

        let e = match opened {
            Some(mut inner) => {
                inner.seq = ev.seq;
                inner.prev = ev.prev.clone();
                inner.hash = ev.hash.clone();
                inner.sealed_wire = Some(ev.env.wire_hash());
                inner
            }
            None => match event_from_content(
                ev.env.content().to_vec(),
                ev.env.sig.clone(),
                ev.env.cert.clone(),
                ev.seq,
                ev.prev.clone(),
                ev.hash.clone(),
            ) {
                Ok(e) => e,
                Err(err) => return Ingest::Invalid(err.to_string()),
            },
        };
        let accept_err = {
            let log = self
                .logs
                .entry(space.clone())
                .or_insert_with(|| SpaceLog::new(space.clone()));
            log.accept(e.clone()).err()
        };
        if let Some(err) = accept_err {
            // The fork write uses this connection. It has to follow the rollback,
            // or the refused event takes the fork meta with it.
            drop(tx);
            return self.refuse_unaccepted(&space, &err, &e);
        }
        let mut stored = self.store.append_event(&e).map_err(storage);
        stored = stored
            .and_then(|_| crate::mls_opening::record(&self.store, &ev, opening).map_err(storage));
        stored = stored.and_then(|_| self.remember_recovery_context(&ev.env));
        if own && kind == SealedKind::Commit {
            if let Some(reference) = ev
                .env
                .recovery()
                .and_then(event_log::recovery::RecoveryRef::parse)
            {
                stored = stored.and_then(|_| {
                    self.store
                        .clear_recovery_upload(&reference.blob)
                        .map_err(storage)
                });
            }
        }
        if let Some((epoch, digest)) = &digest {
            stored = stored.and_then(|_| {
                self.store
                    .set_meta(&digest_meta(&space, *epoch), digest)
                    .map_err(storage)
            });
        }
        if own {
            stored = stored.and_then(|_| {
                self.store.outbox_remove(&client_id).map_err(storage)?;
                self.store
                    .meta_delete(&sealed_meta(&client_id))
                    .map_err(storage)?;
                self.store
                    .meta_delete(&welcome_meta(&client_id))
                    .map_err(storage)
            });
        }
        stored = stored.and_then(|_| self.store.set_synced(&space).map_err(storage));
        if let Err(err) = stored.and_then(|_| tx.commit().map_err(storage)) {
            let _ = self.reload();
            return Ingest::Invalid(err.to_string());
        }
        if let Err(err) = self.compare_checkpoint(&e) {
            return Ingest::Invalid(err.to_string());
        }

        self.net.synced.insert(space.clone());
        let m = &mut self.net.mls;
        if landed.is_some() {
            m.mark_group_dirty(space.clone());
        }
        if matches!(landed, Some((_, true))) || (own && landed.is_some()) {
            m.checkpoint_due.insert(space.clone());
        }
        if kind == SealedKind::Commit && landed.is_none() {
            // Refused or stale: the group may still owe this Space a commit.
            m.mark_group_dirty(space.clone());
        }
        if e.seq >= self.checkpointed_at(&space) + checkpoint_every()
            && self.device().is_ok_and(|d| d.has_group(&space))
        {
            self.net.mls.checkpoint_due.insert(space.clone());
        }

        if self.approval_projection_needs_replay(&e) {
            if own && self.confirmed_item_already_projected(&e) {
                self.net.pending.remove(&client_id);
                if !self.note_own_confirmed(&space, &client_id, e.seq, &e.hash) {
                    self.state =
                        Self::merge_space_projections(&self.space_projections, &self.space_order);
                }
                return Ingest::Confirmed;
            }
            if own {
                self.net.pending.remove(&client_id);
            }
            if let Err(err) = self.reproject_approval_overlay() {
                return Ingest::Invalid(err.to_string());
            }
            return if own {
                Ingest::Confirmed
            } else {
                Ingest::Applied
            };
        }

        if own {
            self.net.pending.remove(&client_id);
            if kind == SealedKind::Welcome {
                // The group may owe a commit that waited for this Welcome to land.
                self.net.mls.mark_group_dirty(space.clone());
            }
            if !self.note_own_confirmed(&space, &client_id, e.seq, &e.hash) {
                self.state =
                    Self::merge_space_projections(&self.space_projections, &self.space_order);
            }
            return Ingest::Confirmed;
        }
        if let Err(err) = self.apply_confirmed_event(&e) {
            return Ingest::Invalid(err.to_string());
        }
        self.index_dirty = true;
        self.note_unknown(&e);
        Ingest::Applied
    }

    /// What a pruned entry leaves (ADR 0026): only a device that joined later reads it, and
    /// it predates its Welcome, so there is nothing to open. It keeps the chain whole: the
    /// stub hashes and verifies like the original (header plus the MLS bytes' hash).
    pub(super) fn ingest_pruned(&mut self, ev: Sequenced) -> Ingest {
        let space = ev.env.space().to_string();
        let mut e = match event_from_content(
            ev.env.content().to_vec(),
            ev.env.sig.clone(),
            ev.env.cert.clone(),
            ev.seq,
            ev.prev.clone(),
            ev.hash.clone(),
        ) {
            Ok(e) => e,
            Err(err) => return Ingest::Invalid(err.to_string()),
        };
        e.sealed_wire = ev.env.legacy_pruned_hash().map(str::to_string);
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
        // No MLS state changes until the stub's signature and chain are accepted.
        // Forgetting a stale group and persisting the verified stub commit together.
        let stored = (|| -> R<bool> {
            let tx = self.store.conn().unchecked_transaction().map_err(storage)?;
            let forgot = match self.device_on(&tx) {
                Ok(device) if device.has_group(&space) => {
                    device.forget(&space).map_err(mls_err)?;
                    true
                }
                _ => false,
            };
            self.store.append_event(&e).map_err(storage)?;
            self.remember_recovery_context(&ev.env)?;
            self.store.set_synced(&space).map_err(storage)?;
            tx.commit().map_err(storage)?;
            Ok(forgot)
        })();
        let forgot = match stored {
            Ok(forgot) => forgot,
            Err(err) => {
                let _ = self.reload();
                return Ingest::Invalid(err.to_string());
            }
        };
        if forgot {
            // History this device never fetched is gone; a fresh Welcome is needed.
            self.net.mls.rejoin.insert(space.clone());
        }
        self.net.mls.wake_recovery(&space);
        self.net.synced.insert(space);
        if let Err(err) = self.compare_checkpoint(&e) {
            return Ingest::Invalid(err.to_string());
        }
        Ingest::Applied
    }
}

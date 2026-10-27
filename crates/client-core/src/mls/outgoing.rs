//! Retained outgoing ciphertext, handshakes and stale-epoch fences.
use super::{
    event_from_content, mls_err, must_seal, new_ulid, sealed_meta, storage, t, CoreError, Engine,
    Envelope, Event, EventBody, HashMap, HashSet, InnerEvent, Sealed, SealedKind, SpaceId, R,
    SUITE_ID,
};
use crate::net::tracing_like;

impl Engine {
    /// The epoch a queued event was sealed at, without reading its sealed bytes.
    pub(super) fn sealed_epoch(&self, e: &Event) -> Option<u64> {
        self.sealed_epoch_of(&e.client_id)
    }

    pub(super) fn sealed_epoch_of(&self, client_id: &str) -> Option<u64> {
        self.store
            .conn()
            .query_row(
                "SELECT substr(value, 1, instr(value, ':') - 1) FROM meta WHERE key = ?1",
                [sealed_meta(client_id)],
                |r| r.get::<_, String>(0),
            )
            .ok()?
            .parse()
            .ok()
    }

    /// The historical sealed copy, without filtering by the group's current epoch.
    pub(crate) fn sealed_copy(&self, e: &Event) -> Option<(u64, Envelope)> {
        let v = self.store.meta(&sealed_meta(&e.client_id)).ok().flatten()?;
        let mut parts = v.splitn(3, ':');
        let epoch = parts.next()?.parse().ok()?;
        let content = hex::decode(parts.next()?).ok()?;
        let env = Envelope::new(content, parts.next()?.to_string(), e.cert.clone(), None)?;
        Some((epoch, env))
    }

    pub(crate) fn unreadable_current_sealed_copy(&self, e: &Event, epoch: u64) -> bool {
        self.sealed_epoch(e) == Some(epoch) && self.sealed_copy(e).is_none()
    }

    /// Seals every queued message whose group is ready and that has no copy for the
    /// group's current epoch. `true` when something new can go out.
    pub fn mls_seal_outbox(&mut self) -> R<bool> {
        self.mls_seal_outbox_changes().map(|(sealed, _)| sealed)
    }

    /// Failed consent uses the normal outbox receipt and change notification.
    pub(crate) fn mls_seal_outbox_changes(&mut self) -> R<(bool, HashSet<SpaceId>)> {
        let mut refused = HashSet::new();
        if self.net.pending.is_empty() {
            return Ok((false, refused));
        }
        // Validate retained originals before any cryptographic effect, including
        // successors of a damaged head that already has a cached ciphertext.
        let heads = self.store.outbox_heads()?;
        let mut ready: HashMap<SpaceId, Option<u64>> = HashMap::new();
        let mut sealed = false;
        let mut blocked = HashSet::new();
        for h in heads {
            if h.failed || self.net.failed.contains_key(&h.client_id) {
                continue;
            }
            let e = match self.read_outbox_head(&h) {
                Ok(event) => event,
                Err(failure) => {
                    failure.trace();
                    if let Some(space) = self.unreadable_outbox_space(&h) {
                        blocked.insert(space.to_string());
                        continue;
                    }
                    // No reliable lane: stop rather than encrypt successors
                    // under an association reconstructed from corrupt metadata.
                    return Err(storage("outbox head has no reliable Space association"));
                }
            };
            // SQL Space metadata is not a selection authority. Resolve the
            // signed original before deciding the lane, readiness or cache hit.
            if blocked.contains(&e.space) {
                continue;
            }
            if !self.app_share_consent_current(&e) {
                self.refuse_stale_app_share(&e)?;
                refused.insert(e.space);
                continue;
            }
            if !self.is_e2e(&e.space) {
                continue;
            }
            let epoch = *ready
                .entry(e.space.clone())
                .or_insert_with(|| self.ready_epoch(&e.space));
            let Some(epoch) = epoch else { continue };
            if !Self::app_share_epoch_matches(&e, epoch) {
                self.refuse_stale_app_share(&e)?;
                refused.insert(e.space);
                continue;
            }
            if self.sealed_epoch_of(&h.client_id) == Some(epoch) {
                if self.unreadable_current_sealed_copy(&e, epoch) {
                    blocked.insert(e.space.clone());
                }
                continue;
            }
            if !must_seal(&e.body) {
                continue;
            }
            self.seal_outgoing(&e, epoch)?;
            sealed = true;
        }
        Ok((sealed, refused))
    }

    pub(super) fn seal_outgoing(&mut self, e: &Event, epoch: u64) -> R<()> {
        // Ratchet and exact ciphertext remain one durable operation.
        let tx = self.store.conn().unchecked_transaction().map_err(storage)?;
        self.seal_outgoing_in_transaction(e, epoch)?;
        tx.commit().map_err(storage)?;
        Ok(())
    }

    /// The caller's transaction also retains its producer-specific association.
    /// This does not open, commit, or recover a partial transaction.
    pub(crate) fn seal_outgoing_in_transaction(&self, e: &Event, epoch: u64) -> R<()> {
        if self.store.conn().is_autocommit() {
            return Err(CoreError::Invalid {
                reason: "Sealing requires a retained transaction.".into(),
            });
        }
        if !self.app_share_consent_current(e) || !Self::app_share_epoch_matches(e, epoch) {
            return Err(crate::app_current_share::refused());
        }
        let author = self
            .net
            .author
            .clone()
            .ok_or_else(|| CoreError::Forbidden {
                reason: t("Entre na sua conta primeiro.", "Sign in first."),
            })?;
        let inner = InnerEvent {
            content: e.content.clone(),
            sig: e.sig.clone(),
        };
        let device = self.device_on(self.store.conn())?;
        if matches!(e.body, EventBody::AppCurrentShared { .. })
            && device.epoch(&e.space).map_err(mls_err)? != epoch
        {
            return Err(crate::app_current_share::refused());
        }
        let data = device.seal(&e.space, &inner.encode()).map_err(mls_err)?;
        let outer = Envelope::sealed(
            &author,
            &e.space,
            &e.client_id,
            e.at_ms,
            e.seen.as_ref(),
            Sealed::new(SealedKind::Application, SUITE_ID, data),
        )?;
        self.store.set_meta(
            &sealed_meta(&e.client_id),
            &format!("{epoch}:{}:{}", hex::encode(outer.content()), outer.sig),
        )?;
        Ok(())
    }

    /// What goes out now for a queued event: its sealed copy for the current epoch, the
    /// handshake it is, or itself. `None` while it waits for its group.
    /// Each Space's ready epoch is looked up once per batch (`ready`).
    pub(crate) fn outgoing_envelope_with(
        &self,
        e: &Event,
        ready: &mut HashMap<SpaceId, Option<u64>>,
    ) -> Option<Envelope> {
        // Also fence retained cached ciphertext when no sealing pass has run.
        if !self.app_share_consent_current(e) {
            return None;
        }
        if matches!(e.body, EventBody::Sealed { .. }) {
            return Envelope::new(e.content.clone(), e.sig.clone(), e.cert.clone(), None);
        }
        if !(self.is_e2e(&e.space) && must_seal(&e.body)) {
            return Envelope::plain(e)
                .inspect_err(|_| tracing_like("publication waiting malformed_signed_content"))
                .ok();
        }
        let epoch = *ready.entry(e.space.clone()).or_insert_with(|| {
            match self.ready_epoch_checked(&e.space, &self.state) {
                Ok(epoch) => Some(epoch),
                Err(reason) => {
                    // Same gate and per-Space memoization, with a closed diagnostic.
                    // Never infer readiness from profiles or discard an original intent.
                    tracing_like(&format!("publication waiting ready_epoch={reason:?}"));
                    None
                }
            }
        });
        // Cheap check first: only decode the sealed copy when it is for this epoch.
        if epoch.is_none()
            || epoch.is_some_and(|epoch| !Self::app_share_epoch_matches(e, epoch))
            || self.sealed_epoch(e) != epoch
        {
            return None;
        }
        let (at, env) = self.sealed_copy(e)?;
        (Some(at) == epoch).then_some(env)
    }

    /// Signs MLS bytes into the Space's outbox, in order.
    pub(crate) fn handshake_event(&self, space: &str, sealed: Sealed) -> R<Event> {
        let author = self
            .net
            .author
            .clone()
            .ok_or_else(|| CoreError::Forbidden {
                reason: t("Entre na sua conta primeiro.", "Sign in first."),
            })?;
        let at_ms = crate::engine::now_ms();
        let client_id = new_ulid(at_ms)?;
        let seen = self.logs.get(space).and_then(event_log::SpaceLog::head);
        let (content, sig) = author.sign_sealed(space, &client_id, at_ms, seen.as_ref(), sealed);
        let e = event_from_content(
            content,
            sig,
            author.cert.clone(),
            0,
            String::new(),
            String::new(),
        )
        .map_err(storage)?;
        Ok(e)
    }

    pub(crate) fn queued_recovery_commit(&mut self, events: Vec<Event>) {
        for e in events {
            self.net.pending.insert(e.client_id, e.space.clone());
            self.net.mls.mark_group_dirty(e.space);
        }
        self.net.wake();
    }

    /// A stale publication blocks sealing only while its sent epoch is still current.
    /// A commit may already have landed and maintenance may have resealed this outbox
    /// entry before the old rejection arrives. Preserve that newer copy and epoch.
    pub(crate) fn mls_sealed_stale(&mut self, client_id: &str, sent_epoch: u64) {
        let Some(space) = self.net.pending.get(client_id).cloned() else {
            return;
        };
        if self.device().ok().and_then(|d| d.epoch(&space).ok()) == Some(sent_epoch) {
            self.net.mls.behind.insert(space, sent_epoch);
        }
    }
}

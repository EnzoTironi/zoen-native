//! Retained outgoing ciphertext, handshakes and stale-epoch fences.
use super::*;

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

    /// Seals every queued message whose group is ready and that has no copy for the
    /// group's current epoch. `true` when something new can go out.
    pub fn mls_seal_outbox(&mut self) -> R<bool> {
        if self.net.pending.is_empty() {
            return Ok(false);
        }
        // Runs after every write: look at the queue without decoding it, and read only the
        // events that still need sealing for their group's current epoch.
        let heads = self.store.outbox_heads()?;
        let mut ready: HashMap<SpaceId, Option<u64>> = HashMap::new();
        let mut sealed = false;
        for h in heads {
            if h.failed || !self.is_e2e(&h.space) {
                continue;
            }
            let epoch = *ready
                .entry(h.space.clone())
                .or_insert_with(|| self.ready_epoch(&h.space));
            let Some(epoch) = epoch else { continue };
            if self.sealed_epoch_of(&h.client_id) == Some(epoch) {
                continue;
            }
            let Some(e) = self.store.outbox_get(&h.client_id)? else {
                continue;
            };
            if !must_seal(&e.body) {
                continue;
            }
            self.seal_outgoing(&e, epoch)?;
            sealed = true;
        }
        Ok(sealed)
    }

    pub(super) fn seal_outgoing(&mut self, e: &Event, epoch: u64) -> R<()> {
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
        // Advancing the sender ratchet and retaining its exact ciphertext are
        // one durable operation. A failed cache write must not consume a generation.
        let tx = self.store.conn().unchecked_transaction().map_err(storage)?;
        let data = self
            .device_on(&tx)?
            .seal(&e.space, &inner.encode())
            .map_err(mls_err)?;
        let outer = Envelope::sealed(
            &author,
            &e.space,
            &e.client_id,
            e.at_ms,
            e.seen.as_ref(),
            Sealed::new(SealedKind::Application, SUITE_ID, data),
        );
        self.store.set_meta(
            &sealed_meta(&e.client_id),
            &format!("{epoch}:{}:{}", hex::encode(outer.content()), outer.sig),
        )?;
        tx.commit().map_err(storage)?;
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
        if matches!(e.body, EventBody::Sealed { .. }) {
            return Envelope::new(e.content.clone(), e.sig.clone(), e.cert.clone(), None);
        }
        if !(self.is_e2e(&e.space) && must_seal(&e.body)) {
            return Some(Envelope::plain(e));
        }
        let epoch = *ready
            .entry(e.space.clone())
            .or_insert_with(|| self.ready_epoch(&e.space));
        // Cheap check first: only decode the sealed copy when it is for this epoch.
        if epoch.is_none() || self.sealed_epoch(e) != epoch {
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
        let client_id = new_ulid(at_ms);
        let seen = self.logs.get(space).and_then(|l| l.head());
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

//! Publication refusals and exact dependent media intents.

use super::{
    same_media_group, ConfirmedMediaDependencies, CoreError, Engine, Event, EventBody, HashSet,
    ItemContent, ItemKind, RejectionOutcome, R,
};

impl Engine {
    /// The relay refused one of ours. `sent_epoch` describes the application envelope
    /// actually sent on this connection, not a copy resealed since then.
    pub(crate) fn reject(
        &mut self,
        client_id: &str,
        reason: &str,
        permanent: bool,
        sent_epoch: Option<u64>,
    ) -> RejectionOutcome {
        if permanent {
            match self.mls_handshake_refused(client_id, reason) {
                Ok(true) => return RejectionOutcome::HandshakeReconciled,
                Ok(false) => {}
                Err(error) => return RejectionOutcome::StorageFailure(error.to_string()),
            }
        }
        let rejected = match self.store.outbox_get(client_id) {
            Ok(None) => return RejectionOutcome::Ignored,
            Err(error) => return RejectionOutcome::StorageFailure(error.to_string()),
            Ok(Some(event)) => event,
        };
        // Written in the clear just before the Space went end-to-end: it waits for the
        // group and goes out sealed, it doesn't fail.
        // Sealed before this device caught up with a commit: it seals again once it has.
        if let (protocol::STALE_SEAL, Some(epoch)) = (reason, sent_epoch) {
            self.mls_sealed_stale(client_id, epoch);
        }
        let permanent =
            permanent && reason != protocol::SEAL_REQUIRED && reason != protocol::STALE_SEAL;
        let rejected_creation = if permanent {
            self.store.outbox_get(client_id).ok().flatten().filter(|e| {
                matches!(e.body, EventBody::SpaceCreated { .. }) && self.next_seq(&e.space) == 0
            })
        } else {
            None
        };
        let noted = if let Some(creation) = rejected_creation {
            self.store
                .outbox_refuse_space(&creation.space, reason)
                .map_err(CoreError::from)
        } else if permanent {
            self.refuse_outbox_media_dependents(&rejected, reason)
        } else {
            self.store
                .outbox_note(client_id, reason, permanent)
                .map_err(CoreError::from)
        };
        if let Err(error) = noted {
            return RejectionOutcome::StorageFailure(error.to_string());
        }
        if !permanent {
            return RejectionOutcome::Retry;
        }
        let _ = self.store.meta_delete(&format!("invite:{client_id}"));
        // Re-project without the refused event (messages stay visible as failed).
        let _ = self.reload();
        RejectionOutcome::UserFailed
    }

    /// Preserve terminal signed intents as readable failures, atomically. A
    /// refused File fails its exact referencing media messages; a refused group
    /// member also fails its matching group, so newer Space heads can progress.
    pub(super) fn refuse_outbox_media_dependents(&self, rejected: &Event, reason: &str) -> R<()> {
        let candidate = matches!(
            &rejected.body,
            EventBody::MessagePosted { media: Some(_), .. }
        ) || matches!(&rejected.body, EventBody::ItemCreated { kind: ItemKind::File, content: ItemContent::File(file), .. } if !file.is_voice());
        if !candidate || event_log::verify_author(rejected).is_err() {
            return self
                .store
                .outbox_note(&rejected.client_id, reason, true)
                .map_err(Into::into);
        }
        let rows = self.store.outbox()?;
        let mut seeds = Vec::new();
        if matches!(&rejected.body, EventBody::MessagePosted { media: Some(media), .. } if media.is_valid())
        {
            seeds.push(rejected);
        }
        if let EventBody::ItemCreated {
            item,
            kind: ItemKind::File,
            content: ItemContent::File(file),
            ..
        } = &rejected.body
        {
            if !file.is_voice() {
                // A refused duplicate creation must not invalidate references to
                // an already authoritative Item. Never consult optimistic state.
                let confirmed = self
                    .logs
                    .get(&rejected.space)
                    .map(|log| ConfirmedMediaDependencies::from_log(self, log))
                    .transpose()?;
                if confirmed
                    .as_ref()
                    .is_none_or(|confirmed| !confirmed.state.items.contains_key(item))
                {
                    seeds.extend(
                        rows.iter()
                            .filter(|row| !row.failed)
                            .map(|row| &row.event)
                            .filter(|event| {
                                event.space == rejected.space
                                    && event.author == rejected.author
                                    && event.device == rejected.device
                                    && event_log::verify_author(event).is_ok()
                                    && matches!(&event.body, EventBody::MessagePosted {
                                attaches: Some(attaches), media, ..
                            } if attaches == item && media.as_ref().is_none_or(types::MessageMedia::is_valid))
                            }),
                    );
                }
            }
        }
        if seeds.is_empty() {
            return self
                .store
                .outbox_note(&rejected.client_id, reason, true)
                .map_err(Into::into);
        }
        let is_member = |event: &Event| {
            event.space == rejected.space
                && event.author == rejected.author
                && event.device == rejected.device
                && event_log::verify_author(event).is_ok()
                && seeds
                    .iter()
                    .any(|seed| seed.client_id == event.client_id || same_media_group(seed, event))
        };
        // An aborted album must not leave a fresh, otherwise unused File head
        // blocking later ordinary messages while that orphan's upload retries.
        // Shared references and already canonical Items retain their own fate.
        let orphan_files: HashSet<_> = rows.iter().filter(|row| is_member(&row.event))
            .filter_map(|row| match &row.event.body {
                EventBody::MessagePosted { attaches: Some(item), .. } => Some(item),
                _ => None,
            }).filter(|item| !rows.iter().any(|row| !row.failed && !is_member(&row.event)
                && matches!(&row.event.body, EventBody::MessagePosted { attaches: Some(other), .. } if other == *item)))
            .collect();
        let confirmed = self
            .logs
            .get(&rejected.space)
            .map(|log| ConfirmedMediaDependencies::from_log(self, log))
            .transpose()?;
        let tx = self
            .store
            .conn()
            .is_autocommit()
            .then(|| self.store.conn().unchecked_transaction())
            .transpose()
            .map_err(store::StoreError::from)?;
        for row in &rows {
            let same_owner = row.event.space == rejected.space
                && row.event.author == rejected.author
                && row.event.device == rejected.device
                && event_log::verify_author(&row.event).is_ok();
            let orphan = same_owner
                && matches!(&row.event.body,
                EventBody::ItemCreated { item, kind: ItemKind::File, content: ItemContent::File(file), .. }
                    if orphan_files.contains(item) && confirmed.as_ref().is_none_or(|state| !state.state.items.contains_key(item))
                    && file.chunks.iter().chain(file.thumb.iter()).any(|chunk| self.store.upload_pending(&chunk.blob).unwrap_or(true)));
            let dependent = is_member(&row.event) || orphan;
            if row.event.client_id == rejected.client_id || (!row.failed && dependent) {
                self.store.outbox_note(&row.event.client_id, reason, true)?;
            }
        }
        if let Some(tx) = tx {
            tx.commit().map_err(store::StoreError::from)?;
        }
        Ok(())
    }

    /// A terminal HTTP refusal and the corresponding failed signed intents are
    /// one durable transaction. No relay confirmation is inferred from local bytes.
    pub fn upload_refused(&mut self, blob: &str) -> R<Vec<String>> {
        let tx = self
            .store
            .conn()
            .unchecked_transaction()
            .map_err(store::StoreError::from)?;
        if !self.store.upload_refused(blob)? {
            return Ok(Vec::new());
        }
        let mut changed = HashSet::new();
        for row in self.store.outbox()? {
            let file = self.outbox_attachment(&row.event);
            if !row.failed
                && event_log::verify_author(&row.event).is_ok()
                && file.is_some_and(|file| {
                    file.chunks
                        .iter()
                        .chain(file.thumb.iter())
                        .any(|chunk| chunk.blob == blob)
                })
            {
                self.refuse_outbox_media_dependents(&row.event, "attachment upload refused")?;
                changed.insert(row.event.space.clone());
            }
        }
        tx.commit().map_err(store::StoreError::from)?;
        self.reload()?;
        self.net.wake();
        Ok(changed.into_iter().collect())
    }
}

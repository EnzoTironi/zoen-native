//! Explicit recovery copies refused local bytes into the existing private journal.
//! It never changes the original event, receipt, Failed state, or transport queue.
use super::*;
use crate::engine::{Entry, EntryBody};
use types::{
    AttachmentGrouping, AttachmentSendOptions, Event, EventBody, FileDoc, ItemContent, ItemKind,
    MessageMedia, ReplyRef,
};

impl Engine {
    /// This closure only receives an immutable Engine: no canonical mutation or
    /// projection reload belongs to a failed journal-only action. Generic action
    /// rollback reloads may prune already-confirmed outbox rows as a side effect.
    fn atomic_media_recovery<T>(&self, work: impl FnOnce(&Self) -> R<T>) -> R<T> {
        self.store
            .conn()
            .execute_batch("SAVEPOINT media_recovery")
            .map_err(|_| storage_error())?;
        let result = work(self).and_then(|value| {
            self.store
                .conn()
                .execute_batch("RELEASE media_recovery")
                .map_err(|_| storage_error())?;
            Ok(value)
        });
        if result.is_err() {
            self.store
                .conn()
                .execute_batch("ROLLBACK TO media_recovery; RELEASE media_recovery")
                .map_err(|_| storage_error())?;
        }
        result
    }
    /// The caller holds the canonical engine lock. The transaction also includes
    /// both draft CAS reads, authenticated chunk reads, and every encrypted copy.
    pub(crate) fn recover_failed_media_draft(
        &self,
        scope: &MediaDraftScope,
        message_id: &str,
        expected: &str,
        expected_message: &str,
    ) -> R<MediaDraftDto> {
        self.atomic_media_recovery(|engine| {
            let storage = engine.media_draft_scope(scope)?;
            let previous = engine.load_media_draft(&storage)?;
            let normal = engine.message_draft(&scope.normal())?;
            if !intent_id(expected)
                || previous.revision != expected
                || previous.content.is_some()
                || !previous.assets.is_empty()
                || previous.attempted
                || normal.revision != expected_message
                || !normal.text.is_empty()
                || normal.reply_to.is_some()
            {
                return Err(stale());
            }
            if !engine
                .state
                .writer(&scope.space_id, &scope.expected_identity_id)
            {
                return Err(CoreError::Forbidden {
                    reason: t(
                        "só autores recuperam mídia para enviar",
                        "only writers can prepare media for sending",
                    ),
                });
            }
            let first = engine.failed_media_event(scope, message_id)?;
            let (file, caption, reply, media) = engine.failed_media_source(scope, &first)?;
            let reply_to = match &reply {
                Some(reply) if reply.thread => {
                    if scope.thread_root_id.as_deref() != Some(reply.to.as_str()) {
                        return Err(invalid());
                    }
                    None
                }
                value if scope.thread_root_id.is_none() => {
                    value.as_ref().map(|reply| reply.to.clone())
                }
                _ => return Err(invalid()),
            };
            engine.draft_target(&scope.normal(), reply_to.as_deref())?;
            let mut sources = vec![(first, file, caption, reply.clone(), media.clone())];
            if let Some(group) = media.as_ref().and_then(|media| media.group.as_ref()) {
                let mut candidates = engine
                    .space_state(&scope.space_id)?
                    .entries
                    .iter()
                    .filter_map(|entry| {
                        let EntryBody::Message { marks, .. } = &entry.body else {
                            return None;
                        };
                        let member = marks.media.as_ref()?.group.as_ref()?;
                        (entry.author == scope.expected_identity_id && member.id == group.id)
                            .then(|| (member.index, entry.id().to_string()))
                    })
                    .collect::<Vec<_>>();
                candidates.sort_by_key(|(index, _)| *index);
                if candidates.len() != group.count as usize {
                    return Err(invalid());
                }
                sources.clear();
                for (expected_index, (index, id)) in candidates.into_iter().enumerate() {
                    if index as usize != expected_index {
                        return Err(invalid());
                    }
                    let event = engine.failed_media_event(scope, &id)?;
                    let (file, caption, member_reply, member_media) =
                        engine.failed_media_source(scope, &event)?;
                    let member = member_media
                        .as_ref()
                        .and_then(|media| media.group.as_ref())
                        .ok_or_else(invalid)?;
                    if member.count != group.count
                        || member.id != group.id
                        || member_reply != reply
                        || member_media.as_ref().map(|value| value.presentation)
                            != media.as_ref().map(|value| value.presentation)
                    {
                        return Err(invalid());
                    }
                    sources.push((event, file, caption, member_reply, member_media));
                }
            }
            let voice = sources[0].1.is_voice();
            if voice && sources.len() != 1 {
                return Err(invalid());
            }
            // Bound the WHOLE copy before allocating any authenticated payload.
            if sources
                .iter()
                .try_fold(0u64, |total, (_, file, _, _, _)| {
                    total.checked_add(file.bytes)
                })
                .is_none_or(|total| total > MAX_ASSET_BYTES as u64)
            {
                return Err(invalid());
            }
            let mut assets = Vec::with_capacity(sources.len());
            let mut fresh = Vec::with_capacity(sources.len());
            let mut attachments = Vec::with_capacity(sources.len());
            for (_, file, caption, _, _) in &sources {
                let bytes = engine
                    .owned_file_bytes(file, MAX_ASSET_BYTES as u64)?
                    .ok_or_else(storage_error)?;
                let asset_id = types::new_id("recovered_asset");
                let asset = MediaDraftAsset {
                    id: asset_id.clone(),
                    name: file.name.clone(),
                    mime: file.mime.clone(),
                    size: file.bytes,
                    sha256: file.sha256.clone(),
                };
                fresh.push((asset.clone(), Zeroizing::new(bytes)));
                assets.push(asset);
                attachments.push(Attachment {
                    asset_id,
                    caption: caption.clone(),
                    mutation_id: types::new_id("send"),
                });
            }
            let content = if voice {
                // The old editor recipe is not recoverable from the sent render.
                // Import these final bytes as a NEW source; native decoding will
                // create/authenticate a fresh recipe before editing or sending.
                Content::Voice {
                    reply_to,
                    source_asset_id: assets[0].id.clone(),
                    editor: serde_json::Value::Null,
                    render: None,
                }
            } else {
                Content::Attachments {
                    reply_to,
                    items: attachments,
                    gallery: None,
                    options: AttachmentSendOptions {
                        presentation: media
                            .as_ref()
                            .map(|media| media.presentation)
                            .unwrap_or_default(),
                        grouping: if sources.len() > 1 {
                            AttachmentGrouping::Grouped
                        } else {
                            AttachmentGrouping::Separate
                        },
                    },
                }
            };
            let record = Record {
                revision: types::new_id("media_draft"),
                content: Some(content),
                assets,
                attempted: false,
                last_accepted: previous.last_accepted.clone(),
                save_receipts: previous.save_receipts.clone(),
            };
            validate(&record)?;
            engine.persist_media_replacement(&storage, &previous, &record, &fresh)?;
            record.dto()
        })
    }

    fn failed_media_event(&self, scope: &MediaDraftScope, id: &str) -> R<Event> {
        if !target_id(id) {
            return Err(invalid());
        }
        let failed: Option<bool> = self
            .store
            .conn()
            .query_row(
                "SELECT failed FROM outbox WHERE client_id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| storage_error())?;
        let event = self.store.outbox_get(id)?.ok_or_else(invalid)?;
        if failed != Some(true)
            || !self.net.failed.contains_key(id)
            || event.client_id != id
            || event.seq != 0
            || event.space != scope.space_id
            || event.author != scope.expected_identity_id
            || event.device.as_deref() != Some(scope.expected_device_id.as_str())
            || event_log::verify_author(&event).is_err()
            || self.logs.get(&scope.space_id).is_some_and(|log| {
                log.events()
                    .iter()
                    .any(|confirmed| confirmed.client_id == id)
            })
        {
            return Err(invalid());
        }
        let entry = self
            .space_state(&scope.space_id)?
            .entries
            .iter()
            .find(|entry| entry.id() == id)
            .ok_or_else(invalid)?;
        if entry.seq != 0
            || entry.author != event.author
            || self.delivery(entry) != crate::Delivery::Failed
        {
            return Err(invalid());
        }
        Ok(event)
    }

    fn failed_media_source(
        &self,
        scope: &MediaDraftScope,
        event: &Event,
    ) -> R<(FileDoc, String, Option<ReplyRef>, Option<MessageMedia>)> {
        let entry: &Entry = self
            .space_state(&scope.space_id)?
            .entries
            .iter()
            .find(|entry| entry.id() == event.client_id)
            .ok_or_else(invalid)?;
        let EntryBody::Message {
            text,
            attaches: Some(item),
            reply,
            marks,
        } = &entry.body
        else {
            return Err(invalid());
        };
        if marks.deleted || marks.edited {
            return Err(invalid());
        }
        let file = self.state.items.get(item).ok_or_else(invalid)?;
        if file.space != scope.space_id
            || file.kind != ItemKind::File
            || file.versions.len() != 1
            || file.versions[0].author != scope.expected_identity_id
        {
            return Err(invalid());
        }
        let ItemContent::File(doc) = &file.versions[0].content else {
            return Err(invalid());
        };
        crate::files::validate_file(doc, MAX_ASSET_BYTES as u64)?;
        match &event.body {
            EventBody::MessagePosted {
                text: original_text,
                attaches: Some(original_item),
                reply: original_reply,
                media,
                forwarded,
                ..
            } if !doc.is_voice()
                && item == original_item
                && text == original_text
                && reply == original_reply
                && marks.media == *media
                && marks.forwarded == *forwarded
                && media.as_ref().is_none_or(|value| {
                    value.is_valid() && !(*forwarded && value.group.is_some())
                }) =>
            {
                Ok((doc.clone(), text.clone(), reply.clone(), media.clone()))
            }
            EventBody::ItemCreated {
                item: original_item,
                kind: ItemKind::File,
                content: ItemContent::File(original),
                ..
            } if doc.is_voice()
                && item == original_item
                && doc == original
                && doc.voice.as_ref().is_some_and(|voice| {
                    voice.reply == *reply && voice.forwarded == marks.forwarded
                }) =>
            {
                Ok((doc.clone(), String::new(), reply.clone(), None))
            }
            _ => Err(invalid()),
        }
    }
}

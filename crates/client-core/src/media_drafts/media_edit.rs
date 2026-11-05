//! The existing encrypted journal, isolated by target; one atomic file/edit/receipt commit.
use super::{
    invalid, stale, Content, Engine, MediaDraftScope, MediaDraftSendResult, Receipt, Record, R,
};
use protocol::message_media_edit::MessageMediaEdit;
use sha2::{Digest, Sha256};

pub(super) fn validate_scope_content(scope: &MediaDraftScope, content: &Option<Content>) -> R<()> {
    let Some(content) = content else {
        return Ok(());
    };
    match (scope.edit_message_id.as_ref(), content) {
        (
            Some(_),
            Content::Attachments {
                items,
                edit_revision: Some(revision),
                normal_revision,
                options,
                ..
            },
        ) => {
            let reply = content.reply();
            if normal_revision
                .as_deref()
                .is_some_and(|revision| !super::target_id(revision))
                || items.len() != 1
                || !super::target_id(revision)
                || reply.to.is_some()
                || reply.selection.is_some()
                || reply.shared.is_some()
                || reply.intent.is_some()
                || !options.is_default()
            {
                return Err(invalid());
            }
        }
        (Some(_), _)
        | (
            None,
            Content::Attachments {
                edit_revision: Some(_),
                ..
            },
        ) => return Err(invalid()),
        (
            None,
            Content::Attachments {
                normal_revision: Some(_),
                ..
            },
        ) => return Err(invalid()),
        _ => {}
    }
    Ok(())
}
impl Engine {
    pub(super) fn send_media_edit_draft(
        &mut self,
        scope: &MediaDraftScope,
        expected: &str,
    ) -> R<MediaDraftSendResult> {
        let storage = self.media_draft_scope(scope)?;
        let mut record = self.load_media_draft(&storage)?;
        let target = scope.edit_message_id.as_ref().ok_or_else(invalid)?;
        if !super::intent_id(expected) {
            return Err(stale());
        }
        if let Some(receipt) = record
            .last_accepted
            .as_ref()
            .filter(|receipt| receipt.input_revision == expected)
        {
            let input = receipt.media_edit.as_ref().ok_or_else(invalid)?;
            if input.message_id != *target
                || input.space_id != scope.space_id
                || input.expected_identity_id != scope.expected_identity_id
                || input.expected_device_id != scope.expected_device_id
            {
                return Err(invalid());
            }
            return Ok(MediaDraftSendResult {
                entries: vec![self.replace_message_media(input)?.entry],
                draft: record.dto()?,
            });
        }
        if record.revision != expected || expected == super::MEDIA_DRAFT_REVISION_ZERO {
            return Err(stale());
        }
        validate_scope_content(scope, &record.content)?;
        let Some(Content::Attachments {
            items,
            edit_revision: Some(revision),
            normal_revision,
            ..
        }) = &record.content
        else {
            return Err(invalid());
        };
        let item = items.first().ok_or_else(invalid)?;
        let asset = record
            .assets
            .first()
            .filter(|asset| asset.id == item.asset_id)
            .ok_or_else(invalid)?;
        let identity = serde_json::to_vec(&("zoen.media.edit.file.v1", scope, &item.mutation_id))
            .map_err(|_| invalid())?;
        let item_id = format!("it_{}", hex::encode(Sha256::digest(identity)));
        let input = MessageMediaEdit {
            expected_identity_id: scope.expected_identity_id.clone(),
            expected_device_id: scope.expected_device_id.clone(),
            space_id: scope.space_id.clone(),
            message_id: target.clone(),
            edit: types::MediaChange {
                mutation_id: item.mutation_id.clone(),
                expected_revision: revision.clone(),
                text: item.caption.clone(),
                replacement: types::MediaReplacement {
                    item_id: item_id.clone(),
                    version: 1,
                    sha256: asset.sha256.clone(),
                },
            },
        };
        self.validate_media_edit_target(&input).map_err(|error| {
            if record.attempted {
                error
            } else {
                crate::CoreError::DraftRejected {
                    reason: error.to_string(),
                }
            }
        })?;
        let bytes = self.load_media_asset(&storage, asset)?;
        let normal_revision = normal_revision.clone();
        let name = asset.name.clone();
        let mime = asset.mime.clone();
        if !record.attempted {
            record.attempted = true;
            self.atomic_agent_action(|engine| engine.persist_media_record(&storage, &record))?;
        }
        self.atomic_agent_action(|engine| {
            engine.file_add_at(
                crate::files::FileAddInput {
                    space: &scope.space_id,
                    path: &name,
                    name: &name,
                    mime: &mime,
                    bytes: &bytes,
                    thumbnail: None,
                },
                &item_id,
            )?;
            let result = engine.replace_message_media(&input)?;
            if let Some(revision) = &normal_revision {
                engine.complete_message_media_edit(&scope.normal(), revision, &input)?;
            }
            let cleared = Record {
                revision: types::new_id("media_draft")?,
                content: None,
                assets: Vec::new(),
                attempted: false,
                last_accepted: Some(Receipt {
                    input_revision: expected.into(),
                    messages: vec![target.clone()],
                    reply: None,
                    media_edit: Some(input.clone()),
                }),
                save_receipts: record.save_receipts.clone(),
            };
            for asset in &record.assets {
                engine.store.meta_delete(&storage.asset_key(&asset.id))?;
            }
            engine.persist_media_record(&storage, &cleared)?;
            Ok(MediaDraftSendResult {
                entries: vec![result.entry],
                draft: cleared.dto()?,
            })
        })
    }
}

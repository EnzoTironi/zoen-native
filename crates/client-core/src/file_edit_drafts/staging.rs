//! Staging on the original Engine and canonical document state.

use super::{
    hash, intent_id, invalid, stale, storage_error, t, valid_pdf, validate, Binding, CoreError,
    Digest, Engine, FileEditDraftCapture, FileEditDraftCommitResult, FileEditDraftDto,
    FileEditDraftSaveResult, FileEditDraftScope, Sha256, StageReceipt, Zeroizing, MAX_RECEIPTS, R,
};

impl Engine {
    pub(super) fn file_edit_draft(&self, scope: &FileEditDraftScope) -> R<FileEditDraftDto> {
        let storage = self.file_edit_draft_scope(scope)?;
        Ok(self.load_file_edit_draft(&storage)?.dto())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn stage_file_edit_draft(
        &mut self,
        scope: &FileEditDraftScope,
        expected: &str,
        editor: &str,
        capture_id: &str,
        origin_version: u32,
        origin_sha256: &str,
        bytes: Vec<u8>,
    ) -> R<FileEditDraftSaveResult> {
        let bytes = Zeroizing::new(bytes);
        let storage = self.file_edit_draft_scope(scope)?;
        let mut record = self.load_file_edit_draft(&storage)?;
        if !intent_id(expected)
            || !intent_id(editor)
            || !intent_id(capture_id)
            || origin_version == 0
            || !hash(origin_sha256)
            || !valid_pdf(&bytes)
        {
            return Err(invalid());
        }
        let origin = FileEditDraftCommitResult {
            version: origin_version,
            sha256: origin_sha256.into(),
        };
        let capture = FileEditDraftCapture {
            capture_id: capture_id.into(),
            size: bytes.len() as u64,
            sha256: hex::encode(Sha256::digest(&*bytes)),
        };
        let fingerprint = hex::encode(Sha256::digest(
            serde_json::to_vec(&(
                "zoen.file.edit.draft.stage.v1",
                expected,
                editor,
                &origin,
                &capture,
            ))
            .map_err(|_| invalid())?,
        ));
        if let Some(receipt) = record.stages.iter().find(|r| r.input_revision == expected) {
            return if receipt.fingerprint == fingerprint {
                Ok(FileEditDraftSaveResult {
                    accepted_revision: receipt.accepted_revision.clone(),
                    draft: record.dto(),
                })
            } else {
                Err(stale())
            };
        }
        if record.revision != expected
            || record.pending.iter().any(|c| c.capture_id == capture_id)
            || record.stages.iter().any(|r| r.capture_id == capture_id)
            || record.commits.iter().any(|r| r.capture_id == capture_id)
        {
            return Err(stale());
        }
        if !self
            .state
            .writer(&scope.space_id, &scope.expected_identity_id)
        {
            return Err(CoreError::Forbidden {
                reason: t("edição de arquivo não autorizada", "unauthorized file edit"),
            });
        }
        self.file_edit_pdf_base(scope, None)?;
        if self.file_edit_pdf_base(scope, Some(origin_version))? != origin {
            return Err(stale());
        }
        match &record.binding {
            Some(binding) if binding.editor_id == editor => {
                if binding.origin != origin {
                    return Err(stale());
                }
            }
            _ => {
                if !record.pending.is_empty() {
                    return Err(stale());
                }
                // Capturing an editor's full copy remains private even when a
                // remote version arrived after it opened. Only commit may publish.
                record.binding = Some(Binding {
                    editor_id: editor.into(),
                    base: origin.clone(),
                    origin,
                });
            }
        }
        record.pending.push(capture.clone());
        record.revision = types::new_id("file_edit_draft")?;
        record.stages.push(StageReceipt {
            input_revision: expected.into(),
            fingerprint,
            accepted_revision: record.revision.clone(),
            editor_id: editor.into(),
            capture_id: capture_id.into(),
        });
        if record.stages.len() > MAX_RECEIPTS {
            record.stages.remove(0);
        }
        validate(&record)?;
        self.atomic_agent_action(|engine| {
            let sealed =
                crate::local_sealed::seal(&storage.key, &storage.asset_aad(&capture)?, &bytes)
                    .map_err(|()| storage_error())?;
            engine
                .store
                .set_meta(&storage.asset_key(capture_id), &sealed)?;
            engine.persist_file_edit_draft(&storage, &record)?;
            Ok(FileEditDraftSaveResult {
                accepted_revision: record.revision.clone(),
                draft: record.dto(),
            })
        })
    }

    pub(super) fn file_edit_draft_bytes(
        &self,
        scope: &FileEditDraftScope,
        expected: &str,
        capture_id: &str,
    ) -> R<Vec<u8>> {
        let storage = self.file_edit_draft_scope(scope)?;
        let record = self.load_file_edit_draft(&storage)?;
        if !intent_id(expected) || record.revision != expected {
            return Err(stale());
        }
        let capture = record
            .pending
            .iter()
            .find(|c| c.capture_id == capture_id)
            .ok_or_else(stale)?;
        Ok(self.load_file_edit_capture(&storage, capture)?.to_vec())
    }
}

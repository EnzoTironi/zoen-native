//! Publication on the original Engine and canonical document state.

use super::*;

impl Engine {
    pub(super) fn commit_file_edit_draft(
        &mut self,
        scope: &FileEditDraftScope,
        editor: &str,
        capture_id: &str,
        note: &str,
    ) -> R<FileEditDraftCommitResult> {
        let storage = self.file_edit_draft_scope(scope)?;
        let mut record = self.load_file_edit_draft(&storage)?;
        if !intent_id(editor) || !intent_id(capture_id) {
            return Err(invalid());
        }
        // Current membership remains mandatory, but an accepted retry must not
        // re-run writer/base preflight after a later version or access-role change.
        if let Some(receipt) = record
            .commits
            .iter()
            .find(|r| r.editor_id == editor && r.capture_id == capture_id)
        {
            return Ok(receipt.result.clone());
        }
        let binding = record
            .binding
            .as_ref()
            .filter(|b| b.editor_id == editor)
            .ok_or_else(stale)?;
        let capture = record
            .pending
            .first()
            .filter(|c| c.capture_id == capture_id)
            .ok_or_else(stale)?
            .clone();
        if note.len() > 4_096 || note.contains('\0') {
            return Err(invalid());
        }
        let current = self.file_edit_pdf_base(scope, None)?;
        if current != binding.base {
            return Err(stale());
        }
        let bytes = self.load_file_edit_capture(&storage, &capture)?;
        self.atomic_agent_action(|engine| {
            engine.file_new_version_checked(
                &scope.item_id,
                current.version,
                &current.sha256,
                &bytes,
                None,
                note,
            )?;
            let result = engine.file_edit_pdf_base(scope, None)?;
            record.pending.remove(0);
            record.binding.as_mut().ok_or_else(storage_error)?.base = result.clone();
            record.revision = types::new_id("file_edit_draft");
            record.commits.push(CommitReceipt {
                editor_id: editor.into(),
                capture_id: capture_id.into(),
                result: result.clone(),
            });
            if record.commits.len() > MAX_RECEIPTS {
                record.commits.remove(0);
            }
            engine.store.meta_delete(&storage.asset_key(capture_id))?;
            engine.persist_file_edit_draft(&storage, &record)?;
            Ok(result)
        })
    }

    pub(super) fn discard_file_edit_draft(
        &mut self,
        scope: &FileEditDraftScope,
        expected: &str,
    ) -> R<FileEditDraftDto> {
        let storage = self.file_edit_draft_scope(scope)?;
        let mut record = self.load_file_edit_draft(&storage)?;
        if !intent_id(expected) || record.revision != expected {
            return Err(stale());
        }
        self.atomic_agent_action(|engine| {
            for capture in &record.pending {
                engine
                    .store
                    .meta_delete(&storage.asset_key(&capture.capture_id))?;
            }
            record.pending.clear();
            record.revision = types::new_id("file_edit_draft");
            engine.persist_file_edit_draft(&storage, &record)?;
            Ok(record.dto())
        })
    }
}

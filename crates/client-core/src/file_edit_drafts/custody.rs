//! Custody on the original Engine and canonical document state.

use super::{
    invalid, stale, storage_error, valid_pdf, validate, Digest, DraftScope, Engine,
    FileEditDraftCapture, FileEditDraftCommitResult, FileEditDraftScope, Hkdf, ItemContent,
    OptionalExtension, Record, Sha256, Storage, Zeroizing, DOMAIN, FILE_EDIT_DRAFT_REVISION_ZERO,
    MAX_ASSET_BYTES, MAX_RECORD_BYTES, MAX_RETAINED_BYTES, MAX_SCOPES, PREFIX, R,
};

impl Engine {
    pub(super) fn file_edit_draft_scope(&self, scope: &FileEditDraftScope) -> R<Storage> {
        self.validate_private_draft_scope(&DraftScope {
            expected_identity_id: scope.expected_identity_id.clone(),
            expected_device_id: scope.expected_device_id.clone(),
            space_id: scope.space_id.clone(),
            thread_root_id: None,
        })?;
        if scope.item_id.is_empty()
            || scope.item_id.len() > 256
            || scope.item_id.chars().any(char::is_control)
            || self.item_state(&scope.item_id)?.space != scope.space_id
        {
            return Err(invalid());
        }
        let author = self.author_for(&scope.expected_identity_id)?;
        let aad = serde_json::to_vec(&(
            "zoen.file.edit.draft.record.v1",
            &scope.expected_identity_id,
            &scope.expected_device_id,
            &scope.space_id,
            &scope.item_id,
        ))
        .map_err(|_| storage_error())?;
        let mut key = Zeroizing::new([0u8; 32]);
        Hkdf::<Sha256>::new(Some(DOMAIN), &author.key.secret())
            .expand(b"local PDF edit encryption", &mut *key)
            .map_err(|_| storage_error())?;
        Ok(Storage {
            id: format!("{PREFIX}record.{}", hex::encode(Sha256::digest(&aad))),
            aad,
            key,
        })
    }

    pub(super) fn file_edit_pdf_base(
        &self,
        scope: &FileEditDraftScope,
        version: Option<u32>,
    ) -> R<FileEditDraftCommitResult> {
        let item = self.item_state(&scope.item_id)?;
        let version = match version {
            Some(number) => item
                .versions
                .iter()
                .find(|v| v.number == number)
                .ok_or_else(stale)?,
            None => item.current(),
        };
        let ItemContent::File(file) = &version.content else {
            return Err(invalid());
        };
        if file.mime != "application/pdf"
            || file.is_voice()
            || item
                .versions
                .iter()
                .any(|v| matches!(&v.content, ItemContent::File(f) if f.is_voice()))
        {
            return Err(invalid());
        }
        Ok(FileEditDraftCommitResult {
            version: version.number,
            sha256: file.sha256.clone(),
        })
    }

    pub(super) fn file_edit_draft_row(&self, id: &str, max_plain: usize) -> R<Option<String>> {
        let maximum = (max_plain + 40).div_ceil(3) * 4 + 3;
        let row: Option<Option<String>> = self.store.conn().query_row(
            "SELECT CASE WHEN length(CAST(value AS BLOB)) <= ?2 THEN value ELSE NULL END FROM meta WHERE key=?1",
            rusqlite::params![id, maximum], |r| r.get(0)).optional().map_err(|_| storage_error())?;
        row.map(|v| v.ok_or_else(storage_error)).transpose()
    }

    pub(super) fn load_file_edit_draft(&self, storage: &Storage) -> R<Record> {
        let Some(encoded) = self.file_edit_draft_row(&storage.id, MAX_RECORD_BYTES)? else {
            return Ok(Record::default());
        };
        let bytes = Zeroizing::new(
            crate::local_sealed::open(&storage.key, &storage.aad, &encoded, MAX_RECORD_BYTES)
                .map_err(|()| storage_error())?,
        );
        let record: Record = serde_json::from_slice(&bytes).map_err(|_| storage_error())?;
        if record.revision == FILE_EDIT_DRAFT_REVISION_ZERO {
            return Err(storage_error());
        }
        validate(&record).map_err(|_| storage_error())?;
        Ok(record)
    }

    pub(super) fn persist_file_edit_draft(&self, storage: &Storage, record: &Record) -> R<()> {
        validate(record)?;
        let bytes = Zeroizing::new(serde_json::to_vec(record).map_err(|_| storage_error())?);
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(invalid());
        }
        let encoded = crate::local_sealed::seal(&storage.key, &storage.aad, &bytes)
            .map_err(|()| storage_error())?;
        self.store.set_meta(&storage.id, &encoded)?;
        let (scopes, size): (usize, usize) = self.store.conn().query_row(
            "SELECT COUNT(CASE WHEN key LIKE 'file.edit.draft.record.%' THEN 1 END), COALESCE(SUM(length(CAST(value AS BLOB))),0) FROM meta WHERE key LIKE 'file.edit.draft.%'",
            [], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|_| storage_error())?;
        if scopes > MAX_SCOPES || size > MAX_RETAINED_BYTES {
            return Err(invalid());
        }
        Ok(())
    }

    pub(super) fn load_file_edit_capture(
        &self,
        storage: &Storage,
        capture: &FileEditDraftCapture,
    ) -> R<Zeroizing<Vec<u8>>> {
        let encoded = self
            .file_edit_draft_row(&storage.asset_key(&capture.capture_id), MAX_ASSET_BYTES)?
            .ok_or_else(storage_error)?;
        let bytes = Zeroizing::new(
            crate::local_sealed::open(
                &storage.key,
                &storage.asset_aad(capture)?,
                &encoded,
                MAX_ASSET_BYTES,
            )
            .map_err(|()| storage_error())?,
        );
        if !valid_pdf(&bytes)
            || bytes.len() as u64 != capture.size
            || hex::encode(Sha256::digest(&*bytes)) != capture.sha256
        {
            return Err(storage_error());
        }
        Ok(bytes)
    }
}

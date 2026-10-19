//! Private, device-local complete PDF edits. Only an explicit FIFO commit versions a file.
use crate::{
    engine::{Engine, R},
    i18n::t,
    message_drafts::DraftScope,
    Client, CoreError,
};
use hkdf::Hkdf;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use types::ItemContent;
use zeroize::Zeroizing;

pub const FILE_EDIT_DRAFT_REVISION_ZERO: &str = "0";
const PREFIX: &str = "file.edit.draft.";
const DOMAIN: &[u8] = b"zoen.file.edit.draft.v1";
const MAX_ASSET_BYTES: usize = 64 << 20;
const MAX_PENDING_BYTES: u64 = 128 << 20;
const MAX_RETAINED_BYTES: usize = 256 << 20;
const MAX_RECORD_BYTES: usize = 64 << 10;
const MAX_SCOPES: usize = 256;
const MAX_PENDING: usize = 16;
const MAX_RECEIPTS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FileEditDraftScope {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub item_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
#[serde(deny_unknown_fields)]
pub struct FileEditDraftCapture {
    pub capture_id: String,
    pub sha256: String,
    pub size: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, uniffi::Record)]
pub struct FileEditDraftDto {
    pub revision: String,
    pub editor_id: Option<String>,
    pub origin_version: Option<u32>,
    pub origin_sha256: Option<String>,
    pub base_version: Option<u32>,
    pub base_sha256: Option<String>,
    pub pending: Vec<FileEditDraftCapture>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, uniffi::Record)]
pub struct FileEditDraftSaveResult {
    pub accepted_revision: String,
    pub draft: FileEditDraftDto,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
#[serde(deny_unknown_fields)]
pub struct FileEditDraftCommitResult {
    pub version: u32,
    pub sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    editor_id: String,
    origin: FileEditDraftCommitResult,
    base: FileEditDraftCommitResult,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StageReceipt {
    input_revision: String,
    fingerprint: String,
    accepted_revision: String,
    editor_id: String,
    capture_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitReceipt {
    editor_id: String,
    capture_id: String,
    result: FileEditDraftCommitResult,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    revision: String,
    binding: Option<Binding>,
    pending: Vec<FileEditDraftCapture>,
    stages: Vec<StageReceipt>,
    commits: Vec<CommitReceipt>,
}
impl Default for Record {
    fn default() -> Self {
        Self {
            revision: FILE_EDIT_DRAFT_REVISION_ZERO.into(),
            binding: None,
            pending: Vec::new(),
            stages: Vec::new(),
            commits: Vec::new(),
        }
    }
}
impl Record {
    fn dto(&self) -> FileEditDraftDto {
        FileEditDraftDto {
            revision: self.revision.clone(),
            editor_id: self.binding.as_ref().map(|b| b.editor_id.clone()),
            origin_version: self.binding.as_ref().map(|b| b.origin.version),
            origin_sha256: self.binding.as_ref().map(|b| b.origin.sha256.clone()),
            base_version: self.binding.as_ref().map(|b| b.base.version),
            base_sha256: self.binding.as_ref().map(|b| b.base.sha256.clone()),
            pending: self.pending.clone(),
        }
    }
}
struct Storage {
    id: String,
    aad: Vec<u8>,
    key: Zeroizing<[u8; 32]>,
}
impl Storage {
    fn asset_key(&self, id: &str) -> String {
        format!(
            "{PREFIX}asset.{}.{}",
            hex::encode(Sha256::digest(&self.aad)),
            hex::encode(Sha256::digest(id.as_bytes()))
        )
    }
    fn asset_aad(&self, capture: &FileEditDraftCapture) -> R<Vec<u8>> {
        serde_json::to_vec(&("zoen.file.edit.draft.asset.v1", &self.aad, capture))
            .map_err(|_| storage_error())
    }
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: t(
            "edição de PDF inválida ou grande demais",
            "invalid or oversized PDF edit",
        ),
    }
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: t(
            "o PDF ou suas edições mudaram; reabra a versão atual",
            "the PDF or its edits changed; reopen the current version",
        ),
    }
}
fn storage_error() -> CoreError {
    CoreError::Storage {
        message: t(
            "não foi possível recuperar as edições do PDF",
            "could not recover the PDF edits",
        ),
    }
}
fn intent_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_base(value: &FileEditDraftCommitResult) -> bool {
    value.version > 0 && hash(&value.sha256)
}
fn valid_pdf(bytes: &[u8]) -> bool {
    bytes.len() <= MAX_ASSET_BYTES && bytes.starts_with(b"%PDF-")
}
fn validate(record: &Record) -> R<()> {
    if !intent_id(&record.revision)
        || record.pending.len() > MAX_PENDING
        || record.stages.len() > MAX_RECEIPTS
        || record.commits.len() > MAX_RECEIPTS
        || (record.binding.is_none() && !record.pending.is_empty())
    {
        return Err(invalid());
    }
    if let Some(binding) = &record.binding {
        if !intent_id(&binding.editor_id)
            || !valid_base(&binding.origin)
            || !valid_base(&binding.base)
            || binding.base.version < binding.origin.version
        {
            return Err(invalid());
        }
    }
    let mut captures = HashSet::new();
    let mut total = 0u64;
    for capture in &record.pending {
        if !intent_id(&capture.capture_id)
            || !captures.insert(&capture.capture_id)
            || !hash(&capture.sha256)
            || capture.size < 5
            || capture.size > MAX_ASSET_BYTES as u64
        {
            return Err(invalid());
        }
        total = total.checked_add(capture.size).ok_or_else(invalid)?;
    }
    if total > MAX_PENDING_BYTES {
        return Err(invalid());
    }
    let mut revisions = HashSet::new();
    for receipt in &record.stages {
        if !intent_id(&receipt.input_revision)
            || !intent_id(&receipt.accepted_revision)
            || receipt.accepted_revision == FILE_EDIT_DRAFT_REVISION_ZERO
            || !intent_id(&receipt.editor_id)
            || !intent_id(&receipt.capture_id)
            || !hash(&receipt.fingerprint)
            || !revisions.insert(&receipt.input_revision)
        {
            return Err(invalid());
        }
    }
    let mut accepted = HashSet::new();
    for receipt in &record.commits {
        if !intent_id(&receipt.editor_id)
            || !intent_id(&receipt.capture_id)
            || !valid_base(&receipt.result)
            || !accepted.insert(&receipt.capture_id)
            || record
                .pending
                .iter()
                .any(|capture| capture.capture_id == receipt.capture_id)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

impl Engine {
    fn file_edit_draft_scope(&self, scope: &FileEditDraftScope) -> R<Storage> {
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
    fn file_edit_pdf_base(
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
    fn file_edit_draft_row(&self, id: &str, max_plain: usize) -> R<Option<String>> {
        let maximum = (max_plain + 40).div_ceil(3) * 4 + 3;
        let row: Option<Option<String>> = self.store.conn().query_row(
            "SELECT CASE WHEN length(CAST(value AS BLOB)) <= ?2 THEN value ELSE NULL END FROM meta WHERE key=?1",
            rusqlite::params![id, maximum], |r| r.get(0)).optional().map_err(|_| storage_error())?;
        row.map(|v| v.ok_or_else(storage_error)).transpose()
    }
    fn load_file_edit_draft(&self, storage: &Storage) -> R<Record> {
        let Some(encoded) = self.file_edit_draft_row(&storage.id, MAX_RECORD_BYTES)? else {
            return Ok(Record::default());
        };
        let bytes = Zeroizing::new(
            crate::local_sealed::open(&storage.key, &storage.aad, &encoded, MAX_RECORD_BYTES)
                .map_err(|_| storage_error())?,
        );
        let record: Record = serde_json::from_slice(&bytes).map_err(|_| storage_error())?;
        if record.revision == FILE_EDIT_DRAFT_REVISION_ZERO {
            return Err(storage_error());
        }
        validate(&record).map_err(|_| storage_error())?;
        Ok(record)
    }
    fn persist_file_edit_draft(&self, storage: &Storage, record: &Record) -> R<()> {
        validate(record)?;
        let bytes = Zeroizing::new(serde_json::to_vec(record).map_err(|_| storage_error())?);
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(invalid());
        }
        let encoded = crate::local_sealed::seal(&storage.key, &storage.aad, &bytes)
            .map_err(|_| storage_error())?;
        self.store.set_meta(&storage.id, &encoded)?;
        let (scopes, size): (usize, usize) = self.store.conn().query_row(
            "SELECT COUNT(CASE WHEN key LIKE 'file.edit.draft.record.%' THEN 1 END), COALESCE(SUM(length(CAST(value AS BLOB))),0) FROM meta WHERE key LIKE 'file.edit.draft.%'",
            [], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|_| storage_error())?;
        if scopes > MAX_SCOPES || size > MAX_RETAINED_BYTES {
            return Err(invalid());
        }
        Ok(())
    }
    fn load_file_edit_capture(
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
            .map_err(|_| storage_error())?,
        );
        if !valid_pdf(&bytes)
            || bytes.len() as u64 != capture.size
            || hex::encode(Sha256::digest(&*bytes)) != capture.sha256
        {
            return Err(storage_error());
        }
        Ok(bytes)
    }
    fn file_edit_draft(&self, scope: &FileEditDraftScope) -> R<FileEditDraftDto> {
        let storage = self.file_edit_draft_scope(scope)?;
        Ok(self.load_file_edit_draft(&storage)?.dto())
    }
    #[allow(clippy::too_many_arguments)]
    fn stage_file_edit_draft(
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
        record.revision = types::new_id("file_edit_draft");
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
                    .map_err(|_| storage_error())?;
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
    fn file_edit_draft_bytes(
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
    fn commit_file_edit_draft(
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
    fn discard_file_edit_draft(
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

#[uniffi::export]
impl Client {
    pub fn file_edit_draft(
        &self,
        scope: FileEditDraftScope,
    ) -> Result<FileEditDraftDto, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock().file_edit_draft(&scope)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn stage_file_edit_draft(
        &self,
        scope: FileEditDraftScope,
        expected_revision: String,
        editor_id: String,
        capture_id: String,
        origin_version: u32,
        origin_sha256: String,
        bytes: Vec<u8>,
    ) -> Result<FileEditDraftSaveResult, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock().stage_file_edit_draft(
            &scope,
            &expected_revision,
            &editor_id,
            &capture_id,
            origin_version,
            &origin_sha256,
            bytes,
        )
    }
    pub fn file_edit_draft_bytes(
        &self,
        scope: FileEditDraftScope,
        expected_revision: String,
        capture_id: String,
    ) -> Result<Vec<u8>, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock()
            .file_edit_draft_bytes(&scope, &expected_revision, &capture_id)
    }
    pub fn commit_file_edit_draft(
        &self,
        scope: FileEditDraftScope,
        editor_id: String,
        capture_id: String,
        note: String,
    ) -> Result<FileEditDraftCommitResult, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock()
            .commit_file_edit_draft(&scope, &editor_id, &capture_id, &note)
    }
    pub fn discard_file_edit_draft(
        &self,
        scope: FileEditDraftScope,
        expected_revision: String,
    ) -> Result<FileEditDraftDto, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock()
            .discard_file_edit_draft(&scope, &expected_revision)
    }
}

#[cfg(test)]
mod tests;

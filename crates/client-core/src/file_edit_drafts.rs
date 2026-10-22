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

mod custody;
mod publication;
mod staging;

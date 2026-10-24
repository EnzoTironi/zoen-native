//! Private, device-key-encrypted media waiting for an explicit Send. No replicated state.
use crate::{
    engine::{Engine, R},
    i18n::t,
    message_drafts::DraftScope,
    Client, CoreError, TimelineEntry,
};
use hkdf::Hkdf;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use zeroize::Zeroizing;

mod gallery;
mod recovery;

pub const MEDIA_DRAFT_REVISION_ZERO: &str = "0";
const PREFIX: &str = "media.draft.";
const DOMAIN: &[u8] = b"zoen.media.draft.v1";
const MAX_ASSET_BYTES: usize = 64 << 20;
const MAX_CONTENT_BYTES: usize = 512 << 10;
const MAX_RECORD_BYTES: usize = MAX_CONTENT_BYTES + (32 << 10);
const MAX_RETAINED_BYTES: usize = 512 << 20;
const MAX_SCOPES: usize = 4096;
const MAX_SAVE_RECEIPTS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct MediaDraftScope {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub thread_root_id: Option<String>,
}
impl MediaDraftScope {
    pub(crate) fn normal(&self) -> DraftScope {
        DraftScope {
            expected_identity_id: self.expected_identity_id.clone(),
            expected_device_id: self.expected_device_id.clone(),
            space_id: self.space_id.clone(),
            thread_root_id: self.thread_root_id.clone(),
        }
    }
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct MediaDraftAssetInput {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
#[serde(deny_unknown_fields)]
pub struct MediaDraftAsset {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, uniffi::Record)]
pub struct MediaDraftDto {
    pub revision: String,
    pub content_json: String,
    pub assets: Vec<MediaDraftAsset>,
    pub attempted: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, uniffi::Record)]
pub struct MediaDraftSaveResult {
    pub accepted_revision: String,
    pub draft: MediaDraftDto,
}
#[derive(Debug, Clone, Serialize, uniffi::Record)]
pub struct MediaDraftSendResult {
    pub entries: Vec<TimelineEntry>,
    pub draft: MediaDraftDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Content {
    Attachments {
        reply_to: Option<String>,
        items: Vec<Attachment>,
        #[serde(
            default,
            skip_serializing_if = "types::AttachmentSendOptions::is_default"
        )]
        options: types::AttachmentSendOptions,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gallery: Option<gallery::Gallery>,
    },
    Voice {
        reply_to: Option<String>,
        source_asset_id: String,
        editor: serde_json::Value,
        render: Option<VoiceRender>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attachment {
    asset_id: String,
    caption: String,
    mutation_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VoiceRender {
    asset_id: String,
    mime: String,
    duration_ms: f64,
    levels: Vec<f32>,
    transcript: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    input_revision: String,
    messages: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveReceipt {
    input_revision: String,
    fingerprint: String,
    // Pre-ACK records remain readable; an unknown output can never prove a replay ACK.
    #[serde(default)]
    accepted_revision: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    revision: String,
    content: Option<Content>,
    assets: Vec<MediaDraftAsset>,
    attempted: bool,
    last_accepted: Option<Receipt>,
    #[serde(default)]
    save_receipts: Vec<SaveReceipt>,
}
impl Default for Record {
    fn default() -> Self {
        Self {
            revision: MEDIA_DRAFT_REVISION_ZERO.into(),
            content: None,
            assets: Vec::new(),
            attempted: false,
            last_accepted: None,
            save_receipts: Vec::new(),
        }
    }
}
struct Storage {
    id: String,
    aad: Vec<u8>,
    key: Zeroizing<[u8; 32]>,
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: t(
            "rascunho de mídia inválido ou grande demais",
            "invalid or oversized media draft",
        ),
    }
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: t(
            "o rascunho de mídia mudou; reabra a versão atual",
            "the media draft changed; reopen the current version",
        ),
    }
}
fn storage_error() -> CoreError {
    CoreError::Storage {
        message: t(
            "não foi possível recuperar o rascunho de mídia",
            "could not recover the media draft",
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
fn target_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn valid_mime(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value.contains('/')
        && !value.chars().any(char::is_control)
}
fn valid_asset(asset: &MediaDraftAsset) -> bool {
    intent_id(&asset.id)
        && !asset.name.trim().is_empty()
        && asset.name.len() <= 1024
        && !asset.name.contains(['/', '\\', '\0'])
        && valid_mime(&asset.mime)
        && asset.size > 0
        && asset.size <= MAX_ASSET_BYTES as u64
        && asset.sha256.len() == 64
        && asset.sha256.bytes().all(|b| b.is_ascii_hexdigit())
}
fn content_json(content: &Option<Content>) -> R<String> {
    let json = serde_json::to_string(content).map_err(|_| invalid())?;
    if json.len() > MAX_CONTENT_BYTES {
        return Err(invalid());
    }
    Ok(json)
}
fn references(content: &Option<Content>) -> R<Vec<&str>> {
    let Some(content) = content else {
        return Ok(Vec::new());
    };
    let reply = match content {
        Content::Attachments { reply_to, .. } | Content::Voice { reply_to, .. } => reply_to,
    };
    if reply.as_deref().is_some_and(|id| !target_id(id)) {
        return Err(invalid());
    }
    match content {
        Content::Attachments {
            items,
            options,
            gallery,
            ..
        } => {
            if !options.validate_count(items.len()) {
                return Err(invalid());
            }
            if let Some(gallery) = gallery {
                gallery.validate(items)?;
            }
            let mut mutations = HashSet::new();
            for item in items {
                if !intent_id(&item.asset_id)
                    || !intent_id(&item.mutation_id)
                    || !mutations.insert(&item.mutation_id)
                    || item.caption.len() > crate::files::MAX_FILE_CAPTION_BYTES
                    || item.caption.contains('\0')
                {
                    return Err(invalid());
                }
            }
            Ok(items.iter().map(|item| item.asset_id.as_str()).collect())
        }
        Content::Voice {
            source_asset_id,
            render,
            ..
        } => {
            if !intent_id(source_asset_id) {
                return Err(invalid());
            }
            let mut ids = vec![source_asset_id.as_str()];
            if let Some(render) = render {
                if !intent_id(&render.asset_id)
                    || !valid_mime(&render.mime)
                    || !render.mime.starts_with("audio/")
                    || !render.duration_ms.is_finite()
                    || !(250.0..=3_600_000.0).contains(&render.duration_ms)
                    || render.levels.len() > 160
                    || render
                        .levels
                        .iter()
                        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                    || render.transcript.len() > 65_536
                    || render.transcript.contains('\0')
                {
                    return Err(invalid());
                }
                ids.push(&render.asset_id);
            }
            Ok(ids)
        }
    }
}
fn validate(record: &Record) -> R<()> {
    let mut save_revisions = HashSet::new();
    if !intent_id(&record.revision)
        || (record.attempted && record.content.is_none())
        || record.save_receipts.len() > MAX_SAVE_RECEIPTS
        || record.save_receipts.iter().any(|receipt| {
            !intent_id(&receipt.input_revision)
                || !save_revisions.insert(&receipt.input_revision)
                || receipt.accepted_revision.as_ref().is_some_and(|revision| {
                    !intent_id(revision)
                        || revision == MEDIA_DRAFT_REVISION_ZERO
                        || revision == &receipt.input_revision
                })
                || receipt.fingerprint.len() != 64
                || !receipt
                    .fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
        })
        || record.last_accepted.as_ref().is_some_and(|r| {
            !intent_id(&r.input_revision)
                || r.input_revision == MEDIA_DRAFT_REVISION_ZERO
                || r.messages.is_empty()
                || r.messages.len() > 10
                || r.messages.iter().any(|id| !target_id(id))
        })
    {
        return Err(invalid());
    }
    content_json(&record.content)?;
    let refs = references(&record.content)?;
    if refs.len() != record.assets.len() {
        return Err(invalid());
    }
    let mut seen = HashSet::new();
    let mut total = 0u64;
    for (id, asset) in refs.iter().zip(&record.assets) {
        if *id != asset.id || !seen.insert(*id) || !valid_asset(asset) {
            return Err(invalid());
        }
        total = total.checked_add(asset.size).ok_or_else(invalid)?;
    }
    if let Some(Content::Attachments {
        gallery: Some(gallery),
        ..
    }) = &record.content
    {
        for entry in &gallery.entries {
            if !record.assets.iter().any(|asset| {
                asset.id == entry.asset_id
                    && (asset.mime.starts_with("image/") || asset.mime.starts_with("video/"))
            }) {
                return Err(invalid());
            }
        }
    }
    match &record.content {
        Some(Content::Attachments { .. }) if total > MAX_ASSET_BYTES as u64 => {
            return Err(invalid())
        }
        Some(Content::Voice { render, .. }) => {
            if record
                .assets
                .iter()
                .any(|asset| !asset.mime.starts_with("audio/"))
            {
                return Err(invalid());
            }
            if let Some(render) = render {
                if record.assets[1].mime != render.mime {
                    return Err(invalid());
                }
            }
        }
        _ => {}
    }
    Ok(())
}
impl Record {
    fn dto(&self) -> R<MediaDraftDto> {
        Ok(MediaDraftDto {
            revision: self.revision.clone(),
            content_json: content_json(&self.content)?,
            assets: self.assets.clone(),
            attempted: self.attempted,
        })
    }
}
impl Storage {
    fn asset_key(&self, id: &str) -> String {
        format!(
            "{PREFIX}asset.{}.{}",
            hex::encode(Sha256::digest(&self.aad)),
            hex::encode(Sha256::digest(id.as_bytes()))
        )
    }
    fn asset_aad(&self, asset: &MediaDraftAsset) -> R<Vec<u8>> {
        serde_json::to_vec(&("zoen.media.draft.asset.v1", &self.aad, asset))
            .map_err(|_| storage_error())
    }
}

#[uniffi::export]
impl Client {
    pub fn media_draft(&self, scope: MediaDraftScope) -> Result<MediaDraftDto, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock().media_draft(&scope)
    }
    pub fn save_media_draft(
        &self,
        scope: MediaDraftScope,
        expected_revision: String,
        content_json: String,
        assets: Vec<MediaDraftAssetInput>,
    ) -> Result<MediaDraftSaveResult, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock()
            .save_media_draft(&scope, &expected_revision, &content_json, assets)
    }
    pub fn save_media_draft_with_recent_sources(
        &self,
        scope: MediaDraftScope,
        expected_revision: String,
        content_json: String,
        assets: Vec<MediaDraftAssetInput>,
        source_bindings: Vec<crate::RecentAttachmentSourceBinding>,
    ) -> Result<MediaDraftSaveResult, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock().save_media_draft_with_recent_sources(
            &scope,
            &expected_revision,
            &content_json,
            assets,
            source_bindings,
        )
    }
    pub fn media_draft_asset(
        &self,
        scope: MediaDraftScope,
        expected_revision: String,
        asset_id: String,
    ) -> Result<Vec<u8>, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock()
            .media_draft_asset(&scope, &expected_revision, &asset_id)
    }
    pub fn send_media_draft(
        &self,
        scope: MediaDraftScope,
        expected_revision: String,
    ) -> Result<MediaDraftSendResult, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock().send_media_draft(&scope, &expected_revision)
    }
    /// The canonical DTO serializer for native bridges whose generated records are not Codable.
    pub fn send_media_draft_json(
        &self,
        scope: MediaDraftScope,
        expected_revision: String,
    ) -> Result<String, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        let result = self.lock().send_media_draft(&scope, &expected_revision)?;
        serde_json::to_string(&result).map_err(|_| storage_error())
    }
}

#[cfg(test)]
mod tests;

mod capture;
mod custody;
mod publication;

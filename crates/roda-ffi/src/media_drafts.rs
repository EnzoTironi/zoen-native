//! Private, device-key-encrypted media waiting for an explicit Send. No replicated state.
use crate::{
    engine::{Engine, R},
    i18n::t,
    message_drafts::DraftScope,
    CoreError, RodaEngine, TimelineEntry,
};
use hkdf::Hkdf;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use zeroize::Zeroizing;

pub const MEDIA_DRAFT_REVISION_ZERO: &str = "0";
const PREFIX: &str = "media.draft.";
const DOMAIN: &[u8] = b"zoen.media.draft.v1";
const MAX_ASSET_BYTES: usize = 64 << 20;
const MAX_CONTENT_BYTES: usize = 512 << 10;
const MAX_RECORD_BYTES: usize = MAX_CONTENT_BYTES + (32 << 10);
const MAX_RETAINED_BYTES: usize = 512 << 20;
const MAX_SCOPES: usize = 4096;
const MAX_SAVE_RECEIPTS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MediaDraftScope {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub thread_root_id: Option<String>,
}
impl MediaDraftScope {
    fn normal(&self) -> DraftScope {
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
        Content::Attachments { items, .. } => {
            if items.is_empty() || items.len() > 10 {
                return Err(invalid());
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

impl Engine {
    fn media_draft_scope(&self, scope: &MediaDraftScope) -> R<Storage> {
        self.validate_private_draft_scope(&scope.normal())?;
        let author = self.author_for(&scope.expected_identity_id)?;
        let aad = serde_json::to_vec(&(
            "zoen.media.draft.record.v1",
            &scope.expected_identity_id,
            &scope.expected_device_id,
            &scope.space_id,
            &scope.thread_root_id,
        ))
        .map_err(|_| storage_error())?;
        let mut key = Zeroizing::new([0u8; 32]);
        Hkdf::<Sha256>::new(Some(DOMAIN), &author.key.secret())
            .expand(b"local media draft encryption", &mut *key)
            .map_err(|_| storage_error())?;
        Ok(Storage {
            id: format!("{PREFIX}record.{}", hex::encode(Sha256::digest(&aad))),
            aad,
            key,
        })
    }
    fn media_draft_row(&self, key: &str, max_plain: usize) -> R<Option<String>> {
        let maximum = (max_plain + 40).div_ceil(3) * 4 + 3;
        let row: Option<Option<String>> = self.store.conn().query_row(
            "SELECT CASE WHEN length(CAST(value AS BLOB)) <= ?2 THEN value ELSE NULL END FROM meta WHERE key=?1",
            rusqlite::params![key, maximum], |r| r.get(0)).optional().map_err(|_| storage_error())?;
        row.map(|value| value.ok_or_else(storage_error)).transpose()
    }
    fn load_media_draft(&self, storage: &Storage) -> R<Record> {
        let Some(encoded) = self.media_draft_row(&storage.id, MAX_RECORD_BYTES)? else {
            return Ok(Record::default());
        };
        let plain = Zeroizing::new(
            crate::local_sealed::open(&storage.key, &storage.aad, &encoded, MAX_RECORD_BYTES)
                .map_err(|_| storage_error())?,
        );
        let record: Record = serde_json::from_slice(&plain).map_err(|_| storage_error())?;
        if record.revision == MEDIA_DRAFT_REVISION_ZERO {
            return Err(storage_error());
        }
        validate(&record).map_err(|_| storage_error())?;
        Ok(record)
    }
    fn persist_media_record(&self, storage: &Storage, record: &Record) -> R<()> {
        validate(record)?;
        let plain = Zeroizing::new(serde_json::to_vec(record).map_err(|_| storage_error())?);
        if plain.len() > MAX_RECORD_BYTES {
            return Err(invalid());
        }
        let encrypted = crate::local_sealed::seal(&storage.key, &storage.aad, &plain)
            .map_err(|_| storage_error())?;
        self.store.set_meta(&storage.id, &encrypted)?;
        let (scopes, bytes): (usize, usize) = self.store.conn().query_row(
            "SELECT COUNT(CASE WHEN key LIKE 'media.draft.record.%' THEN 1 END), COALESCE(SUM(length(CAST(value AS BLOB))),0) FROM meta WHERE key LIKE 'media.draft.%'",
            [], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|_| storage_error())?;
        if scopes > MAX_SCOPES || bytes > MAX_RETAINED_BYTES {
            return Err(invalid());
        }
        Ok(())
    }
    fn load_media_asset(
        &self,
        storage: &Storage,
        asset: &MediaDraftAsset,
    ) -> R<Zeroizing<Vec<u8>>> {
        let encoded = self
            .media_draft_row(&storage.asset_key(&asset.id), MAX_ASSET_BYTES)?
            .ok_or_else(storage_error)?;
        let bytes = Zeroizing::new(
            crate::local_sealed::open(
                &storage.key,
                &storage.asset_aad(asset)?,
                &encoded,
                MAX_ASSET_BYTES,
            )
            .map_err(|_| storage_error())?,
        );
        if bytes.len() as u64 != asset.size || hex::encode(Sha256::digest(&*bytes)) != asset.sha256
        {
            return Err(storage_error());
        }
        Ok(bytes)
    }
    fn media_draft(&self, scope: &MediaDraftScope) -> R<MediaDraftDto> {
        let storage = self.media_draft_scope(scope)?;
        self.load_media_draft(&storage)?.dto()
    }
    fn save_media_draft(
        &mut self,
        scope: &MediaDraftScope,
        expected: &str,
        json: &str,
        inputs: Vec<MediaDraftAssetInput>,
    ) -> R<MediaDraftSaveResult> {
        let storage = self.media_draft_scope(scope)?;
        let previous = self.load_media_draft(&storage)?;
        if !intent_id(expected) {
            return Err(stale());
        }
        if json.len() > MAX_CONTENT_BYTES || inputs.len() > 10 {
            return Err(invalid());
        }
        let content: Option<Content> = serde_json::from_str(json).map_err(|_| invalid())?;
        let refs = references(&content)?;
        let input_limit = if matches!(&content, Some(Content::Voice { .. })) {
            2 * MAX_ASSET_BYTES
        } else {
            MAX_ASSET_BYTES
        };
        let input_bytes = inputs
            .iter()
            .try_fold(0usize, |bytes, input| bytes.checked_add(input.bytes.len()))
            .ok_or_else(invalid)?;
        if input_bytes > input_limit {
            return Err(invalid());
        }
        let mut incoming = HashMap::new();
        let mut supplied = Vec::with_capacity(inputs.len());
        for input in inputs {
            if !intent_id(&input.id)
                || input.bytes.is_empty()
                || input.bytes.len() > MAX_ASSET_BYTES
                || !refs.contains(&input.id.as_str())
                || incoming.contains_key(&input.id)
            {
                return Err(invalid());
            }
            let bytes = Zeroizing::new(input.bytes);
            let asset = MediaDraftAsset {
                id: input.id,
                name: input.name,
                mime: input.mime,
                size: bytes.len() as u64,
                sha256: hex::encode(Sha256::digest(&*bytes)),
            };
            if !valid_asset(&asset) {
                return Err(invalid());
            }
            supplied.push(asset.clone());
            incoming.insert(asset.id.clone(), (asset, bytes));
        }
        // The receipt is authoritative for lost native acknowledgements, including
        // cleared/pruned assets. Never infer acceptance from JSON equality alone.
        let fingerprint = hex::encode(Sha256::digest(
            serde_json::to_vec(&(
                "zoen.media.draft.save.v1",
                expected,
                content_json(&content)?,
                supplied,
            ))
            .map_err(|_| invalid())?,
        ));
        if let Some(receipt) = previous
            .save_receipts
            .iter()
            .find(|receipt| receipt.input_revision == expected)
        {
            return if receipt.fingerprint == fingerprint {
                Ok(MediaDraftSaveResult {
                    accepted_revision: receipt.accepted_revision.clone().ok_or_else(stale)?,
                    draft: previous.dto()?,
                })
            } else {
                Err(stale())
            };
        }
        if previous.revision != expected || (previous.attempted && content.is_some()) {
            return Err(stale());
        }
        let mut assets = Vec::with_capacity(refs.len());
        let mut fresh = Vec::new();
        for id in refs {
            let prior = previous.assets.iter().find(|asset| asset.id == id);
            let asset = if let Some((asset, bytes)) = incoming.remove(id) {
                if let Some(prior) = prior {
                    if prior != &asset || *self.load_media_asset(&storage, prior)? != *bytes {
                        return Err(invalid());
                    }
                } else {
                    fresh.push((asset.clone(), bytes));
                }
                asset
            } else {
                prior.cloned().ok_or_else(invalid)?
            };
            assets.push(asset);
        }
        let revision = roda_types::new_id("media_draft");
        let mut save_receipts = previous.save_receipts.clone();
        save_receipts.push(SaveReceipt {
            input_revision: expected.into(),
            fingerprint,
            accepted_revision: Some(revision.clone()),
        });
        if save_receipts.len() > MAX_SAVE_RECEIPTS {
            save_receipts.remove(0);
        }
        let record = Record {
            revision,
            content,
            assets,
            attempted: false,
            last_accepted: previous.last_accepted.clone(),
            save_receipts,
        };
        validate(&record)?;
        self.draft_target(
            &scope.normal(),
            match &record.content {
                Some(Content::Attachments { reply_to, .. } | Content::Voice { reply_to, .. }) => {
                    reply_to.as_deref()
                }
                None => None,
            },
        )?;
        self.atomic_agent_action(|engine| {
            for (asset, bytes) in fresh {
                let encrypted =
                    crate::local_sealed::seal(&storage.key, &storage.asset_aad(&asset)?, &bytes)
                        .map_err(|_| storage_error())?;
                engine
                    .store
                    .set_meta(&storage.asset_key(&asset.id), &encrypted)?;
            }
            for asset in &previous.assets {
                if !record.assets.iter().any(|next| next.id == asset.id) {
                    engine.store.meta_delete(&storage.asset_key(&asset.id))?;
                }
            }
            engine.persist_media_record(&storage, &record)?;
            Ok(MediaDraftSaveResult {
                accepted_revision: record.revision.clone(),
                draft: record.dto()?,
            })
        })
    }
    fn media_draft_asset(
        &self,
        scope: &MediaDraftScope,
        expected: &str,
        asset_id: &str,
    ) -> R<Vec<u8>> {
        let storage = self.media_draft_scope(scope)?;
        let record = self.load_media_draft(&storage)?;
        if !intent_id(expected) || record.revision != expected {
            return Err(stale());
        }
        let asset = record
            .assets
            .iter()
            .find(|asset| asset.id == asset_id)
            .ok_or_else(invalid)?;
        Ok(self.load_media_asset(&storage, asset)?.to_vec())
    }
    fn send_media_draft(
        &mut self,
        scope: &MediaDraftScope,
        expected: &str,
    ) -> R<MediaDraftSendResult> {
        let storage = self.media_draft_scope(scope)?;
        let mut record = self.load_media_draft(&storage)?;
        if !intent_id(expected) {
            return Err(stale());
        }
        if let Some(receipt) = record
            .last_accepted
            .as_ref()
            .filter(|receipt| receipt.input_revision == expected)
        {
            let timeline = self.timeline(&scope.space_id)?;
            let entries = receipt
                .messages
                .iter()
                .map(|id| {
                    timeline
                        .iter()
                        .find(|entry| &entry.id == id)
                        .cloned()
                        .ok_or_else(stale)
                })
                .collect::<R<Vec<_>>>()?;
            return Ok(MediaDraftSendResult {
                entries,
                draft: record.dto()?,
            });
        }
        if record.revision != expected || expected == MEDIA_DRAFT_REVISION_ZERO {
            return Err(stale());
        }
        let preflight = (|| -> R<()> {
            let content = record.content.as_ref().ok_or_else(invalid)?;
            if !self
                .state
                .writer(&scope.space_id, &scope.expected_identity_id)
            {
                return Err(CoreError::Forbidden {
                    reason: t("só autores enviam mídia", "only writers can send media"),
                });
            }
            let reply = match content {
                Content::Attachments { reply_to, .. } | Content::Voice { reply_to, .. } => {
                    reply_to.as_deref()
                }
            };
            self.draft_target(&scope.normal(), reply)?;
            if matches!(content, Content::Voice { render: None, .. }) {
                return Err(CoreError::Invalid {
                    reason: t(
                        "prepare a prévia do áudio antes de enviar",
                        "prepare the audio preview before sending",
                    ),
                });
            }
            Ok(())
        })();
        if let Err(error) = preflight {
            return Err(if record.attempted {
                error
            } else {
                CoreError::DraftRejected {
                    reason: error.to_string(),
                }
            });
        }
        // Authenticate every source/derived asset before admitting the immutable intent.
        let bytes = record
            .assets
            .iter()
            .map(|asset| self.load_media_asset(&storage, asset))
            .collect::<R<Vec<_>>>()?;
        if !record.attempted {
            record.attempted = true;
            self.atomic_agent_action(|engine| engine.persist_media_record(&storage, &record))?;
        }
        self.atomic_agent_action(|engine| {
            let reply = match record.content.as_ref().expect("preflight") {
                Content::Attachments { reply_to, .. } | Content::Voice { reply_to, .. } => {
                    reply_to.as_deref().or(scope.thread_root_id.as_deref())
                }
            };
            let thread = scope.thread_root_id.is_some();
            let entries = match record.content.as_ref().expect("preflight") {
                Content::Attachments { items, .. } => {
                    let mut entries = Vec::with_capacity(items.len());
                    for ((item, asset), bytes) in items.iter().zip(&record.assets).zip(&bytes) {
                        entries.push(engine.send_file_message_with_caption(
                            &scope.space_id,
                            &asset.name,
                            &asset.mime,
                            bytes,
                            None,
                            reply,
                            thread,
                            &item.mutation_id,
                            &item.caption,
                        )?);
                    }
                    entries
                }
                Content::Voice {
                    render: Some(render),
                    ..
                } => vec![engine.send_voice_note(
                    &scope.space_id,
                    &bytes[1],
                    &render.mime,
                    render.duration_ms.round() as u64,
                    &render.levels,
                    &render.transcript,
                    reply,
                    thread,
                )?],
                _ => unreachable!("preflight"),
            };
            let cleared = Record {
                revision: roda_types::new_id("media_draft"),
                content: None,
                assets: Vec::new(),
                attempted: false,
                last_accepted: Some(Receipt {
                    input_revision: expected.into(),
                    messages: entries.iter().map(|entry| entry.id.clone()).collect(),
                }),
                save_receipts: record.save_receipts.clone(),
            };
            for asset in &record.assets {
                engine.store.meta_delete(&storage.asset_key(&asset.id))?;
            }
            engine.persist_media_record(&storage, &cleared)?;
            Ok(MediaDraftSendResult {
                entries,
                draft: cleared.dto()?,
            })
        })
    }
}

#[uniffi::export]
impl RodaEngine {
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

//! Typed binary Apps share the existing signed Item/FileDoc, encrypted chunks and
//! atomic event/outbox transaction. This module is not another document database.
use crate::{
    Client, CoreError,
    engine::{Engine, R},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use miniapps::{binary::Format, deckcraft};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use types::{
    AppDoc, Capability, EventBody, Grant, GrantScope, ItemContent, ItemKind, TrustLevel, new_id,
};

pub(crate) const MAX_BASE64_BYTES: usize = deckcraft::MAX_BYTES.div_ceil(3) * 4;
const MAX_RECEIPTS: usize = 64;
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AppBinaryDocumentDto {
    pub item_id: String,
    pub kind: String,
    pub version: u32,
    pub current_version: u32,
    pub sha256: String,
    pub byte_length: u32,
    pub data_base64: String,
    pub engine_revision: String,
    /// From the signed Item version, never a browser-supplied author preference.
    pub author_id: String,
    pub actor_id: String,
    pub actor_kind: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    id: String,
    fingerprint: String,
    intent: String,
    item: String,
    version: u32,
    sha256: String,
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: crate::i18n::t(
            "Documento deste App inválido ou incompatível.",
            "Invalid or unsupported App document.",
        ),
    }
}
fn unavailable() -> CoreError {
    CoreError::Forbidden {
        reason: crate::i18n::t("O acesso a este App mudou.", "Access to this App changed."),
    }
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: crate::i18n::t(
            "O App mudou; confira a versão atual.",
            "The App changed; review its current version.",
        ),
    }
}
fn storage() -> CoreError {
    CoreError::Storage {
        message: crate::i18n::t(
            "Não foi possível confirmar o salvamento deste App.",
            "The App receipt could not be verified.",
        ),
    }
}
fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

mod creation;
pub(crate) use creation::creation_item_id;

fn token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn decode(data: &str, format: Format) -> R<Vec<u8>> {
    if data.is_empty() || data.len() > MAX_BASE64_BYTES {
        return Err(invalid());
    }
    let bytes = STANDARD.decode(data).map_err(|_| invalid())?;
    if bytes.len() > format.max_bytes() || STANDARD.encode(&bytes) != data {
        return Err(invalid());
    }
    Ok(bytes)
}
fn format(app: &AppDoc) -> R<Format> {
    let format = Format::for_id(&app.app).ok_or_else(invalid)?;
    if app.resource_uri != format.resource() {
        return Err(invalid());
    }
    Ok(format)
}
fn app(app: &AppDoc) -> R<&types::FileDoc> {
    let format = format(app)?;
    let file = app.document.as_ref().ok_or_else(invalid)?;
    if file.mime != format.mime() || file.voice.is_some() || !file.path.is_empty() {
        return Err(invalid());
    }
    crate::files::validate_file(file, format.max_bytes() as u64)?;
    Ok(file)
}
impl Engine {
    fn binary_scope(&self, item: &str, write: bool) -> R<(String, String, String, u32, AppDoc)> {
        if !token(item) {
            return Err(invalid());
        }
        let state = self.item_state(item)?;
        let (me, device) = self.app_document_identity(&state.space)?;
        let ItemContent::App(document) = &state.current().content else {
            return Err(invalid());
        };
        app(document)?;
        if write
            && (!self.state.writer(&state.space, &me) || self.app_trust(item) < TrustLevel::Act)
        {
            return Err(unavailable());
        }
        Ok((
            me,
            device,
            state.space.clone(),
            state.current().number,
            document.clone(),
        ))
    }
    fn binary_receipts(&self, key: &str) -> R<Vec<Receipt>> {
        let receipts: Vec<Receipt> = self
            .store
            .meta(key)?
            .map(|s| serde_json::from_str(&s).map_err(|_| storage()))
            .transpose()?
            .unwrap_or_default();
        let mut ids = BTreeSet::new();
        if receipts.len() > MAX_RECEIPTS
            || receipts.iter().any(|r| {
                !token(&r.id)
                    || !ids.insert(&r.id)
                    || !token(&r.item)
                    || !digest(&r.fingerprint)
                    || !digest(&r.intent)
                    || !digest(&r.sha256)
                    || r.version == 0
            })
        {
            return Err(storage());
        }
        Ok(receipts)
    }
    fn binary_receipt(&mut self, key: &str, mut receipts: Vec<Receipt>, receipt: Receipt) -> R<()> {
        receipts.push(receipt);
        if receipts.len() > MAX_RECEIPTS {
            receipts.remove(0);
        }
        self.store.set_meta(
            key,
            &serde_json::to_string(&receipts).map_err(|_| storage())?,
        )?;
        Ok(())
    }
    pub(crate) fn app_binary_read(
        &self,
        item: &str,
        version: Option<u32>,
        expected_sha: Option<&str>,
    ) -> R<AppBinaryDocumentDto> {
        let (actor_id, _, _, current_version, _) = self.binary_scope(item, false)?;
        let actor_kind = if self
            .identities
            .get(&actor_id)
            .is_some_and(|i| i.owner.is_some())
        {
            "agent"
        } else {
            "human"
        };
        let number = version.unwrap_or(current_version);
        if number == 0 || expected_sha.is_some_and(|s| !digest(s)) {
            return Err(invalid());
        }
        let selected = self
            .item_state(item)?
            .versions
            .iter()
            .find(|v| v.number == number)
            .ok_or_else(stale)?;
        let ItemContent::App(document) = &selected.content else {
            return Err(invalid());
        };
        let format = format(document)?;
        let file = app(document)?;
        if expected_sha.is_some_and(|sha| sha != file.sha256) {
            return Err(stale());
        }
        let bytes = self
            .owned_file_bytes(file, format.max_bytes() as u64)?
            .ok_or_else(|| CoreError::Invalid {
                reason: crate::i18n::t(
                    "O documento deste App ainda está chegando.",
                    "The App document is still arriving.",
                ),
            })?;
        format
            .metadata(&bytes, &document.title)
            .map_err(|_| invalid())?;
        Ok(AppBinaryDocumentDto {
            item_id: item.into(),
            kind: format.id().into(),
            version: number,
            current_version,
            sha256: file.sha256.clone(),
            byte_length: bytes.len() as u32,
            data_base64: STANDARD.encode(bytes),
            engine_revision: format.revision().into(),
            author_id: selected.author.clone(),
            actor_id,
            actor_kind: actor_kind.into(),
        })
    }
    /// Resolve a durable command intent before executing an engine after a lost response.
    /// Missing/evicted receipts with a stale base refuse replay rather than guessing.
    pub(crate) fn app_binary_result(
        &self,
        item: &str,
        expected_version: u32,
        expected_sha: &str,
        mutation: &str,
        intent_sha: &str,
    ) -> R<Option<AppBinaryDocumentDto>> {
        if !token(mutation) || expected_version == 0 || !digest(expected_sha) || !digest(intent_sha)
        {
            return Err(invalid());
        }
        let (me, device, _, current_version, document) = self.binary_scope(item, true)?;
        let key = format!(
            "app_document_receipts:{}",
            hash(&serde_json::to_vec(&(&me, &device, item)).map_err(|_| invalid())?)
        );
        let intent = hash(
            &serde_json::to_vec(&(
                &me,
                &device,
                item,
                expected_version,
                expected_sha,
                intent_sha,
            ))
            .map_err(|_| invalid())?,
        );
        let receipts = self.binary_receipts(&key)?;
        if let Some(receipt) = receipts.iter().find(|r| r.id == mutation) {
            if receipt.item != item || receipt.intent != intent {
                return Err(invalid());
            }
            return self
                .app_binary_read(item, Some(receipt.version), Some(&receipt.sha256))
                .map(Some);
        }
        if expected_version != current_version || app(&document)?.sha256 != expected_sha {
            return Err(stale());
        }
        Ok(None)
    }
    pub(crate) fn app_binary_save(
        &mut self,
        item: &str,
        expected_version: u32,
        expected_sha: &str,
        data: &str,
        mutation: &str,
        intent_sha: &str,
    ) -> R<AppBinaryDocumentDto> {
        if !token(mutation) || expected_version == 0 || !digest(expected_sha) || !digest(intent_sha)
        {
            return Err(invalid());
        }
        let (me, device, space, current_version, previous) = self.binary_scope(item, true)?;
        let format = format(&previous)?;
        let bytes = decode(data, format)?;
        let metadata = format
            .metadata(&bytes, &previous.title)
            .map_err(|_| invalid())?;
        let sha = hash(&bytes);
        let owner = serde_json::to_vec(&(&me, &device, item)).map_err(|_| invalid())?;
        let key = format!("app_document_receipts:{}", hash(&owner));
        let fingerprint = hash(
            &serde_json::to_vec(&(
                &me,
                &device,
                item,
                expected_version,
                expected_sha,
                &sha,
                intent_sha,
                format.revision(),
            ))
            .map_err(|_| invalid())?,
        );
        let intent = hash(
            &serde_json::to_vec(&(
                &me,
                &device,
                item,
                expected_version,
                expected_sha,
                intent_sha,
            ))
            .map_err(|_| invalid())?,
        );
        let receipts = self.binary_receipts(&key)?;
        if let Some(r) = receipts.iter().find(|r| r.id == mutation) {
            if r.fingerprint != fingerprint || r.item != item || r.intent != intent {
                return Err(invalid());
            }
            let accepted = self.app_binary_read(item, Some(r.version), Some(&r.sha256))?;
            if accepted.sha256 != sha {
                return Err(storage());
            }
            return Ok(accepted);
        }
        if current_version != expected_version || app(&previous)?.sha256 != expected_sha {
            return Err(stale());
        }
        current_version.checked_add(1).ok_or_else(stale)?;
        self.atomic_agent_action(|e| {
            let mut prior = HashMap::new();
            for version in &e.item_state(item)?.versions {
                if let Some(file) = version.content.owned_file() {
                    for c in &file.chunks {
                        prior.insert(c.sha256.clone(), c.clone());
                    }
                }
            }
            let file_key = e.file_key(Some(item))?;
            let file = e.file_doc(
                &space,
                &file_key,
                format.filename(),
                "",
                format.mime(),
                &bytes,
                None,
                &prior,
            )?;
            e.store
                .set_meta(&format!("file_key:{item}"), &hex::encode(file_key))?;
            let next = AppDoc {
                state_json: metadata,
                document: Some(file),
                ..previous
            };
            // Every accepted intent gets an attributed version, including equal bytes.
            // The pinned Session's receipt contract advances exactly base+1.
            e.append(
                &space,
                &me,
                EventBody::ItemVersioned {
                    item: item.into(),
                    content: ItemContent::App(next),
                    note: match format {
                        Format::Deckcraft => {
                            crate::i18n::t("Apresentação atualizada", "Updated presentation")
                        }
                        Format::Gridcraft => {
                            crate::i18n::t("Planilha atualizada", "Updated spreadsheet")
                        }
                    },
                },
            )?;
            let version = e.item_state(item)?.current().number;
            e.binary_receipt(
                &key,
                receipts,
                Receipt {
                    id: mutation.into(),
                    fingerprint,
                    intent,
                    item: item.into(),
                    version,
                    sha256: sha,
                },
            )?;
            e.app_binary_read(item, Some(version), None)
        })
    }
}
#[uniffi::export]
impl Client {
    pub fn app_binary_read(
        &self,
        item_id: String,
        version: Option<u32>,
        expected_sha256: Option<String>,
    ) -> R<AppBinaryDocumentDto> {
        let _g = crate::i18n::enter(self.lang);
        self.lock()
            .app_binary_read(&item_id, version, expected_sha256.as_deref())
    }
    pub fn app_binary_create(
        &self,
        space_id: String,
        title: String,
        data_base64: String,
        mutation_id: String,
    ) -> R<AppBinaryDocumentDto> {
        let _g = crate::i18n::enter(self.lang);
        self.lock()
            .app_binary_create(&space_id, &title, &data_base64, &mutation_id)
    }
    pub fn app_binary_create_for(
        &self,
        app_id: String,
        space_id: String,
        title: String,
        data_base64: String,
        mutation_id: String,
    ) -> R<AppBinaryDocumentDto> {
        let _g = crate::i18n::enter(self.lang);
        let format = Format::for_id(&app_id).ok_or_else(invalid)?;
        self.lock()
            .app_binary_create_format(format, &space_id, &title, &data_base64, &mutation_id)
    }
    pub fn app_binary_result(
        &self,
        item_id: String,
        expected_version: u32,
        expected_sha256: String,
        mutation_id: String,
        intent_sha256: String,
    ) -> R<Option<AppBinaryDocumentDto>> {
        let _g = crate::i18n::enter(self.lang);
        self.lock().app_binary_result(
            &item_id,
            expected_version,
            &expected_sha256,
            &mutation_id,
            &intent_sha256,
        )
    }
    pub fn app_binary_save(
        &self,
        item_id: String,
        expected_version: u32,
        expected_sha256: String,
        data_base64: String,
        mutation_id: String,
        intent_sha256: String,
    ) -> R<AppBinaryDocumentDto> {
        let _g = crate::i18n::enter(self.lang);
        self.lock().app_binary_save(
            &item_id,
            expected_version,
            &expected_sha256,
            &data_base64,
            &mutation_id,
            &intent_sha256,
        )
    }
}
#[cfg(test)]
mod tests;

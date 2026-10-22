//! Canonical app-owned documents: one signed App Item, encrypted file chunks, checked saves.
use crate::engine::{Engine, R};
use crate::i18n::t;
use crate::{Client, CoreError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use types::{
    new_id, AppDoc, Capability, EventBody, Grant, GrantScope, ItemContent, ItemKind, TrustLevel,
};

pub(crate) use miniapps::excalidraw::MAX_SCENE_BYTES;
use miniapps::excalidraw::{Scene, EMPTY, MIME, RESOURCE};
const MAX_RECEIPTS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AppDocumentDto {
    pub item_id: String,
    pub version: u32,
    pub sha256: String,
    pub scene_json: String,
    // The accepted version may precede a later edit when an acknowledgement is retried.
    pub current_version: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    id: String,
    fingerprint: String,
    version: u32,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CopyReceipt {
    id: String,
    fingerprint: String,
    item_id: String,
    sha256: String,
}

fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: t(
            "documento de desenho inválido ou grande demais",
            "invalid or oversized drawing document",
        ),
    }
}
fn unavailable() -> CoreError {
    CoreError::Forbidden {
        reason: t(
            "o acesso a este desenho mudou",
            "access to this drawing changed",
        ),
    }
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: t(
            "o desenho mudou; revise a versão atual antes de salvar",
            "the drawing changed; review the current version before saving",
        ),
    }
}
fn storage() -> CoreError {
    CoreError::Storage {
        message: t(
            "não foi possível verificar o recibo do desenho",
            "could not verify the drawing receipt",
        ),
    }
}
fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn validate_scene(text: &str) -> R<Scene> {
    Scene::parse(text).map_err(|_| invalid())
}
fn metadata(scene: &Scene) -> String {
    scene.metadata()
}

#[uniffi::export]
impl Client {
    pub fn app_document_read(&self, item_id: String) -> R<AppDocumentDto> {
        let _g = crate::i18n::enter(self.lang);
        self.lock().app_document_read(&item_id)
    }
    pub fn app_document_copy(
        &self,
        item_id: String,
        scene_json: String,
        mutation_id: String,
    ) -> R<AppDocumentDto> {
        let _g = crate::i18n::enter(self.lang);
        self.lock()
            .app_document_copy(&item_id, &scene_json, &mutation_id)
    }
    pub fn app_document_save(
        &self,
        item_id: String,
        expected_version: u32,
        expected_sha256: String,
        scene_json: String,
        mutation_id: String,
    ) -> R<AppDocumentDto> {
        let _g = crate::i18n::enter(self.lang);
        self.lock().app_document_save(
            &item_id,
            expected_version,
            &expected_sha256,
            &scene_json,
            &mutation_id,
        )
    }
}

#[cfg(test)]
mod tests;

mod admission;
mod copies;
mod saving;

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

impl Engine {
    pub(crate) fn attach_app_document(
        &mut self,
        space: &str,
        item: &str,
        mut app: AppDoc,
    ) -> R<AppDoc> {
        if app.app != "excalidraw" {
            return Ok(app);
        }
        let key = self.file_key(None)?;
        let file = self.file_doc(
            space,
            &key,
            "drawing.excalidraw",
            "",
            MIME,
            EMPTY.as_bytes(),
            None,
            &HashMap::new(),
        )?;
        self.store
            .set_meta(&format!("file_key:{item}"), &hex::encode(key))?;
        app.document = Some(file);
        let mut compact: Value =
            serde_json::from_str(&metadata(&validate_scene(EMPTY)?)).map_err(|_| invalid())?;
        compact["title"] = json!(app.title);
        app.state_json = compact.to_string();
        Ok(app)
    }
    /// Shared with the file-byte accessor so an App document cannot bypass its owner gate.
    pub(crate) fn app_document_identity(&self, space: &str) -> R<(String, String)> {
        let me = self.me_id()?;
        let device = if let Some(account) = self.account() {
            self.native_surface_owner(&account.identity, &account.device)?;
            account.device.clone()
        } else {
            String::new()
        };
        if !self.is_member(space, &me)
            || self.broken_spaces.contains_key(space)
            || self.space_state(space)?.integrity_error.is_some()
        {
            return Err(unavailable());
        }
        Ok((me, device))
    }
    fn app_document_scope(&self, item: &str) -> R<(String, String, String, u32, AppDoc)> {
        let state = self.item_state(item)?;
        let (me, device) = self.app_document_identity(&state.space)?;
        let ItemContent::App(app) = &state.current().content else {
            return Err(invalid());
        };
        if app.app != "excalidraw" || app.resource_uri != RESOURCE || app.document.is_none() {
            return Err(invalid());
        }
        let file = app.document.as_ref().expect("checked");
        if file.mime != MIME || file.voice.is_some() {
            return Err(invalid());
        }
        crate::files::validate_file(file, MAX_SCENE_BYTES as u64)?;
        Ok((
            me,
            device,
            state.space.clone(),
            state.current().number,
            app.clone(),
        ))
    }
    pub(crate) fn app_document_read(&self, item: &str) -> R<AppDocumentDto> {
        let (_, _, _, version, app) = self.app_document_scope(item)?;
        let file = app.document.as_ref().expect("checked");
        let bytes = self
            .owned_file_bytes(file, MAX_SCENE_BYTES as u64)?
            .ok_or_else(|| CoreError::Invalid {
                reason: t(
                    "o desenho ainda está chegando; tente novamente",
                    "the drawing is still arriving; try again",
                ),
            })?;
        let scene_json = String::from_utf8(bytes).map_err(|_| invalid())?;
        validate_scene(&scene_json)?;
        Ok(AppDocumentDto {
            item_id: item.into(),
            version,
            current_version: version,
            sha256: file.sha256.clone(),
            scene_json,
        })
    }
    pub(crate) fn app_document_save(
        &mut self,
        item: &str,
        expected_version: u32,
        expected_sha256: &str,
        scene_json: &str,
        mutation_id: &str,
    ) -> R<AppDocumentDto> {
        if !valid_id(mutation_id)
            || !valid_hash(expected_sha256)
            || expected_version == 0
            || scene_json.len() > MAX_SCENE_BYTES
        {
            return Err(invalid());
        }
        let (me, device, space, current_version, app) = self.app_document_scope(item)?;
        let new_hash = hash(scene_json.as_bytes());
        let fingerprint = hash(
            &serde_json::to_vec(&(
                &me,
                &device,
                item,
                expected_version,
                expected_sha256,
                &new_hash,
            ))
            .map_err(|_| invalid())?,
        );
        let receipt_key = format!(
            "app_document_receipts:{}",
            hash(&serde_json::to_vec(&(&me, &device, item)).map_err(|_| invalid())?)
        );
        let mut receipts: Vec<Receipt> = self
            .store
            .meta(&receipt_key)?
            .map(|s| serde_json::from_str(&s).map_err(|_| storage()))
            .transpose()?
            .unwrap_or_default();
        let mut receipt_ids = BTreeSet::new();
        if receipts.len() > MAX_RECEIPTS
            || receipts.iter().any(|r| {
                !receipt_ids.insert(&r.id)
                    || !valid_id(&r.id)
                    || !valid_hash(&r.fingerprint)
                    || !valid_hash(&r.sha256)
                    || r.version == 0
            })
        {
            return Err(storage());
        }
        if let Some(r) = receipts.iter().find(|r| r.id == mutation_id) {
            if r.fingerprint != fingerprint {
                return Err(invalid());
            }
            let accepted = self
                .item_state(item)?
                .versions
                .iter()
                .find(|v| v.number == r.version)
                .and_then(|v| v.content.owned_file())
                .filter(|f| f.sha256 == r.sha256)
                .ok_or_else(storage)?;
            if accepted.sha256 != new_hash {
                return Err(storage());
            }
            return Ok(AppDocumentDto {
                item_id: item.into(),
                version: r.version,
                current_version,
                sha256: r.sha256.clone(),
                scene_json: scene_json.into(),
            });
        }
        if !self.state.writer(&space, &me) || self.app_trust(item) < TrustLevel::Act {
            return Err(unavailable());
        }
        let current = app.document.as_ref().expect("checked");
        if expected_version != current_version || expected_sha256 != current.sha256 {
            return Err(stale());
        }
        let scene = validate_scene(scene_json)?;
        self.atomic_agent_action(|e| {
            let version = if new_hash == current.sha256 {
                current_version
            } else {
                let mut prior = HashMap::new();
                for v in &e.item_state(item)?.versions {
                    if let Some(f) = v.content.owned_file() {
                        for c in &f.chunks {
                            prior.insert(c.sha256.clone(), c.clone());
                        }
                    }
                }
                let key = e.file_key(Some(item))?;
                let file = e.file_doc(
                    &space,
                    &key,
                    "drawing.excalidraw",
                    "",
                    MIME,
                    scene_json.as_bytes(),
                    None,
                    &prior,
                )?;
                e.store
                    .set_meta(&format!("file_key:{item}"), &hex::encode(key))?;
                let mut compact: Value =
                    serde_json::from_str(&metadata(&scene)).map_err(|_| invalid())?;
                compact["title"] = json!(app.title);
                let next = AppDoc {
                    document: Some(file),
                    state_json: compact.to_string(),
                    ..app
                };
                e.append(
                    &space,
                    &me,
                    EventBody::ItemVersioned {
                        item: item.into(),
                        content: ItemContent::App(next),
                        note: t("Atualizou o desenho", "Updated the drawing"),
                    },
                )?;
                e.item_state(item)?.current().number
            };
            receipts.push(Receipt {
                id: mutation_id.into(),
                fingerprint,
                version,
                sha256: new_hash.clone(),
            });
            if receipts.len() > MAX_RECEIPTS {
                receipts.remove(0);
            }
            e.store.set_meta(
                &receipt_key,
                &serde_json::to_string(&receipts).map_err(|_| storage())?,
            )?;
            Ok(AppDocumentDto {
                item_id: item.into(),
                version,
                current_version: version,
                sha256: new_hash,
                scene_json: scene_json.into(),
            })
        })
    }
    /// Copy is an explicit conflict recovery action, never an automatic overwrite.
    pub(crate) fn app_document_copy(
        &mut self,
        item: &str,
        scene_json: &str,
        mutation_id: &str,
    ) -> R<AppDocumentDto> {
        if !valid_id(mutation_id) {
            return Err(invalid());
        }
        let scene = validate_scene(scene_json)?;
        let (me, device, space, _, app) = self.app_document_scope(item)?;
        let sha = hash(scene_json.as_bytes());
        let fingerprint =
            hash(&serde_json::to_vec(&(&me, &device, item, &sha)).map_err(|_| invalid())?);
        let key = format!(
            "app_document_copy_receipts:{}",
            hash(&serde_json::to_vec(&(&me, &device, item)).map_err(|_| invalid())?)
        );
        let mut receipts: Vec<CopyReceipt> = self
            .store
            .meta(&key)?
            .map(|s| serde_json::from_str(&s).map_err(|_| storage()))
            .transpose()?
            .unwrap_or_default();
        let mut ids = BTreeSet::new();
        if receipts.len() > MAX_RECEIPTS
            || receipts.iter().any(|r| {
                !valid_id(&r.id)
                    || !ids.insert(&r.id)
                    || !valid_id(&r.item_id)
                    || !valid_hash(&r.fingerprint)
                    || !valid_hash(&r.sha256)
            })
        {
            return Err(storage());
        }
        if let Some(r) = receipts.iter().find(|r| r.id == mutation_id) {
            if r.fingerprint != fingerprint {
                return Err(invalid());
            }
            let (_, _, accepted_space, current_version, _) = self.app_document_scope(&r.item_id)?;
            let accepted = self
                .item_state(&r.item_id)?
                .versions
                .first()
                .and_then(|v| v.content.owned_file())
                .filter(|f| f.sha256 == sha && f.sha256 == r.sha256)
                .ok_or_else(storage)?;
            if accepted_space != space {
                return Err(storage());
            }
            return Ok(AppDocumentDto {
                item_id: r.item_id.clone(),
                version: 1,
                current_version,
                sha256: accepted.sha256.clone(),
                scene_json: scene_json.into(),
            });
        }
        // The canonical Item also fences an evicted receipt: an old intent must
        // never create a second copy merely because its local ACK was retired.
        let copy_item = format!(
            "it_copy_{}",
            hash(&serde_json::to_vec(&(&me, &device, item, mutation_id)).map_err(|_| invalid())?)
        );
        if self.state.items.contains_key(&copy_item) {
            return Err(stale());
        }
        if !self.state.writer(&space, &me) {
            return Err(unavailable());
        }
        self.atomic_agent_action(|e| {
            let title = format!("{} ({})", app.title, t("cópia", "copy"));
            let new_item = e.create_app_document_copy(
                &space,
                &me,
                app,
                &title,
                scene_json,
                &scene,
                false,
                &t(
                    "Mini-app adicionado a este Espaço.",
                    "Mini-app added to this Space.",
                ),
                Some(copy_item),
            )?;
            receipts.push(CopyReceipt {
                id: mutation_id.into(),
                fingerprint,
                item_id: new_item.clone(),
                sha256: sha.clone(),
            });
            if receipts.len() > MAX_RECEIPTS {
                receipts.remove(0);
            }
            e.store.set_meta(
                &key,
                &serde_json::to_string(&receipts).map_err(|_| storage())?,
            )?;
            Ok(AppDocumentDto {
                item_id: new_item,
                version: 1,
                current_version: 1,
                sha256: sha,
                scene_json: scene_json.into(),
            })
        })
    }

    /// A new space gets a new owned Item and encryption key; source references stay home.
    pub(crate) fn forward_app_document(
        &mut self,
        from: &str,
        item: &str,
        to: &str,
        caption: &str,
    ) -> R<crate::TimelineEntry> {
        let (me, _, source_space, _, app) = self.app_document_scope(item)?;
        if source_space != from || !self.state.writer(to, &me) {
            return Err(unavailable());
        }
        let read = self.app_document_read(item)?;
        let scene = validate_scene(&read.scene_json)?;
        self.atomic_agent_action(|e| {
            let title = app.title.clone();
            let copied = e.create_app_document_copy(
                to, &me, app, &title, &read.scene_json, &scene, true, caption, None,
            )?;
            let entry = e.space_state(to)?.entries.iter().find(|entry| {
                matches!(&entry.body, crate::engine::EntryBody::Message { attaches: Some(id), .. } if id == &copied)
            }).ok_or_else(storage)?;
            Ok(e.entry_dto(entry))
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn create_app_document_copy(
        &mut self,
        space: &str,
        me: &str,
        app: AppDoc,
        title: &str,
        text: &str,
        scene: &Scene,
        forwarded: bool,
        caption: &str,
        item_id: Option<String>,
    ) -> R<String> {
        let item = item_id.unwrap_or_else(|| new_id("it"));
        let key = self.file_key(None)?;
        let file = self.file_doc(
            space,
            &key,
            "drawing.excalidraw",
            "",
            MIME,
            text.as_bytes(),
            None,
            &HashMap::new(),
        )?;
        self.store
            .set_meta(&format!("file_key:{item}"), &hex::encode(key))?;
        let mut compact: Value = serde_json::from_str(&metadata(scene)).map_err(|_| invalid())?;
        compact["title"] = json!(title);
        self.append(
            space,
            me,
            EventBody::ItemCreated {
                item: item.clone(),
                kind: ItemKind::App,
                content: ItemContent::App(AppDoc {
                    title: title.into(),
                    state_json: compact.to_string(),
                    document: Some(file),
                    ..app
                }),
                origin: t("Cópia de desenho", "Drawing copy"),
            },
        )?;
        self.append(
            space,
            me,
            EventBody::GrantIssued {
                grant: Grant {
                    id: new_id("gr"),
                    grantor: me.into(),
                    grantee: Some(format!("app:{item}")),
                    scope: GrantScope::Item(item.clone()),
                    capability: Capability::Trust(TrustLevel::Act),
                    expires_at_ms: None,
                },
            },
        )?;
        self.append(
            space,
            me,
            EventBody::MessagePosted {
                message: new_id("msg"),
                text: caption.into(),
                attaches: Some(item.clone()),
                reply: None,
                forwarded,
            },
        )?;
        Ok(item)
    }
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

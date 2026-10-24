//! Canonical drawing adapter. Scene policy is shared Rust; original bytes remain authoritative.
use super::*;
use crate::media::{self, Sealed};
use miniapps::excalidraw::{Scene, EMPTY, MAX_SCENE_BYTES, MIME, RESOURCE};
use types::{AppDoc, Capability, Grant, GrantScope, TrustLevel};

// JSON escaping can expand a valid UTF-8 scene sixfold. Other document commands keep 1 MiB.
pub(super) const MAX_REQUEST_BYTES: usize = MAX_SCENE_BYTES * 6 + 4096;
const MAX_RECEIPTS: usize = 64;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Journal {
    saves: BTreeMap<String, Vec<Receipt>>,
    copies: BTreeMap<String, Vec<CopyReceipt>>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    id: String,
    fingerprint: String,
    version: u32,
    sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CopyReceipt {
    id: String,
    fingerprint: String,
    item_id: String,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    #[serde(rename = "app_document_read")]
    Read { item_id: String },
    #[serde(rename = "app_document_save")]
    Save {
        item_id: String,
        expected_version: u32,
        expected_sha256: String,
        scene_json: String,
        mutation_id: String,
    },
    #[serde(rename = "app_document_copy")]
    Copy {
        item_id: String,
        scene_json: String,
        mutation_id: String,
    },
}

fn invalid() -> String {
    t(
        "documento de desenho inválido ou grande demais",
        "invalid or oversized drawing document",
    )
}
fn unavailable() -> String {
    t(
        "o acesso a este desenho mudou",
        "access to this drawing changed",
    )
}
fn stale() -> String {
    t(
        "o desenho mudou; revise a versão atual antes de salvar",
        "the drawing changed; review the current version before saving",
    )
}
fn storage() -> String {
    t(
        "não foi possível verificar o recibo do desenho",
        "could not verify the drawing receipt",
    )
}
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn fingerprint(value: &impl Serialize) -> Result<String, String> {
    Ok(hash(&serde_json::to_vec(value).map_err(error)?))
}
fn dto(item: &str, version: u32, current: u32, sha: &str, text: &str) -> Value {
    json!({"item_id":item,"version":version,"current_version":current,"sha256":sha,"scene_json":text})
}
fn compact(scene: &Scene, title: &str) -> Result<String, String> {
    let mut value: Value = serde_json::from_str(&scene.metadata()).map_err(error)?;
    value["title"] = json!(title);
    Ok(value.to_string())
}

impl<S: Signer> Client<S> {
    fn drawing_identity(&self) -> Result<(), String> {
        if self.saved.membership.revoked.contains(&self.account.device) {
            return Err(unavailable());
        }
        Ok(())
    }

    pub(super) fn app_document_access(&self, space: &str) -> Result<(), String> {
        self.drawing_identity()?;
        if !self.roster(space).contains_key(&self.account.identity)
            || self.saved.integrity_errors.contains_key(space)
        {
            return Err(unavailable());
        }
        Ok(())
    }

    fn drawing_scope(&self, id: &str) -> Result<(Item, AppDoc), String> {
        let item = self.document_item(id)?;
        self.app_document_access(&item.space)?;
        let ItemContent::App(app) = &item.current().content else {
            return Err(invalid());
        };
        let file = app.document.as_ref().ok_or_else(invalid)?;
        if item.kind != ItemKind::App
            || app.app != "excalidraw"
            || app.resource_uri != RESOURCE
            || file.mime != MIME
        {
            return Err(invalid());
        }
        media::validate_owned_file(file, MAX_SCENE_BYTES).map_err(|_| invalid())?;
        Ok((item.clone(), app.clone()))
    }

    fn drawing_key(&self, item: &str) -> Result<String, String> {
        fingerprint(&(&self.account.identity, &self.account.device, item))
    }

    fn read_drawing(&self, id: &str) -> Result<(Value, Vec<ChunkRef>), String> {
        let (item, app) = self.drawing_scope(id)?;
        let file = app.document.as_ref().ok_or_else(invalid)?;
        let mut missing = Vec::new();
        let mut joined = Vec::with_capacity(file.bytes as usize);
        for chunk in &file.chunks {
            let plain = if let Some(bytes) = self.saved.documents.blobs.get(&chunk.sha256) {
                Some(bytes.clone())
            } else if let Some(sealed) = self.saved.local_chunks.get(&chunk.blob) {
                Some(media::open_chunk(chunk, sealed).map_err(|_| invalid())?)
            } else {
                None
            };
            if let Some(bytes) = plain {
                if bytes.len() as u64 != chunk.bytes || hash(&bytes) != chunk.sha256 {
                    return Err(invalid());
                }
                joined.extend(bytes);
            } else {
                missing.push(chunk.clone());
            }
        }
        if !missing.is_empty() {
            return Ok((Value::Null, missing));
        }
        if joined.len() as u64 != file.bytes || hash(&joined) != file.sha256 {
            return Err(invalid());
        }
        let text = String::from_utf8(joined).map_err(|_| invalid())?;
        Scene::parse(&text).map_err(|_| invalid())?;
        let version = item.versions.len() as u32;
        Ok((dto(id, version, version, &file.sha256, &text), Vec::new()))
    }

    /// Preparation can advance MLS sender state. Restore it if any body cannot queue;
    /// queue_domain_batch itself commits no outbox entry until every body is prepared.
    fn drawing_batch(
        &mut self,
        space: &str,
        bodies: Vec<EventBody>,
        uploads: &[String],
        now: i64,
    ) -> Result<(), String> {
        let before = self
            .provider
            .storage()
            .values
            .read()
            .map_err(|_| storage())?
            .clone();
        let order = self.saved.outbox_sequence;
        if let Err(reason) = self.queue_domain_batch(space, bodies, now) {
            *self
                .provider
                .storage()
                .values
                .write()
                .map_err(|_| storage())? = before;
            return Err(reason);
        }
        for pending in self
            .saved
            .outbox
            .values_mut()
            .filter(|pending| pending.order.is_some_and(|value| value > order))
        {
            pending.blob_uploads = uploads.to_vec();
        }
        Ok(())
    }

    fn drawing_uploads(&self, sealed: &Sealed) -> Result<Vec<String>, String> {
        let mut uploads = sealed
            .uploads
            .iter()
            .map(|upload| upload.blob.clone())
            .collect::<BTreeSet<_>>();
        // A second offline save may reuse a chunk whose first upload is still pending.
        for pending in self.saved.outbox.values() {
            for blob in &pending.blob_uploads {
                if sealed.doc.chunks.iter().any(|chunk| chunk.blob == *blob) {
                    if !self.saved.local_chunks.contains_key(blob) {
                        return Err(storage());
                    }
                    uploads.insert(blob.clone());
                }
            }
        }
        Ok(uploads.into_iter().collect())
    }

    fn cache_drawing(&mut self, sealed: Sealed, text: &str) {
        let mut offset = 0;
        for chunk in &sealed.doc.chunks {
            let end = offset + chunk.bytes as usize;
            self.saved
                .documents
                .blobs
                .insert(chunk.sha256.clone(), text.as_bytes()[offset..end].to_vec());
            offset = end;
        }
        for upload in sealed.uploads {
            self.saved.local_chunks.insert(upload.blob, upload.sealed);
        }
    }

    pub(in crate::client) fn install_browser_drawing(
        &mut self,
        space: &str,
        args: &str,
        now: i64,
    ) -> Result<DocumentResponse, String> {
        self.drawing_identity()?;
        self.document_writer(space)?;
        let arguments: Value = serde_json::from_str(args).map_err(error)?;
        if !arguments.is_object() {
            return Err(invalid());
        }
        let profile = self.saved.profiles.get(&self.account.identity);
        let actor = miniapps::Actor {
            id: &self.account.identity,
            name: profile
                .map(|profile| profile.name.as_str())
                .unwrap_or(&self.account.name),
            owner: profile.and_then(|profile| profile.owner.as_deref()),
        };
        let (title, _) = miniapps::create_as("excalidraw", &arguments, &actor, now)?;
        let item = browser_id("it", now)?;
        let app = AppDoc {
            app: "excalidraw".into(),
            resource_uri: RESOURCE.into(),
            title: title.clone(),
            state_json: String::new(),
            document: None,
        };
        self.create_drawing(
            space,
            &item,
            app,
            &title,
            EMPTY,
            &Scene::parse(EMPTY).map_err(|_| invalid())?,
            now,
        )?;
        let result = self.item_value(&self.document_item(&item)?, None, &mut Vec::new())?;
        Ok(DocumentResponse {
            result,
            update: Some(self.update(Vec::new())?),
            downloads: Vec::new(),
            uploads: self.pending_document_uploads(),
        })
    }

    pub(super) fn drawing_request(
        &mut self,
        request: &str,
        now: i64,
    ) -> Result<DocumentResponse, String> {
        let command: Command = serde_json::from_str(request).map_err(error)?;
        let mut downloads = Vec::new();
        let (result, mutated) = match command {
            Command::Read { item_id } => {
                let (result, missing) = self.read_drawing(&item_id)?;
                downloads = missing;
                (result, false)
            }
            Command::Save {
                item_id,
                expected_version,
                expected_sha256,
                scene_json,
                mutation_id,
            } => (
                self.save_drawing(
                    &item_id,
                    expected_version,
                    &expected_sha256,
                    &scene_json,
                    &mutation_id,
                    now,
                )?,
                true,
            ),
            Command::Copy {
                item_id,
                scene_json,
                mutation_id,
            } => (
                self.copy_drawing(&item_id, &scene_json, &mutation_id, now)?,
                true,
            ),
        };
        Ok(DocumentResponse {
            result,
            update: if mutated {
                Some(self.update(Vec::new())?)
            } else {
                None
            },
            downloads,
            uploads: if mutated {
                self.pending_document_uploads()
            } else {
                Vec::new()
            },
        })
    }
}

mod mutations;

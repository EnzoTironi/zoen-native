//! Saving on the original Engine and canonical document state.

use super::{
    hash, invalid, json, metadata, stale, storage, t, unavailable, valid_hash, valid_id,
    validate_scene, AppDoc, AppDocumentDto, BTreeSet, Engine, EventBody, HashMap, ItemContent,
    Receipt, TrustLevel, Value, MAX_RECEIPTS, MAX_SCENE_BYTES, MIME, R,
};

impl Engine {
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
}

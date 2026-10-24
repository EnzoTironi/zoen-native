//! Copies on the original Engine and canonical document state.

use super::{
    hash, invalid, json, metadata, new_id, stale, storage, t, unavailable, valid_hash, valid_id,
    validate_scene, AppDoc, AppDocumentDto, BTreeSet, Capability, CopyReceipt, Digest, Engine,
    EventBody, Grant, GrantScope, HashMap, ItemContent, ItemKind, Scene, TrustLevel, Value,
    MAX_RECEIPTS, MIME, R,
};

impl Engine {
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
                &t("App adicionado a este Espaço.", "App added to this Space."),
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
    pub(super) fn create_app_document_copy(
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
        let item = match item_id {
            Some(item) => item,
            None => new_id("it")?,
        };
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
                    id: new_id("gr")?,
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
                message: new_id("msg")?,
                text: caption.into(),
                attaches: Some(item.clone()),
                reply: None,
                media: None,
                forwarded,
            },
        )?;
        Ok(item)
    }
}

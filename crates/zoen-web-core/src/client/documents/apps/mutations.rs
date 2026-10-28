//! Implementation within the existing browser client authority.
use super::{
    browser_id, compact, dto, fingerprint, hash, invalid, media, stale, storage, t, unavailable,
    valid_hash, valid_id, AppDoc, BTreeSet, Capability, Client, CopyReceipt, EventBody, Grant,
    GrantScope, ItemContent, ItemKind, Receipt, Role, Scene, Signer, TrustLevel, Value,
    MAX_RECEIPTS, MAX_SCENE_BYTES, MIME,
};

fn validate_save_receipts(receipts: &[Receipt]) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    if receipts.len() > MAX_RECEIPTS
        || receipts.iter().any(|r| {
            !ids.insert(&r.id)
                || !valid_id(&r.id)
                || !valid_hash(&r.fingerprint)
                || !valid_hash(&r.sha256)
                || r.version == 0
        })
    {
        return Err(storage());
    }
    Ok(())
}

impl<S: Signer> Client<S> {
    fn drawing_write_allowed(&self, item: &super::Item, now: i64) -> Result<(), String> {
        if self
            .roster(&item.space)
            .get(&self.account.identity)
            .is_none_or(|role| *role == Role::Reader)
        {
            return Err(unavailable());
        }
        let trust = self
            .active_app_grants(&item.id, now)?
            .into_iter()
            .rev()
            .find_map(|grant| match grant.capability {
                Capability::Trust(level) => Some(level),
                _ => None,
            })
            .unwrap_or(TrustLevel::Listen);
        if trust < TrustLevel::Act {
            return Err(unavailable());
        }
        Ok(())
    }

    pub(super) fn save_drawing(
        &mut self,
        id: &str,
        expected: u32,
        expected_sha: &str,
        text: &str,
        mutation: &str,
        now: i64,
    ) -> Result<Value, String> {
        if !valid_id(mutation)
            || !valid_hash(expected_sha)
            || expected == 0
            || text.len() > MAX_SCENE_BYTES
        {
            return Err(invalid());
        }
        let (item, app) = self.drawing_scope(id)?;
        let current_version =
            u32::try_from(item.versions.len()).map_err(|_| "document version limit exceeded")?;
        let sha = hash(text.as_bytes());
        let fp = fingerprint(&(
            &self.account.identity,
            &self.account.device,
            id,
            expected,
            expected_sha,
            &sha,
        ))?;
        let key = self.drawing_key(id)?;
        let mut receipts = self
            .saved
            .documents
            .drawings
            .saves
            .get(&key)
            .cloned()
            .unwrap_or_default();
        validate_save_receipts(&receipts)?;
        if let Some(receipt) = receipts.iter().find(|r| r.id == mutation) {
            if receipt.fingerprint != fp {
                return Err(invalid());
            }
            let accepted = item
                .versions
                .get(receipt.version as usize - 1)
                .and_then(|version| version.content.owned_file())
                .ok_or_else(storage)?;
            if accepted.sha256 != receipt.sha256 || accepted.sha256 != sha {
                return Err(storage());
            }
            return Ok(dto(id, receipt.version, current_version, &sha, text));
        }
        // A no-op save records an immutable receipt without requiring a new MLS
        // encryption. Actual version publication still passes drawing_batch's epoch gate.
        self.drawing_write_allowed(&item, now)?;
        let current = app.document.as_ref().ok_or_else(invalid)?;
        if expected != current_version || expected_sha != current.sha256 {
            return Err(stale());
        }
        let scene = Scene::parse(text).map_err(|_| invalid())?;
        let version = if sha == current.sha256 {
            current_version
        } else {
            let next_version = current_version.checked_add(1).ok_or_else(storage)?;
            let prior = item
                .versions
                .iter()
                .filter_map(|version| version.content.owned_file())
                .flat_map(|file| file.chunks.iter().cloned())
                .collect::<Vec<_>>();
            let sealed =
                media::seal_file_reusing("drawing.excalidraw", MIME, text.as_bytes(), &prior)?;
            let uploads = self.drawing_uploads(&sealed)?;
            let next = AppDoc {
                document: Some(sealed.doc.clone()),
                state_json: compact(&scene, &app.title)?,
                ..app
            };
            self.drawing_batch(
                &item.space,
                vec![EventBody::ItemVersioned {
                    item: id.into(),
                    content: ItemContent::App(next),
                    note: t("Atualizou o desenho", "Updated the drawing"),
                }],
                &uploads,
                now,
            )?;
            self.cache_drawing(sealed, text)?;
            next_version
        };
        receipts.push(Receipt {
            id: mutation.into(),
            fingerprint: fp,
            version,
            sha256: sha.clone(),
        });
        if receipts.len() > MAX_RECEIPTS {
            receipts.remove(0);
        }
        self.saved.documents.drawings.saves.insert(key, receipts);
        Ok(dto(id, version, version, &sha, text))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn create_drawing(
        &mut self,
        space: &str,
        item: &str,
        app: AppDoc,
        title: &str,
        text: &str,
        scene: &Scene,
        now: i64,
    ) -> Result<(), String> {
        let sealed = media::seal_file("drawing.excalidraw", MIME, text.as_bytes(), None)?;
        let uploads = self.drawing_uploads(&sealed)?;
        let app = AppDoc {
            title: title.into(),
            state_json: compact(scene, title)?,
            document: Some(sealed.doc.clone()),
            ..app
        };
        self.drawing_batch(
            space,
            vec![
                EventBody::ItemCreated {
                    item: item.into(),
                    kind: ItemKind::App,
                    content: ItemContent::App(app),
                    origin: t("Cópia de desenho", "Drawing copy"),
                },
                EventBody::GrantIssued {
                    grant: Grant {
                        id: browser_id("gr", now)?,
                        grantor: self.account.identity.clone(),
                        grantee: Some(format!("app:{item}")),
                        scope: GrantScope::Item(item.into()),
                        capability: Capability::Trust(TrustLevel::Act),
                        expires_at_ms: None,
                    },
                },
                EventBody::MessagePosted {
                    message: types::new_secret_id("msg").map_err(|error| error.to_string())?,
                    text: t("App adicionado a este Espaço.", "App added to this Space."),
                    attaches: Some(item.into()),
                    reply: None,
                    media: None,
                    forwarded: false,
                },
            ],
            &uploads,
            now,
        )?;
        self.cache_drawing(sealed, text)?;
        Ok(())
    }

    pub(super) fn copy_drawing(
        &mut self,
        id: &str,
        text: &str,
        mutation: &str,
        now: i64,
    ) -> Result<Value, String> {
        if !valid_id(mutation) {
            return Err(invalid());
        }
        let scene = Scene::parse(text).map_err(|_| invalid())?;
        let (item, app) = self.drawing_scope(id)?;
        let sha = hash(text.as_bytes());
        let fp = fingerprint(&(&self.account.identity, &self.account.device, id, &sha))?;
        let key = self.drawing_key(id)?;
        let mut receipts = self
            .saved
            .documents
            .drawings
            .copies
            .get(&key)
            .cloned()
            .unwrap_or_default();
        let mut ids = BTreeSet::new();
        if receipts.len() > MAX_RECEIPTS
            || receipts.iter().any(|r| {
                !ids.insert(&r.id)
                    || !valid_id(&r.id)
                    || !valid_hash(&r.fingerprint)
                    || !valid_hash(&r.sha256)
                    || !valid_id(&r.item_id)
            })
        {
            return Err(storage());
        }
        if let Some(receipt) = receipts.iter().find(|r| r.id == mutation) {
            if receipt.fingerprint != fp {
                return Err(invalid());
            }
            let (accepted, _) = self.drawing_scope(&receipt.item_id)?;
            let file = accepted
                .versions
                .first()
                .content
                .owned_file()
                .ok_or_else(storage)?;
            if accepted.space != item.space || file.sha256 != sha || file.sha256 != receipt.sha256 {
                return Err(storage());
            }
            return Ok(dto(
                &receipt.item_id,
                1,
                u32::try_from(accepted.versions.len())
                    .map_err(|_| "document version limit exceeded")?,
                &sha,
                text,
            ));
        }
        let copied = format!(
            "it_copy_{}",
            fingerprint(&(&self.account.identity, &self.account.device, id, mutation))?
        );
        if self.document_items()?.contains_key(&copied) {
            return Err(stale());
        }
        self.document_writer(&item.space)
            .map_err(|_| unavailable())?;
        let title = format!("{} ({})", app.title, t("cópia", "copy"));
        self.create_drawing(&item.space, &copied, app, &title, text, &scene, now)?;
        receipts.push(CopyReceipt {
            id: mutation.into(),
            fingerprint: fp,
            item_id: copied.clone(),
            sha256: sha.clone(),
        });
        if receipts.len() > MAX_RECEIPTS {
            receipts.remove(0);
        }
        self.saved.documents.drawings.copies.insert(key, receipts);
        Ok(dto(&copied, 1, 1, &sha, text))
    }
}

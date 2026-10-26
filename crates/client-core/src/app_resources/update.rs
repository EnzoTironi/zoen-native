use super::*;
impl Engine {
    pub(crate) fn update_app_resource(&mut self, input: Update) -> R<crate::AppDocumentDto> {
        let args = &input.admission;
        self.native_surface_owner(&args.expected_identity_id, &args.expected_device_id)?;
        let (me, device, space, version, mut app) = self.app_document_scope(&input.item_id)?;
        let uri = package_uri(args)?;
        if me != args.expected_identity_id || space != args.space_id || !token(&args.mutation_id) {
            return Err(denied());
        }
        let fingerprint =
            hash(&serde_json::to_vec(&("ui-release-update-v1", &input)).map_err(|_| invalid())?);
        let (key, mut receipts) = self.document_receipts(&me, &device, &input.item_id)?;
        if let Some(receipt) = receipts.iter().find(|r| r.id == args.mutation_id) {
            let accepted = self
                .item_state(&input.item_id)?
                .versions
                .iter()
                .find(|v| v.number == receipt.version);
            if receipt.fingerprint != fingerprint || !accepted.is_some_and(|v| matches!(&v.content, ItemContent::App(a)
                if a.resource_uri == uri && a.document.as_ref().is_some_and(|f| f.sha256 == receipt.sha256))) { return Err(invalid()); }
            let file = accepted
                .and_then(|v| v.content.owned_file())
                .ok_or_else(invalid)?;
            let bytes = self
                .owned_file_bytes(file, miniapps::excalidraw::MAX_SCENE_BYTES as u64)?
                .ok_or_else(invalid)?;
            return Ok(crate::AppDocumentDto {
                item_id: input.item_id.clone(),
                version: receipt.version,
                current_version: version,
                sha256: file.sha256.clone(),
                scene_json: String::from_utf8(bytes).map_err(|_| invalid())?,
            });
        }
        self.check_app_resource_pin(&input.item_id, &input.expected_resource_uri)?;
        if version != input.expected_version
            || app
                .document
                .as_ref()
                .is_none_or(|f| f.sha256 != input.expected_sha256)
        {
            return Err(CoreError::Stale {
                reason: "The document changed; review its current version.".into(),
            });
        }
        if !self.state.writer(&space, &me)
            || self.item_state(&input.item_id)?.versions.initial().author != me
            || self.app_trust(&input.item_id) < TrustLevel::Act
            || app.view.is_none()
            || args.title != app.title
        {
            return Err(denied());
        }
        let source = self.item_state(&args.package_item_id)?;
        if source.space != space {
            return Err(denied());
        }
        let file = source
            .versions
            .iter()
            .find(|v| v.number == args.package_version)
            .and_then(|v| match &v.content {
                ItemContent::File(f) => Some(f),
                _ => None,
            })
            .ok_or_else(invalid)?;
        if file.sha256 != args.package_sha256
            || file.mime != MIME
            || file.bytes == 0
            || file.voice.is_some()
            || crate::files::validate_file(file, MAX_BYTES).is_err()
        {
            return Err(invalid());
        }
        let bytes = self
            .owned_file_bytes(file, MAX_BYTES)?
            .ok_or_else(invalid)?;
        std::str::from_utf8(&bytes).map_err(|_| invalid())?;
        let next_version = version.checked_add(1).ok_or_else(invalid)?;
        self.atomic_agent_action(|e| {
            let owned_key = e.file_key(Some(&input.item_id))?;
            app.view = Some(Box::new(e.file_doc(
                &space,
                &owned_key,
                "app.html",
                "",
                MIME,
                &bytes,
                None,
                &std::collections::HashMap::new(),
            )?));
            app.resource_uri = uri.clone();
            e.append(
                &space,
                &me,
                EventBody::ItemVersioned {
                    item: input.item_id.clone(),
                    content: ItemContent::App(app),
                    note: crate::i18n::t("Atualizou o App", "Updated the App"),
                },
            )?;
            e.check_app_resource_pin(&input.item_id, &uri)?;
            let result = e.app_document_read(&input.item_id)?;
            if result.version != next_version {
                return Err(invalid());
            }
            receipts.push(crate::app_documents::Receipt {
                id: args.mutation_id.clone(),
                fingerprint,
                version: result.version,
                sha256: result.sha256.clone(),
                source: None,
            });
            if receipts.len() > crate::app_documents::MAX_RECEIPTS {
                receipts.remove(0);
            }
            e.store.set_meta(
                &key,
                &serde_json::to_string(&receipts).map_err(|_| invalid())?,
            )?;
            Ok(result)
        })
    }
}

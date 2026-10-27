use super::*;
impl Engine {
    pub(crate) fn find_app_resource(&self, input: &Install) -> R<Option<ItemDetail>> {
        self.native_surface_owner(&input.expected_identity_id, &input.expected_device_id)?;
        if !self.is_member(&input.space_id, &input.expected_identity_id) {
            return Err(denied());
        }
        let uri = package_uri(input)?;
        // The card may have advanced to a newer release while an installation
        // ACK was lost. Its signed versions retain the admitted package pin;
        // recovering that intent must never install another artifact.
        let mut matches = self.state.items.values().filter(|item| {
            item.space == input.space_id
                && item.versions.initial().author == input.expected_identity_id
                && matches!(&item.current().content, ItemContent::App(app) if app.view.is_some() && valid(app))
                && item.versions.iter().any(|version| matches!(&version.content,
                    ItemContent::App(app) if app.resource_uri == uri && app.view.is_some()))
        });
        let first = matches.next();
        if matches.next().is_some() {
            return Err(CoreError::Stale {
                reason: crate::i18n::t(
                    "Escolha o App que deseja abrir.",
                    "Choose the App you want to open.",
                ),
            });
        }
        first.map(|item| self.item(&item.id)).transpose()
    }
    pub(crate) fn install_app_resource(&mut self, input: Install) -> R<ItemDetail> {
        self.native_surface_owner(&input.expected_identity_id, &input.expected_device_id)?;
        let me = self.me_id()?;
        if me != input.expected_identity_id
            || !self.state.writer(&input.space_id, &me)
            || !self
                .identities
                .get(&me)
                .is_some_and(|i| i.kind == types::IdentityKind::Person)
            || !token(&input.mutation_id)
            || input.title.trim().is_empty()
            || input.title.chars().count() > 160
        {
            return Err(denied());
        }
        let uri = package_uri(&input)?;
        let item = item_id(
            &me,
            &input.expected_device_id,
            &input.space_id,
            &input.mutation_id,
        )?;
        if let Some(existing) = self.state.items.get(&item) {
            if existing.space != input.space_id
                || !matches!(&existing.versions.initial().content,
                ItemContent::App(app) if app.resource_uri == uri && app.title == input.title && app.view.as_ref().is_some_and(|view| view.sha256 == input.package_sha256))
            {
                return Err(invalid());
            }
            return self.item(&item);
        }
        let package = self.item_state(&input.package_item_id)?;
        if package.space != input.space_id {
            return Err(denied());
        }
        let version = package
            .versions
            .iter()
            .find(|v| v.number == input.package_version)
            .ok_or_else(invalid)?;
        let ItemContent::File(source) = &version.content else {
            return Err(invalid());
        };
        if source.mime != MIME
            || source.sha256 != input.package_sha256
            || source.voice.is_some()
            || source.bytes == 0
            || crate::files::validate_file(source, MAX_BYTES).is_err()
        {
            return Err(invalid());
        }
        let bytes = self
            .owned_file_bytes(source, MAX_BYTES)?
            .ok_or_else(invalid)?;
        std::str::from_utf8(&bytes).map_err(|_| invalid())?;
        self.atomic_agent_action(|e| {
            let key = e.file_key(None)?;
            let view = e.file_doc(
                &input.space_id,
                &key,
                "app.html",
                "",
                MIME,
                &bytes,
                None,
                &std::collections::HashMap::new(),
            )?;
            let mut document = e.file_doc(
                &input.space_id,
                &key,
                "drawing.excalidraw",
                "",
                miniapps::excalidraw::MIME,
                miniapps::excalidraw::EMPTY.as_bytes(),
                None,
                &std::collections::HashMap::new(),
            )?;
            document.app_origin = Some(AppItemOrigin {
                author: me.clone(),
                device: input.expected_device_id.clone(),
                space: input.space_id.clone(),
                mutation: input.mutation_id.clone(),
            });
            e.store
                .set_meta(&format!("file_key:{item}"), &hex::encode(key))?;
            let state = miniapps::excalidraw::Scene::parse(miniapps::excalidraw::EMPTY)
                .map_err(|_| invalid())?
                .metadata();
            e.append(
                &input.space_id,
                &me,
                EventBody::ItemCreated {
                    item: item.clone(),
                    kind: ItemKind::App,
                    content: ItemContent::App(AppDoc {
                        app: "excalidraw".into(),
                        resource_uri: uri,
                        title: input.title,
                        state_json: state,
                        document: Some(document),
                        view: Some(Box::new(view)),
                    }),
                    origin: crate::i18n::t("App adicionado", "App added"),
                },
            )?;
            e.append(
                &input.space_id,
                &me,
                EventBody::GrantIssued {
                    grant: Grant {
                        id: types::new_id("gr")?,
                        grantor: me.clone(),
                        grantee: Some(format!("app:{item}")),
                        scope: GrantScope::Item(item.clone()),
                        capability: Capability::Trust(TrustLevel::Act),
                        expires_at_ms: None,
                    },
                },
            )?;
            e.append(
                &input.space_id,
                &me,
                EventBody::MessagePosted {
                    message: types::new_id("msg")?,
                    text: String::new(),
                    attaches: Some(item.clone()),
                    reply: None,
                    media: None,
                    forwarded: false,
                },
            )?;
            e.item(&item)
        })
    }
}

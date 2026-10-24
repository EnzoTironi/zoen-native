//! Admission on the original Engine and canonical document state.

use super::{
    invalid, json, metadata, t, unavailable, validate_scene, AppDoc, AppDocumentDto, CoreError,
    Engine, HashMap, ItemContent, Value, EMPTY, MAX_SCENE_BYTES, MIME, R, RESOURCE,
};

impl Engine {
    pub(crate) fn attach_app_document(
        &mut self,
        space: &str,
        item: &str,
        mut app: AppDoc,
    ) -> R<AppDoc> {
        if let Some(format) = miniapps::binary::Format::for_id(&app.app) {
            let seed = format.seed();
            if !format.available()
                || app.resource_uri != format.resource()
                || app.document.is_some()
            {
                return Err(invalid());
            }
            let metadata = format.metadata(seed, &app.title).map_err(|_| invalid())?;
            let key = self.file_key(None)?;
            let file = self.file_doc(
                space,
                &key,
                format.filename(),
                "",
                format.mime(),
                seed,
                None,
                &HashMap::new(),
            )?;
            self.store
                .set_meta(&format!("file_key:{item}"), &hex::encode(key))?;
            app.document = Some(file);
            app.state_json = metadata;
            return Ok(app);
        }
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

    pub(super) fn app_document_scope(
        &self,
        item: &str,
    ) -> R<(String, String, String, u32, AppDoc)> {
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
}

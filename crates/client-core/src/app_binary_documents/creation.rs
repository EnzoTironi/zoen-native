//! Existing creation IDs and durable receipt lookup, shared by native and agent paths.
use super::*;

pub(crate) fn creation_item_id_for(
    format: Format,
    me: &str,
    device: &str,
    space: &str,
    mutation: &str,
) -> R<String> {
    Ok(format!(
        "{}{}",
        format.item_prefix(),
        hash(&serde_json::to_vec(&(me, device, space, mutation)).map_err(|_| invalid())?)
    ))
}
impl Engine {
    /// Read only, using the existing durable receipt. Absence never authorizes a
    /// retry: creation still has a deterministic item ID and refuses collisions.
    pub(crate) fn app_binary_creation_result_for(
        &self,
        format: Format,
        space: &str,
        title: &str,
        seed_sha: &str,
        mutation: &str,
    ) -> R<Option<AppBinaryDocumentDto>> {
        let (me, device) = self.app_document_identity(space)?;
        let owner =
            serde_json::to_vec(&(&me, &device, space, format.id())).map_err(|_| invalid())?;
        let key = format!("app_document_create_receipts:{}", hash(&owner));
        let fingerprint = hash(
            &serde_json::to_vec(&(&me, &device, space, title, seed_sha, format.revision()))
                .map_err(|_| invalid())?,
        );
        let receipts = self.binary_receipts(&key)?;
        let Some(receipt) = receipts.iter().find(|receipt| receipt.id == mutation) else {
            return Ok(None);
        };
        if receipt.fingerprint != fingerprint
            || receipt.version != 1
            || receipt.sha256 != seed_sha
            || receipt.item != creation_item_id_for(format, &me, &device, space, mutation)?
        {
            return Err(storage());
        }
        let accepted =
            self.app_binary_read(&receipt.item, Some(receipt.version), Some(&receipt.sha256))?;
        if self.item_state(&receipt.item)?.space != space || accepted.author_id != me {
            return Err(storage());
        }
        Ok(Some(accepted))
    }
}

impl Engine {
    pub(crate) fn app_binary_create(
        &mut self,
        space: &str,
        title: &str,
        data: &str,
        mutation: &str,
    ) -> R<AppBinaryDocumentDto> {
        self.app_binary_create_format(Format::Deckcraft, space, title, data, mutation)
    }
    pub(crate) fn app_binary_create_format(
        &mut self,
        format: Format,
        space: &str,
        title: &str,
        data: &str,
        mutation: &str,
    ) -> R<AppBinaryDocumentDto> {
        if !token(space)
            || !token(mutation)
            || title.trim().is_empty()
            || title.chars().count() > 160
            || title.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        let (me, device) = self.app_document_identity(space)?;
        if !self.state.writer(space, &me) {
            return Err(unavailable());
        }
        let bytes = decode(data, format)?;
        let metadata = format.metadata(&bytes, title).map_err(|_| invalid())?;
        let sha = hash(&bytes);
        let owner =
            serde_json::to_vec(&(&me, &device, space, format.id())).map_err(|_| invalid())?;
        let key = format!("app_document_create_receipts:{}", hash(&owner));
        let fingerprint = hash(
            &serde_json::to_vec(&(&me, &device, space, title, &sha, format.revision()))
                .map_err(|_| invalid())?,
        );
        let receipts = self.binary_receipts(&key)?;
        if let Some(r) = receipts.iter().find(|r| r.id == mutation) {
            if r.fingerprint != fingerprint {
                return Err(invalid());
            }
            let accepted = self.app_binary_read(&r.item, Some(r.version), Some(&r.sha256))?;
            if self.item_state(&r.item)?.space != space || accepted.sha256 != sha {
                return Err(storage());
            }
            return Ok(accepted);
        }
        let item = creation_item_id_for(format, &me, &device, space, mutation)?;
        if self.state.items.contains_key(&item) {
            return Err(stale());
        }
        self.atomic_agent_action(|e| {
            let file_key = e.file_key(None)?;
            let file = e.file_doc(
                space,
                &file_key,
                format.filename(),
                "",
                format.mime(),
                &bytes,
                None,
                &HashMap::new(),
            )?;
            e.store
                .set_meta(&format!("file_key:{item}"), &hex::encode(file_key))?;
            e.append(
                space,
                &me,
                EventBody::ItemCreated {
                    item: item.clone(),
                    kind: ItemKind::App,
                    content: ItemContent::App(AppDoc {
                        app: format.id().into(),
                        resource_uri: format.resource().into(),
                        title: title.into(),
                        state_json: metadata,
                        document: Some(file),
                    }),
                    origin: match format {
                        Format::Photocraft => crate::i18n::t("Imagem criada", "Created image"),
                        Format::Wordcraft => crate::i18n::t("Documento criado", "Created document"),
                        Format::Deckcraft => {
                            crate::i18n::t("Apresentação criada", "Created presentation")
                        }
                        Format::Gridcraft => {
                            crate::i18n::t("Planilha criada", "Created spreadsheet")
                        }
                    },
                },
            )?;
            e.append(
                space,
                &me,
                EventBody::GrantIssued {
                    grant: Grant {
                        id: new_id("gr"),
                        grantor: me.clone(),
                        grantee: Some(format!("app:{item}")),
                        scope: GrantScope::Item(item.clone()),
                        capability: Capability::Trust(TrustLevel::Act),
                        expires_at_ms: None,
                    },
                },
            )?;
            e.post_as(
                space,
                &me,
                &match format {
                    Format::Photocraft => crate::i18n::t(
                        "Imagem adicionada a esta conversa.",
                        "Image added to this conversation.",
                    ),
                    Format::Wordcraft => crate::i18n::t(
                        "Documento adicionado a esta conversa.",
                        "Document added to this conversation.",
                    ),
                    Format::Deckcraft => crate::i18n::t(
                        "Apresentação adicionada a esta conversa.",
                        "Presentation added to this conversation.",
                    ),
                    Format::Gridcraft => crate::i18n::t(
                        "Planilha adicionada a esta conversa.",
                        "Spreadsheet added to this conversation.",
                    ),
                },
                Some(item.clone()),
            )?;
            e.binary_receipt(
                &key,
                receipts,
                Receipt {
                    id: mutation.into(),
                    intent: fingerprint.clone(),
                    fingerprint,
                    item: item.clone(),
                    version: 1,
                    sha256: sha,
                },
            )?;
            e.app_binary_read(&item, Some(1), None)
        })
    }
}

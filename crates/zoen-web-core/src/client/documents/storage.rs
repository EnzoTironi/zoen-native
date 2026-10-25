//! Document storage on the existing signed client and encrypted journal.
use super::*;

impl<S: Signer> Client<S> {
    pub(in crate::client) fn document_items(&self) -> Result<BTreeMap<String, Item>, String> {
        let mut items: BTreeMap<String, Item> = BTreeMap::new();
        let approved = self.approved_request_items()?;
        for (space, log) in &self.logs {
            let mut roster = BTreeMap::new();
            for event in log.events() {
                match &event.body {
                    EventBody::SpaceCreated { .. } => {
                        roster.insert(event.author.clone(), Role::Owner);
                    }
                    EventBody::MemberAdded { identity, role } => {
                        roster.insert(identity.clone(), *role);
                    }
                    EventBody::MemberRemoved { identity } => {
                        roster.remove(identity);
                    }
                    _ => {}
                }
                if roster
                    .get(&event.author)
                    .is_some_and(|role| *role != Role::Reader)
                {
                    fold_item(&mut items, space, event, false, None)?;
                }
                if let Some(virtual_event) = approved.get(&(space.clone(), event.client_id.clone()))
                {
                    fold_item(&mut items, space, virtual_event, false, None)?;
                }
            }
        }
        let mut pending = self.saved.outbox.values().collect::<Vec<_>>();
        pending.sort_by_key(|pending| (pending.order.unwrap_or(0), &pending.content));
        for pending in pending {
            let event = pending
                .inner
                .clone()
                .or_else(|| pending.envelope().ok()?.plain_event());
            let Some(event) = event else {
                continue;
            };
            if self
                .roster(&event.space)
                .get(&event.author)
                .is_some_and(|role| *role != Role::Reader)
            {
                fold_item(
                    &mut items,
                    &event.space,
                    &event,
                    true,
                    pending.failure.clone(),
                )?;
            }
            if let Some(virtual_event) =
                approved.get(&(event.space.clone(), event.client_id.clone()))
            {
                fold_item(
                    &mut items,
                    &event.space,
                    virtual_event,
                    true,
                    pending.failure.clone(),
                )?;
            }
        }
        Ok(items)
    }

    pub(in crate::client) fn document_item(&self, id: &str) -> Result<Item, String> {
        self.document_items()?
            .remove(id)
            .ok_or_else(|| "Arquivo indisponível neste aparelho.".into())
    }

    pub(in crate::client) fn document_writer(&self, space: &str) -> Result<(), String> {
        if let Some(reason) = self.saved.integrity_errors.get(space) {
            return Err(reason.clone());
        }
        if !self
            .roster(space)
            .get(&self.account.identity)
            .is_some_and(|role| *role != Role::Reader)
        {
            return Err("Só membros com permissão de escrita podem editar este arquivo.".into());
        }
        if self
            .metadata(space)
            .is_some_and(|(_, _, privacy)| privacy == Privacy::EndToEnd)
        {
            self.queue_epoch(space)?;
        }
        Ok(())
    }

    pub(in crate::client) fn queue_domain_body(
        &mut self,
        space: &str,
        body: EventBody,
        now: i64,
        uploads: Vec<String>,
    ) -> Result<(), String> {
        self.document_writer(space)?;
        let mut pending = self.clear_event(space, body, now)?;
        let event = pending
            .envelope()?
            .plain_event()
            .ok_or("invalid document event")?;
        if self
            .metadata(space)
            .is_some_and(|(_, _, privacy)| privacy == Privacy::EndToEnd)
        {
            pending = self.seal_inner(&event, self.queue_epoch(space)?)?;
        } else {
            pending.inner = Some(event.clone());
        }
        pending.blob_uploads = uploads;
        self.saved.outbox_sequence = self
            .saved
            .outbox_sequence
            .checked_add(1)
            .ok_or("O diário de envio está cheio.")?;
        pending.order = Some(self.saved.outbox_sequence);
        self.saved.outbox.insert(event.client_id.clone(), pending);
        Ok(())
    }

    /// One local domain transaction. Every event is prepared before any intent enters
    /// the durable outbox, and its order is independent of random same-millisecond IDs.
    pub(in crate::client) fn queue_domain_batch(
        &mut self,
        space: &str,
        bodies: Vec<EventBody>,
        now: i64,
    ) -> Result<(), String> {
        self.queue_domain_batch_uploads(space, bodies, now, &[])
            .map(|_| ())
    }

    /// The same atomic domain batch, held behind the existing durable blob barrier.
    pub(in crate::client) fn queue_domain_batch_uploads(
        &mut self,
        space: &str,
        bodies: Vec<EventBody>,
        now: i64,
        uploads: &[String],
    ) -> Result<Vec<String>, String> {
        self.document_writer(space)?;
        let end = self
            .saved
            .outbox_sequence
            .checked_add(bodies.len() as u64)
            .ok_or("O diário de envio está cheio.")?;
        let mut prepared = Vec::with_capacity(bodies.len());
        for (index, body) in bodies.into_iter().enumerate() {
            let mut pending = self.clear_event(space, body, now)?;
            let event = pending
                .envelope()?
                .plain_event()
                .ok_or("invalid domain event")?;
            if self
                .metadata(space)
                .is_some_and(|(_, _, privacy)| privacy == Privacy::EndToEnd)
            {
                pending = self.seal_inner(&event, self.queue_epoch(space)?)?;
            } else {
                pending.inner = Some(event.clone());
            }
            pending.blob_uploads = uploads.to_vec();
            pending.order = Some(self.saved.outbox_sequence + index as u64 + 1);
            prepared.push((event.client_id.clone(), pending));
        }
        self.saved.outbox_sequence = end;
        let identities = prepared.iter().map(|(id, _)| id.clone()).collect();
        self.saved.outbox.extend(prepared);
        Ok(identities)
    }

    pub(super) fn read_page(&self, item: &Item, include_draft: bool) -> Result<PageRead, String> {
        if item.kind != ItemKind::Page {
            return Err("Este arquivo não é uma página.".into());
        }
        let shadow = Page::new();
        let mut downloads = Vec::new();
        let mut failure = None;
        for version in &item.versions {
            let ItemContent::Page(content) = &version.content else {
                failure = Some(
                    "Uma versão tem conteúdo incompatível. O histórico foi preservado.".into(),
                );
                break;
            };
            if content.manifest.is_some() {
                failure = Some(
                    "Esta página usa um formato incompatível. O histórico foi preservado.".into(),
                );
                break;
            }
            let payload = if let Some(reference) = &content.blob {
                match self.saved.documents.blobs.get(&reference.sha256) {
                    Some(bytes)
                        if bytes.len() as u64 == reference.bytes
                            && hash(bytes) == reference.sha256 =>
                    {
                        Some(bytes.clone())
                    }
                    _ => {
                        downloads.push(reference.clone());
                        None
                    }
                }
            } else {
                match BASE64.decode(&content.loro) {
                    Ok(bytes) => Some(bytes),
                    Err(reason) => {
                        failure = Some(reason.to_string());
                        break;
                    }
                }
            };
            let Some(payload) = payload else {
                continue;
            };
            if payload.len() > MAX_PAGE_BYTES {
                failure = Some("A página excede o limite de tamanho.".into());
                break;
            }
            if !downloads.is_empty() {
                continue;
            }
            if let Err(reason) = shadow.import(&payload) {
                failure = Some(reason.to_string());
                break;
            }
        }
        let live = shadow.at(&shadow.frontiers()).map_err(error)?;
        if include_draft {
            if let Some(draft) = self.saved.documents.drafts.get(&item.id) {
                if draft.origin != item.origin_hash() {
                    return Err(
                        "O rascunho pertence a outra página; nenhum conteúdo foi substituído."
                            .into(),
                    );
                }
                live.import(&draft.snapshot).map_err(error)?;
            }
        }
        Ok(PageRead {
            shadow,
            live,
            ready: downloads.is_empty() && failure.is_none(),
            error: failure,
            downloads,
        })
    }

    pub(super) fn page_context(&self, item: &Item, frontiers: Vec<u8>) -> Result<String, String> {
        let bytes = serde_json::to_vec(&Context {
            item: item.id.clone(),
            space: item.space.clone(),
            origin: item.origin_hash(),
            frontiers,
        })
        .map_err(error)?;
        Ok(format!("web-page-v1:{}", BASE64.encode(bytes)))
    }

    pub(super) fn page_value(
        &self,
        item: &Item,
        read: &PageRead,
        historical: bool,
    ) -> Result<Value, String> {
        let can_edit = !historical && read.ready && self.document_writer(&item.space).is_ok();
        let content = match &item.current().content {
            ItemContent::Page(page) => page,
            _ => return Err("Esta versão não é uma página.".into()),
        };
        Ok(json!({
            "item_id": item.id, "space_id": item.space, "title": content.title, "path": content.path,
            "version": item.versions.len(), "ready": read.ready,
            "unsaved": !historical && read.live.content_hash().map_err(|error| error.to_string())? != read.shadow.content_hash().map_err(|error| error.to_string())?,
            "pending_sync": !historical && item.versions.iter().any(|version| version.pending),
            "save_error": read.error.clone().or_else(|| item.versions.iter().find_map(|version| version.failure.clone())),
            "can_edit": can_edit, "content_hash": read.live.content_hash().map_err(|error| error.to_string())?,
            "edit_context": if can_edit { self.page_context(item, read.live.frontiers().encode())? } else { String::new() },
            "blocks": read.live.blocks().iter().map(Block::from).collect::<Vec<_>>()
        }))
    }

    pub(super) fn page_content(
        &mut self,
        title: String,
        path: String,
        bytes: Vec<u8>,
    ) -> Result<(PageContent, Vec<String>), String> {
        let (loro, blob, uploads) = if bytes.len() > INLINE_MAX {
            let (reference, upload) = crate::media::seal_page(&bytes)?;
            self.saved
                .documents
                .blobs
                .insert(reference.sha256.clone(), bytes);
            self.saved
                .local_chunks
                .insert(upload.blob.clone(), upload.sealed);
            (String::new(), Some(reference), vec![upload.blob])
        } else {
            (BASE64.encode(bytes), None, Vec::new())
        };
        Ok((
            PageContent {
                title,
                path,
                loro,
                blob,
                manifest: None,
            },
            uploads,
        ))
    }

    pub(in crate::client) fn pending_document_uploads(&self) -> Vec<String> {
        self.saved
            .outbox
            .values()
            .filter(|pending| pending.failure.is_none())
            .flat_map(|pending| &pending.blob_uploads)
            .cloned()
            .chain(self.pending_profile_uploads())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Upload barriers survive reload. A document event cannot publish before its blob.
    pub fn document_uploads(&self) -> Result<String, String> {
        serde_json::to_string(&self.pending_document_uploads()).map_err(error)
    }

    pub fn document_uploaded(&mut self, blobs: &[String]) -> Result<Update, String> {
        for blob in blobs {
            if !self.saved.local_chunks.contains_key(blob) {
                return Err("Payload de página indisponível.".into());
            }
        }
        for pending in self.saved.outbox.values_mut() {
            pending.blob_uploads.retain(|blob| !blobs.contains(blob));
        }
        self.mark_profile_uploaded(blobs);
        self.update(Vec::new())
    }

    pub fn document_cache_blob(
        &mut self,
        reference: ChunkRef,
        bytes: Vec<u8>,
    ) -> Result<Update, String> {
        if bytes.len() > MAX_PAGE_BYTES
            || bytes.len() as u64 != reference.bytes
            || hash(&bytes) != reference.sha256
        {
            return Err("A página recebida não corresponde ao conteúdo assinado.".into());
        }
        self.saved.documents.blobs.insert(reference.sha256, bytes);
        self.update(Vec::new())
    }
}

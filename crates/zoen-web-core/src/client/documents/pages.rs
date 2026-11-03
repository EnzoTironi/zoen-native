//! Document pages on the existing signed client and encrypted journal.
use super::*;

fn page_edit_context(item: &Item, context: &str) -> Result<Context, String> {
    let raw = context
        .strip_prefix("web-page-v1:")
        .ok_or("Contexto de edição inválido.")?;
    let decoded: Context =
        serde_json::from_slice(&BASE64.decode(raw).map_err(error)?).map_err(error)?;
    if decoded.item != item.id
        || decoded.space != item.space
        || decoded.origin != item.origin_hash()
    {
        return Err("A edição pertence a outra página.".into());
    }
    Ok(decoded)
}

impl<S: Signer> Client<S> {
    pub(super) fn create_page(
        &mut self,
        space: &str,
        path: &str,
        page: &Page,
        origin: &str,
        now: i64,
    ) -> Result<String, String> {
        let mut title = page.title();
        if title.is_empty() {
            path.rsplit('/')
                .next()
                .unwrap_or("")
                .trim_end_matches(".md")
                .clone_into(&mut title);
        }
        if title.is_empty() {
            title = t("Sem título", "Untitled");
        }
        let (content, uploads) = self.page_content(title, path.into(), page.snapshot())?;
        let item = browser_id("it", now)?;
        self.queue_domain_body(
            space,
            &EventBody::ItemCreated {
                item: item.clone(),
                kind: ItemKind::Page,
                content: ItemContent::Page(content),
                origin: origin.into(),
            },
            now,
            uploads,
        )?;
        Ok(item)
    }

    pub(super) fn apply_page(
        &mut self,
        item: &Item,
        mutation: &str,
        context: &str,
        order: &[String],
        changed: &[Block],
    ) -> Result<Value, String> {
        self.document_writer(&item.space)?;
        if mutation.trim().is_empty() || mutation.len() > 256 || context.len() > 2 << 20 {
            return Err("Identificação de edição inválida.".into());
        }
        let decoded = page_edit_context(item, context)?;
        let request_hash =
            hash(serde_json::to_vec(&(mutation, context, order, changed)).map_err(error)?);
        let read = self.read_page(item, true)?;
        if !read.ready {
            return Err("A página ainda está chegando; o rascunho foi preservado.".into());
        }
        if let Some(receipt) = self
            .saved
            .documents
            .drafts
            .get(&item.id)
            .and_then(|draft| draft.receipts.get(mutation))
        {
            if receipt.fingerprint != request_hash {
                return Err("Este identificador já foi usado por outra edição.".into());
            }
            return Ok(
                json!({ "mutation_hash": request_hash, "mutation_id": mutation, "applied_content_hash": receipt.content_hash,
                "replayed": true, "applied_edit_context": Self::page_context(item, receipt.frontiers.clone())?, "page": self.page_value(item, &read, false)? }),
            );
        }
        let revision = Frontiers::decode(&decoded.frontiers).map_err(error)?;
        let desired = read.live.at(&revision).map_err(error)?;
        let kinds = desired
            .blocks()
            .into_iter()
            .map(|block| (block.id, block.kind))
            .collect::<BTreeMap<_, _>>();
        let edits = changed
            .iter()
            .map(|block| block.edit(kinds.get(&block.id)))
            .collect::<Result<Vec<_>, _>>()?;
        desired.apply(order, &edits).map_err(error)?;
        let context_hash = hash(context);
        let prior = self.saved.documents.drafts.get(&item.id).and_then(|draft| {
            draft
                .receipts
                .values()
                .filter(|receipt| receipt.context_hash == context_hash)
                .max_by_key(|receipt| receipt.sequence)
        });
        let (observed, base) = if let Some(prior) = prior {
            let previous = Frontiers::decode(&prior.frontiers).map_err(error)?;
            let observed = read.live.at(&previous).map_err(error)?;
            let base = observed.version();
            let complete = desired
                .blocks()
                .into_iter()
                .map(|block| BlockEdit {
                    text: block.plain_text(),
                    spans: ::documents::spans_of(&block.runs),
                    id: block.id,
                    kind: block.kind,
                })
                .collect::<Vec<_>>();
            observed.apply(order, &complete).map_err(error)?;
            (observed, base)
        } else {
            let base = read.live.at(&revision).map_err(error)?.version();
            (desired, base)
        };
        read.live
            .import(&observed.updates_since(&base))
            .map_err(error)?;
        let receipt = Receipt {
            fingerprint: request_hash.clone(),
            context_hash,
            content_hash: observed.content_hash().map_err(|error| error.to_string())?,
            frontiers: observed.frontiers().encode(),
            sequence: self
                .saved
                .documents
                .drafts
                .get(&item.id)
                .map_or(0, |draft| draft.receipts.len()),
        };
        let applied_context = Self::page_context(item, receipt.frontiers.clone())?;
        let applied_hash = receipt.content_hash.clone();
        let draft = self
            .saved
            .documents
            .drafts
            .entry(item.id.clone())
            .or_default();
        if draft.receipts.len() >= 8192 {
            return Err("O diário de edição está cheio; o conteúdo foi preservado.".into());
        }
        draft.origin = item.origin_hash();
        draft.snapshot = read.live.snapshot();
        draft.receipts.insert(mutation.into(), receipt);
        Ok(
            json!({ "mutation_hash": request_hash, "mutation_id": mutation, "applied_content_hash": applied_hash,
            "replayed": false, "applied_edit_context": applied_context, "page": self.page_value(item, &read, false)? }),
        )
    }

    pub(super) fn commit_page(
        &mut self,
        item: &Item,
        note: &str,
        now: i64,
    ) -> Result<bool, String> {
        self.document_writer(&item.space)?;
        let read = self.read_page(item, true)?;
        if !read.ready {
            return Err("A página ainda está chegando.".into());
        }
        if read
            .live
            .content_hash()
            .map_err(|error| error.to_string())?
            == read
                .shadow
                .content_hash()
                .map_err(|error| error.to_string())?
        {
            return Ok(false);
        }
        let path = match &item.versions.first().content {
            ItemContent::Page(content) => content.path.clone(),
            _ => String::new(),
        };
        let title = if read.live.title().is_empty() {
            item.current().content.title()
        } else {
            read.live.title()
        };
        let (content, uploads) =
            self.page_content(title, path, read.live.updates_since(&read.shadow.version()))?;
        self.queue_domain_body(
            &item.space,
            &EventBody::ItemVersioned {
                item: item.id.clone(),
                content: ItemContent::Page(content),
                note: if note.trim().is_empty() {
                    t("Editou", "Edited")
                } else {
                    note.into()
                },
            },
            now,
            uploads,
        )?;
        Ok(true)
    }
}

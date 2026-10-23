//! Document dispatch on the existing signed client and encrypted journal.
use super::*;

impl<S: Signer> Client<S> {
    pub fn document_request(
        &mut self,
        request: &str,
        now: i64,
    ) -> Result<DocumentResponse, String> {
        self.view_now_ms = now;
        if request.len() > apps::MAX_REQUEST_BYTES {
            return Err("Edição grande demais.".into());
        }
        #[derive(Deserialize)]
        struct Operation<'a> {
            op: &'a str,
        }
        let operation: Operation<'_> = serde_json::from_str(request).map_err(error)?;
        if matches!(
            operation.op,
            "app_document_read" | "app_document_save" | "app_document_copy"
        ) {
            return self.drawing_request(request, now);
        }
        if request.len() > MAX_REQUEST_BYTES {
            return Err("Edição grande demais.".into());
        }
        let command: Command = serde_json::from_str(request).map_err(error)?;
        let mut downloads = Vec::new();
        let mut mutated = false;
        let result = match command {
            Command::NewMutationId {} => json!(browser_id("client_edit", now)),
            Command::PageParseMarkdown { markdown } => json!(Page::from_markdown(&markdown)
                .map_err(error)?
                .blocks()
                .iter()
                .map(Block::from)
                .collect::<Vec<_>>()),
            Command::PageBlockMarkdown { block } => {
                let page = Page::new();
                page.apply(std::slice::from_ref(&block.id), &[block.edit(None)?])
                    .map_err(error)?;
                json!(page.canonical_markdown())
            }
            Command::Items {} => {
                let mut items = self.document_items()?.into_values().collect::<Vec<_>>();
                items.sort_by_key(|item| std::cmp::Reverse(item.current().event.at_ms));
                Value::Array(
                    items
                        .iter()
                        .map(|item| self.item_value(item, None, &mut downloads))
                        .collect::<Result<_, _>>()?,
                )
            }
            Command::Item { item_id } => {
                self.item_value(&self.document_item(&item_id)?, None, &mut downloads)?
            }
            Command::ItemDiscussion { space_id, item_id } => {
                self.item_discussion_value(&space_id, &item_id)?
            }
            Command::ItemAt { item_id, version } => self.item_value(
                &self.document_item(&item_id)?,
                Some(version),
                &mut downloads,
            )?,
            Command::FileManifest { item_id, version } => {
                let item = self.document_item(&item_id)?;
                if item.kind == ItemKind::App {
                    self.app_document_access(&item.space)?;
                }
                let number = version.unwrap_or(item.versions.len() as u32);
                let current = number
                    .checked_sub(1)
                    .and_then(|index| item.versions.get(index as usize))
                    .ok_or("Versão indisponível.")?;
                if !matches!(
                    (item.kind, &current.content),
                    (ItemKind::File, ItemContent::File(_)) | (ItemKind::App, ItemContent::App(_))
                ) {
                    return Err("Esta versão não contém mídia.".into());
                }
                let document = current
                    .content
                    .owned_file()
                    .ok_or("Esta versão não contém mídia.")?;
                let detail = self.item_value(&item, Some(number), &mut downloads)?;
                json!({"itemId":item.id,"version":number,"doc":document,"info":detail["file"]})
            }
            Command::Page { item_id } => {
                let item = self.document_item(&item_id)?;
                let read = self.read_page(&item, true)?;
                downloads.extend(read.downloads.clone());
                self.page_value(&item, &read, false)?
            }
            Command::PageAt { item_id, version } => {
                let mut item = self.document_item(&item_id)?;
                if version == 0 || version as usize > item.versions.len() {
                    return Err("Versão indisponível.".into());
                }
                item.versions.truncate(version as usize);
                let read = self.read_page(&item, false)?;
                downloads.extend(read.downloads.clone());
                self.page_value(&item, &read, true)?
            }
            Command::PageCreate { space_id, title } => {
                self.document_writer(&space_id)?;
                let page = Page::from_markdown(&if title.trim().is_empty() {
                    String::new()
                } else {
                    format!("# {}\n", title.trim())
                })
                .map_err(error)?;
                let item = self.create_page(&space_id, "", page, &t("Criado", "Created"), now)?;
                mutated = true;
                self.item_value(&self.document_item(&item)?, None, &mut downloads)?
            }
            Command::PageImportMarkdown {
                space_id,
                path,
                markdown,
            } => {
                self.document_writer(&space_id)?;
                if path.contains('\0')
                    || path.starts_with('/')
                    || path.split('/').any(|part| part == "..")
                {
                    return Err("Caminho inválido.".into());
                }
                let page = Page::from_markdown(&markdown).map_err(error)?;
                let item =
                    self.create_page(&space_id, &path, page, &t("Importado", "Imported"), now)?;
                mutated = true;
                self.item_value(&self.document_item(&item)?, None, &mut downloads)?
            }
            Command::PageApply {
                item_id,
                mutation_id,
                edit_context,
                order,
                changed,
            } => {
                let item = self.document_item(&item_id)?;
                let result =
                    self.apply_page(&item, &mutation_id, &edit_context, &order, &changed)?;
                mutated = true;
                result
            }
            Command::PageCommit { item_id, note } => {
                let item = self.document_item(&item_id)?;
                let committed = self.commit_page(&item, &note, now)?;
                mutated = committed;
                json!(committed)
            }
            Command::PageMarkdown { item_id } => {
                let item = self.document_item(&item_id)?;
                let read = self.read_page(&item, true)?;
                downloads.extend(read.downloads.clone());
                if !read.ready {
                    return Ok(DocumentResponse {
                        result: Value::Null,
                        update: None,
                        downloads,
                        uploads: Vec::new(),
                    });
                }
                json!(read.live.to_markdown())
            }
            Command::PlanPage { item_id } => {
                self.plan_page_value(&self.document_item(&item_id)?, &mut downloads)?
            }
            Command::TogglePlanLine {
                item_id,
                line_id,
                observed_hash,
            } => self
                .edit_plan_document(
                    &item_id,
                    &observed_hash,
                    &t("Alterou conclusão", "Changed completion"),
                    now,
                    |plan| {
                        let line = unique_line_mut(plan, &line_id)?;
                        line.done = !line.done;
                        Ok(())
                    },
                )
                .inspect(|_| {
                    mutated = true;
                })?,
            Command::EditPlanLine {
                item_id,
                line_id,
                text,
                cost_cents,
                observed_hash,
            } => {
                let cost = cents(&cost_cents)?;
                let text = valid_line(text, cost)?;
                self.edit_plan_document(
                    &item_id,
                    &observed_hash,
                    &t("Editou linha", "Edited line"),
                    now,
                    |plan| {
                        let line = unique_line_mut(plan, &line_id)?;
                        line.text = text;
                        line.cost_cents = cost;
                        Ok(())
                    },
                )
                .inspect(|_| {
                    mutated = true;
                })?
            }
            Command::AddPlanLine {
                item_id,
                section_index,
                text,
                cost_cents,
                observed_hash,
            } => {
                let cost = cents(&cost_cents)?;
                let text = valid_line(text, cost)?;
                self.edit_plan_document(
                    &item_id,
                    &observed_hash,
                    &t("Adicionou linha", "Added line"),
                    now,
                    |plan| {
                        plan.sections
                            .get_mut(section_index as usize)
                            .ok_or("Seção indisponível.")?
                            .lines
                            .push(types::PlanLine {
                                id: browser_id("ln", now),
                                text,
                                cost_cents: cost,
                                done: false,
                            });
                        Ok(())
                    },
                )
                .inspect(|_| {
                    mutated = true;
                })?
            }
            Command::RemovePlanLine {
                item_id,
                line_id,
                observed_hash,
            } => self
                .edit_plan_document(
                    &item_id,
                    &observed_hash,
                    &t("Removeu linha", "Removed line"),
                    now,
                    |plan| {
                        unique_line_mut(plan, &line_id)?;
                        plan.remove_line(&line_id).ok_or("Linha indisponível.")?;
                        Ok(())
                    },
                )
                .inspect(|_| {
                    mutated = true;
                })?,
            Command::RestorePlanVersion {
                item_id,
                version,
                observed_hash,
            } => {
                self.restore_plan(
                    &item_id,
                    version,
                    &observed_hash,
                    &t("Restaurou versão", "Restored version"),
                    now,
                )?;
                mutated = true;
                self.item_value(&self.document_item(&item_id)?, None, &mut downloads)?
            }
            Command::Undo { token } => {
                let item = self.document_item(&token.item_id)?;
                if item.versions.len() != token.result_version as usize {
                    return Err("O plano mudou. Revise antes de desfazer.".into());
                }
                self.restore_plan(
                    &token.item_id,
                    token.restore_version,
                    &token.observed_hash,
                    &tr!("Desfez: {}", "Undid: {}", token.label),
                    now,
                )?;
                mutated = true;
                self.item_value(&self.document_item(&token.item_id)?, None, &mut downloads)?
            }
            Command::RestoreVersion { item_id, version } => {
                let item = self.document_item(&item_id)?;
                if item.kind == ItemKind::Plan {
                    return Err("A restauração de um plano exige o vínculo observado.".into());
                }
                if version == 0 || version as usize > item.versions.len() {
                    return Err("Versão indisponível.".into());
                }
                if item.kind == ItemKind::Page {
                    self.document_writer(&item.space)?;
                    let read = self.read_page(&item, true)?;
                    if !read.ready {
                        return Err("A página ainda está chegando.".into());
                    }
                    let mut past = item.clone();
                    past.versions.truncate(version as usize);
                    let target = self.read_page(&past, false)?;
                    if !target.ready {
                        return Err("A versão ainda está chegando.".into());
                    }
                    read.live
                        .revert_to(&target.shadow.frontiers())
                        .map_err(error)?;
                    let draft = self
                        .saved
                        .documents
                        .drafts
                        .entry(item.id.clone())
                        .or_default();
                    draft.origin = item.origin_hash();
                    draft.snapshot = read.live.snapshot();
                    self.commit_page(
                        &item,
                        &tr!("Restaurou a v{version}", "Restored v{version}"),
                        now,
                    )?;
                } else {
                    self.queue_domain_body(
                        &item.space,
                        EventBody::ItemReverted {
                            item: item_id.clone(),
                            to_version: version,
                            note: tr!("Restaurou a v{version}", "Restored v{version}"),
                        },
                        now,
                        Vec::new(),
                    )?;
                }
                mutated = true;
                self.item_value(&self.document_item(&item_id)?, None, &mut downloads)?
            }
        };
        downloads.sort_by(|a, b| a.sha256.cmp(&b.sha256));
        downloads.dedup_by(|a, b| a.sha256 == b.sha256);
        let uploads = if mutated {
            self.pending_document_uploads()
        } else {
            Vec::new()
        };
        Ok(DocumentResponse {
            result,
            update: if mutated {
                Some(self.update(Vec::new())?)
            } else {
                None
            },
            downloads,
            uploads,
        })
    }
}

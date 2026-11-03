//! The authenticated Plan tape is reconstructed from the same signed Item versions as
//! native. A claimed `PlanV1` manifest never selects its own ancestry or grants write access.
use super::*;
use ::documents::plan::{
    CanonicalPlanSeed, LegacyVersion, PlanBlock, PlanBlockContent, PlanDocument, PlanUpdate,
};
use types::{PageManifest, PlanManifestV1};

pub(crate) struct PlanRead {
    pub(crate) document: PlanDoc,
    blocks: Vec<Value>,
    binding: String,
    pub(crate) ready: bool,
    pub(crate) error: Option<String>,
    pub(crate) downloads: Vec<ChunkRef>,
    accepted: u32,
    frozen: bool,
    pending: bool,
}

fn field(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
}

fn source(item: &Item, event: &Event) -> String {
    let mut digest = Sha256::new();
    digest.update(b"zoen.plan.timeline.source.v1\0");
    field(&mut digest, item.space.as_bytes());
    field(&mut digest, item.id.as_bytes());
    field(&mut digest, &event.content);
    hex::encode(digest.finalize())
}

fn legacy_blocks(item: &Item, document: &PlanDoc) -> Vec<Value> {
    let prefix = format!("local-legacy:{}:v{}", item.id, item.versions.len());
    let block = |suffix: &str, content: Value, text: &str| {
        json!({
            "id": format!("{prefix}:{suffix}"), "content": content, "text": text, "spans": [],
        })
    };
    let mut blocks = vec![
        block(
            "title",
            json!({"Title":{"budget_cents":document.budget_cents.map(|cents| cents.to_string())}}),
            &document.title,
        ),
        block("summary", json!("Summary"), &document.summary),
    ];
    for (section_index, section) in document.sections.iter().enumerate() {
        blocks.push(block(
            &format!("section:{section_index}"),
            json!("Section"),
            &section.title,
        ));
        for (line_index, line) in section.lines.iter().enumerate() {
            blocks.push(block(
                &format!("section:{section_index}:line:{line_index}"),
                json!({"Line":{
                    "legacy_id":line.id,"cost_cents":line.cost_cents.to_string(),"done":line.done,
                }}),
                &line.text,
            ));
        }
    }
    blocks
}

fn prose_kind(kind: &Kind) -> Value {
    match kind {
        Kind::Paragraph => json!("Paragraph"),
        Kind::Quote => json!("Quote"),
        Kind::Divider => json!("Divider"),
        Kind::Raw => json!("Raw"),
        Kind::Heading { level, setext } => json!({"Heading":{"level":level,"setext":setext}}),
        Kind::Bullet { indent } => json!({"Bullet":{"indent":indent}}),
        Kind::Numbered { indent, number } => json!({"Numbered":{"indent":indent,"number":number}}),
        Kind::Task { indent, checked } => json!({"Task":{"indent":indent,"checked":checked}}),
        Kind::Code { lang } => json!({"Code":{"lang":lang}}),
        Kind::Image { url, alt, title } => json!({"Image":{"url":url,"alt":alt,"title":title}}),
    }
}

fn canonical_block(block: &PlanBlock) -> Value {
    let content = match &block.content {
        PlanBlockContent::Title { budget_cents } => {
            json!({"Title":{"budget_cents":budget_cents.map(|cents|cents.to_string())}})
        }
        PlanBlockContent::Summary => json!("Summary"),
        PlanBlockContent::Section => json!("Section"),
        PlanBlockContent::Line {
            legacy_id,
            cost_cents,
            done,
        } => json!({"Line":{
            "legacy_id":legacy_id,"cost_cents":cost_cents.to_string(),"done":done,
        }}),
        PlanBlockContent::Prose { kind } => json!({"Prose":{"kind":prose_kind(kind)}}),
    };
    json!({"id":block.id,"content":content,"text":block.plain_text(),"spans": ::documents::spans_of(&block.runs)})
}

mod binding;
use binding::observed_hash;

fn append_legacy(
    history: &mut Vec<LegacyVersion>,
    prefixes: &mut Vec<PlanManifestV1>,
    digest: &mut Sha256,
    item: &Item,
    event: &Event,
    plan: &PlanDoc,
) -> Result<(), String> {
    let event_id = source(item, event);
    if history.is_empty() {
        field(digest, event_id.as_bytes());
    }
    field(digest, &event.content);
    history.push(LegacyVersion {
        event_id: event_id.clone(),
        document: plan.clone(),
    });
    prefixes.push(PlanManifestV1 {
        schema: "plan_v1".into(),
        space: item.space.clone(),
        item: item.id.clone(),
        creation_event: history
            .first()
            .ok_or("legacy plan creation missing")?
            .event_id
            .clone(),
        legacy_versions: u32::try_from(history.len()).map_err(|_| "plan history limit exceeded")?,
        legacy_head: event_id,
        prefix_sha256: hex::encode(digest.clone().finalize()),
    });
    Ok(())
}

fn import_plan(
    canonical: &mut Option<PlanDocument>,
    history: &[LegacyVersion],
    payload: &[u8],
) -> Result<Option<PlanDoc>, String> {
    let Ok(update) = PlanUpdate::from_json(payload) else {
        return Ok(None);
    };
    if canonical.is_none() {
        *canonical = Some(
            CanonicalPlanSeed::replay(history)
                .and_then(|seed| seed.open_editor())
                .map_err(error)?,
        );
    }
    let plan = canonical
        .as_mut()
        .ok_or("canonical plan initialization failed")?;
    if plan.import(&update).is_err() {
        return Ok(None);
    }
    Ok(Some(plan.view().map_err(error)?.to_plan_doc()))
}

fn plan_blocks(
    item: &Item,
    document: &PlanDoc,
    canonical: Option<&PlanDocument>,
    history: &[LegacyVersion],
    pending: bool,
    frozen: bool,
) -> Result<Vec<Value>, String> {
    Ok(if frozen {
        canonical
            .ok_or("canonical plan missing after cutover")?
            .blocks()
            .map_err(error)?
            .iter()
            .map(canonical_block)
            .collect()
    } else if !history.is_empty() && !pending {
        CanonicalPlanSeed::replay(history)
            .and_then(|seed| seed.open_editor())
            .and_then(|plan| plan.blocks())
            .map_err(error)?
            .iter()
            .map(canonical_block)
            .collect()
    } else {
        legacy_blocks(item, document)
    })
}

impl<S: Signer> Client<S> {
    pub(crate) fn plan_legacy_writable(&self, item: &Item) -> Result<bool, String> {
        let read = self.plan_read(item)?;
        Ok(read.ready && read.error.is_none() && !read.frozen && total(&read.document).is_some())
    }
    fn plan_payload(
        &self,
        page: &types::PageContent,
        downloads: &mut Vec<ChunkRef>,
    ) -> Option<Vec<u8>> {
        if let Some(reference) = &page.blob {
            match self.saved.documents.blobs.get(&reference.sha256) {
                Some(bytes)
                    if bytes.len() as u64 == reference.bytes && hash(bytes) == reference.sha256 =>
                {
                    Some(bytes.clone())
                }
                _ => {
                    downloads.push(reference.clone());
                    None
                }
            }
        } else {
            BASE64.decode(&page.loro).ok()
        }
    }

    fn read_plan_page(
        &self,
        page: &types::PageContent,
        prefixes: &[PlanManifestV1],
        history: &[LegacyVersion],
        canonical: &mut Option<PlanDocument>,
        downloads: &mut Vec<ChunkRef>,
    ) -> Result<Option<PlanDoc>, String> {
        let Some(PageManifest::PlanV1(manifest)) = &page.manifest else {
            return Ok(None);
        };
        if manifest
            .legacy_versions
            .checked_sub(1)
            .and_then(|index| prefixes.get(index as usize))
            != Some(manifest)
        {
            return Ok(None);
        }
        let payload = self.plan_payload(page, downloads);
        let Some(payload) = payload else {
            return Ok(None);
        };
        if !downloads.is_empty() {
            return Ok(None);
        }
        import_plan(canonical, history, &payload)
    }

    pub(crate) fn plan_read(&self, item: &Item) -> Result<PlanRead, String> {
        if item.kind != ItemKind::Plan {
            return Err("Este arquivo não é um plano.".into());
        }
        let mut history = Vec::new();
        let mut prefixes = Vec::new();
        let mut digest = Sha256::new();
        digest.update(b"zoen.plan.legacy.prefix.v1\0");
        field(&mut digest, item.space.as_bytes());
        field(&mut digest, item.id.as_bytes());
        let mut canonical: Option<PlanDocument> = None;
        let mut document = None;
        let mut frozen = false;
        let mut accepted = 0;
        let mut downloads = Vec::new();
        let mut failure = None;
        let mut pending = false;
        let explicit_origin = item.has_explicit_origin();
        for (index, version) in item.versions.iter().enumerate() {
            pending |= version.pending;
            if version.failure.is_some() {
                continue;
            }
            match &version.content {
                ItemContent::Plan(plan) if !frozen => {
                    if version.pending {
                        document = Some(plan.clone());
                        continue;
                    }
                    if !explicit_origin {
                        document = Some(plan.clone());
                        accepted =
                            u32::try_from(index + 1).map_err(|_| "plan version limit exceeded")?;
                        continue;
                    }
                    append_legacy(
                        &mut history,
                        &mut prefixes,
                        &mut digest,
                        item,
                        &version.event,
                        plan,
                    )?;
                    canonical = None;
                    document = Some(plan.clone());
                    accepted =
                        u32::try_from(index + 1).map_err(|_| "plan version limit exceeded")?;
                }
                ItemContent::Page(page)
                    if explicit_origin && !version.pending && !version.is_undo =>
                {
                    let Some(imported) = self.read_plan_page(
                        page,
                        &prefixes,
                        &history,
                        &mut canonical,
                        &mut downloads,
                    )?
                    else {
                        continue;
                    };
                    document = Some(imported);
                    frozen = true;
                    accepted =
                        u32::try_from(index + 1).map_err(|_| "plan version limit exceeded")?;
                }
                ItemContent::Page(_) if version.pending => {
                    failure = Some("Uma atualização do plano ainda está sendo confirmada.".into());
                }
                _ => {}
            }
        }
        let document =
            document.ok_or("O plano ainda está chegando ou não tem uma origem autenticada.")?;
        let blocks = plan_blocks(
            item,
            &document,
            canonical.as_ref(),
            &history,
            pending,
            frozen,
        )?;
        let binding = if frozen {
            observed_hash(&blocks)?
        } else {
            observed_hash(&legacy_blocks(item, &document))?
        };
        Ok(PlanRead {
            document,
            blocks,
            binding,
            ready: downloads.is_empty(),
            error: failure,
            downloads,
            accepted,
            frozen,
            pending,
        })
    }

    pub(super) fn plan_page_value(
        &self,
        item: &Item,
        downloads: &mut Vec<ChunkRef>,
    ) -> Result<Value, String> {
        let read = self.plan_read(item)?;
        downloads.extend(read.downloads);
        Ok(json!({ "item_id": item.id, "space_id": item.space,
            "source": if read.pending && !read.frozen { "LocalPendingLegacy" } else { "DocumentSession" },
            "version": read.accepted, "ready": read.ready,
            "legacy_edit_admitted": read.ready && read.error.is_none() && !read.frozen && total(&read.document).is_some() && self.document_writer(&item.space).is_ok(),
            "unsaved": false, "pending_sync": read.pending, "save_error": read.error,
            "content_hash": read.binding, "total_cents": total(&read.document).map(|cents| cents.to_string()), "blocks": read.blocks }))
    }

    fn editable_plan(&self, item: &Item, observed: &str) -> Result<PlanDoc, String> {
        self.document_writer(&item.space)?;
        let read = self.plan_read(item)?;
        if !read.ready || read.error.is_some() || read.frozen {
            return Err(
                "O plano exige o editor de página ou ainda está chegando; a edição não foi salva."
                    .into(),
            );
        }
        if read.binding != observed {
            return Err("O plano mudou. Revise antes de editar.".into());
        }
        if total(&read.document).is_none() {
            return Err("O total do plano excede o limite.".into());
        }
        Ok(read.document)
    }

    pub(super) fn edit_plan_document(
        &mut self,
        id: &str,
        observed: &str,
        note: &str,
        now: i64,
        change: impl FnOnce(&mut PlanDoc) -> Result<(), String>,
    ) -> Result<Value, String> {
        let item = self.document_item(id)?;
        let mut document = self.editable_plan(&item, observed)?;
        change(&mut document)?;
        if total(&document).is_none() {
            return Err("O total do plano excede o limite; nenhuma linha foi descartada.".into());
        }
        self.queue_domain_body(
            &item.space,
            &EventBody::ItemVersioned {
                item: id.into(),
                content: ItemContent::Plan(document),
                note: note.into(),
            },
            now,
            Vec::new(),
        )?;
        let updated = self.document_item(id)?;
        let binding = self.plan_read(&updated)?.binding;
        Ok(
            json!({ "item": self.item_value(&updated, None, &mut Vec::new())?,
            "undo": { "item_id": id, "restore_version": item.versions.len(), "observed_hash": binding,
                "result_version": updated.versions.len(), "label": note }, "reaction": null }),
        )
    }

    pub(super) fn restore_plan(
        &mut self,
        id: &str,
        version: u32,
        observed: &str,
        note: &str,
        now: i64,
    ) -> Result<(), String> {
        let item = self.document_item(id)?;
        self.editable_plan(&item, observed)?;
        if version == 0 || version as usize > item.versions.len() {
            return Err("Versão indisponível.".into());
        }
        if !matches!(
            item.versions
                .get(version as usize - 1)
                .ok_or("plan version unavailable")?
                .content,
            ItemContent::Plan(_)
        ) {
            return Err("Esta versão exige o editor de página.".into());
        }
        self.queue_domain_body(
            &item.space,
            &EventBody::ItemReverted {
                item: id.into(),
                to_version: version,
                note: note.into(),
            },
            now,
            Vec::new(),
        )
    }
}

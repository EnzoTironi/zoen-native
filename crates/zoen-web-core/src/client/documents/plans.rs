//! The authenticated Plan tape is reconstructed from the same signed Item versions as
//! native. A claimed PlanV1 manifest never selects its own ancestry or grants write access.
use super::*;
use roda_docs::plan::{
    CanonicalPlanSeed, LegacyVersion, PlanBlock, PlanBlockContent, PlanDocument, PlanUpdate,
};
use roda_types::{PageManifest, PlanManifestV1};

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
    json!({"id":block.id,"content":content,"text":block.plain_text(),"spans":roda_docs::spans_of(&block.runs)})
}

fn string_field(digest: &mut Sha256, text: &str) {
    digest.update((text.len() as u64).to_le_bytes());
    digest.update(text.as_bytes());
}

/// Native's exact PlanDoc binding: presentation IDs and relay confirmation stay out.
fn observed_hash(blocks: &[Value]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"zoen-plan-observed-v1\0");
    digest.update((blocks.len() as u64).to_le_bytes());
    for block in blocks {
        let content = &block["content"];
        if let Some(title) = content.get("Title") {
            digest.update([1]);
            match title["budget_cents"].as_str() {
                Some(cents) => {
                    digest.update([1]);
                    digest.update(cents.parse::<i64>().expect("typed cents").to_le_bytes());
                }
                None => digest.update([0]),
            }
        } else if content == "Summary" {
            digest.update([2]);
        } else if content == "Section" {
            digest.update([3]);
        } else if let Some(line) = content.get("Line") {
            digest.update([4]);
            string_field(&mut digest, line["legacy_id"].as_str().expect("typed id"));
            digest.update(
                line["cost_cents"]
                    .as_str()
                    .expect("typed cents")
                    .parse::<i64>()
                    .expect("typed cents")
                    .to_le_bytes(),
            );
            digest.update([u8::from(line["done"].as_bool().expect("typed boolean"))]);
        } else if let Some(prose) = content.get("Prose") {
            digest.update([5]);
            hash_prose(&mut digest, &prose["kind"]);
        }
        string_field(&mut digest, block["text"].as_str().expect("typed text"));
        let spans = block["spans"].as_array().expect("typed spans");
        digest.update((spans.len() as u64).to_le_bytes());
        for span in spans {
            digest.update((span["start"].as_u64().expect("typed offset") as u32).to_le_bytes());
            digest.update((span["end"].as_u64().expect("typed offset") as u32).to_le_bytes());
            string_field(&mut digest, span["key"].as_str().expect("typed key"));
            string_field(&mut digest, span["value"].as_str().expect("typed value"));
        }
    }
    hex::encode(digest.finalize())
}

fn hash_prose(digest: &mut Sha256, kind: &Value) {
    match kind.as_str() {
        Some("Paragraph") => digest.update([1]),
        Some("Quote") => digest.update([6]),
        Some("Divider") => digest.update([8]),
        Some("Raw") => digest.update([10]),
        _ => {
            if let Some(heading) = kind.get("Heading") {
                digest.update([2]);
                digest.update((heading["level"].as_u64().unwrap() as u32).to_le_bytes());
                digest.update([u8::from(heading["setext"].as_bool().unwrap())]);
            } else if let Some(bullet) = kind.get("Bullet") {
                digest.update([3]);
                digest.update((bullet["indent"].as_u64().unwrap() as u32).to_le_bytes());
            } else if let Some(numbered) = kind.get("Numbered") {
                digest.update([4]);
                digest.update((numbered["indent"].as_u64().unwrap() as u32).to_le_bytes());
                digest.update((numbered["number"].as_u64().unwrap() as u32).to_le_bytes());
            } else if let Some(task) = kind.get("Task") {
                digest.update([5]);
                digest.update((task["indent"].as_u64().unwrap() as u32).to_le_bytes());
                digest.update([u8::from(task["checked"].as_bool().unwrap())]);
            } else if let Some(code) = kind.get("Code") {
                digest.update([7]);
                string_field(digest, code["lang"].as_str().unwrap());
            } else if let Some(image) = kind.get("Image") {
                digest.update([9]);
                for key in ["url", "alt", "title"] {
                    string_field(digest, image[key].as_str().unwrap());
                }
            }
        }
    }
}

impl<S: Signer> Client<S> {
    pub(crate) fn plan_legacy_writable(&self, item: &Item) -> Result<bool, String> {
        let read = self.plan_read(item)?;
        Ok(read.ready && read.error.is_none() && !read.frozen && total(&read.document).is_some())
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
                        accepted = (index + 1) as u32;
                        continue;
                    }
                    let event_id = source(item, &version.event);
                    if history.is_empty() {
                        field(&mut digest, event_id.as_bytes());
                    }
                    field(&mut digest, &version.event.content);
                    history.push(LegacyVersion {
                        event_id: event_id.clone(),
                        document: plan.clone(),
                    });
                    prefixes.push(PlanManifestV1 {
                        schema: "plan_v1".into(),
                        space: item.space.clone(),
                        item: item.id.clone(),
                        creation_event: history[0].event_id.clone(),
                        legacy_versions: history.len() as u32,
                        legacy_head: event_id,
                        prefix_sha256: hex::encode(digest.clone().finalize()),
                    });
                    canonical = None;
                    document = Some(plan.clone());
                    accepted = (index + 1) as u32;
                }
                ItemContent::Page(page)
                    if explicit_origin && !version.pending && !version.is_undo =>
                {
                    let Some(PageManifest::PlanV1(manifest)) = &page.manifest else {
                        continue;
                    };
                    if manifest
                        .legacy_versions
                        .checked_sub(1)
                        .and_then(|index| prefixes.get(index as usize))
                        != Some(manifest)
                    {
                        continue;
                    }
                    let payload = if let Some(reference) = &page.blob {
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
                        BASE64.decode(&page.loro).ok()
                    };
                    let Some(payload) = payload else {
                        continue;
                    };
                    if !downloads.is_empty() {
                        continue;
                    }
                    let update = match PlanUpdate::from_json(&payload) {
                        Ok(update) => update,
                        Err(_) => continue,
                    };
                    if canonical.is_none() {
                        canonical = Some(
                            CanonicalPlanSeed::replay(&history)
                                .and_then(|seed| seed.open_editor())
                                .map_err(error)?,
                        );
                    }
                    let plan = canonical.as_mut().expect("initialized above");
                    if plan.import(&update).is_err() {
                        continue;
                    }
                    document = Some(plan.view().map_err(error)?.to_plan_doc());
                    frozen = true;
                    accepted = (index + 1) as u32;
                }
                ItemContent::Page(_) if version.pending => {
                    failure = Some("Uma atualização do plano ainda está sendo confirmada.".into());
                }
                _ => {}
            }
        }
        let document =
            document.ok_or("O plano ainda está chegando ou não tem uma origem autenticada.")?;
        let blocks = if frozen {
            canonical
                .as_ref()
                .expect("canonical cutover")
                .blocks()
                .map_err(error)?
                .iter()
                .map(canonical_block)
                .collect()
        } else if !history.is_empty() && !pending {
            CanonicalPlanSeed::replay(&history)
                .and_then(|seed| seed.open_editor())
                .and_then(|plan| plan.blocks())
                .map_err(error)?
                .iter()
                .map(canonical_block)
                .collect()
        } else {
            legacy_blocks(item, &document)
        };
        let binding = if !frozen {
            observed_hash(&legacy_blocks(item, &document))
        } else {
            observed_hash(&blocks)
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
            EventBody::ItemVersioned {
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
            item.versions[version as usize - 1].content,
            ItemContent::Plan(_)
        ) {
            return Err("Esta versão exige o editor de página.".into());
        }
        self.queue_domain_body(
            &item.space,
            EventBody::ItemReverted {
                item: id.into(),
                to_version: version,
                note: note.into(),
            },
            now,
            Vec::new(),
        )
    }
}

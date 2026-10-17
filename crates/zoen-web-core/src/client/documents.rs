//! Browser document adapter. Signed Space history is canonical; the existing encrypted
//! browser snapshot owns only disposable projections, downloaded payloads and local drafts.
//! Pages use roda-docs' exact Loro/Markdown codec and share native Item events and blobs.

use super::*;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use roda_docs::{BlockEdit, Frontiers, Kind, Page};
use roda_i18n::{t, tr};
use roda_types::{ChunkRef, ItemContent, ItemKind, PageContent, PlanDoc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const INLINE_MAX: usize = 48 << 10;
const MAX_PAGE_BYTES: usize = 32 << 20;
const MAX_REQUEST_BYTES: usize = 1 << 20;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Journal {
    drafts: BTreeMap<String, Draft>,
    /// Hash-addressed verified page downloads; never an independent editable document.
    blobs: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Draft {
    origin: String,
    snapshot: Vec<u8>,
    receipts: BTreeMap<String, Receipt>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Receipt {
    fingerprint: String,
    context_hash: String,
    content_hash: String,
    frontiers: Vec<u8>,
    sequence: usize,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Context {
    item: String,
    space: String,
    origin: String,
    frontiers: Vec<u8>,
}

#[derive(Serialize)]
pub struct DocumentResponse {
    pub result: Value,
    pub update: Option<Update>,
    pub downloads: Vec<ChunkRef>,
    pub uploads: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Span {
    start: u32,
    end: u32,
    key: String,
    value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Block {
    id: String,
    kind: String,
    level: u32,
    indent: u32,
    number: u32,
    checked: bool,
    lang: String,
    url: String,
    alt: String,
    text: String,
    spans: Vec<Span>,
}

impl From<&roda_docs::Block> for Block {
    fn from(block: &roda_docs::Block) -> Self {
        let mut dto = Self {
            id: block.id.clone(),
            kind: block.kind.id().into(),
            level: 0,
            indent: 0,
            number: 0,
            checked: false,
            lang: String::new(),
            url: String::new(),
            alt: String::new(),
            text: block.plain_text(),
            spans: roda_docs::spans_of(&block.runs)
                .into_iter()
                .map(|span| Span {
                    start: span.start,
                    end: span.end,
                    key: span.key,
                    value: span.value,
                })
                .collect(),
        };
        match &block.kind {
            Kind::Heading { level, .. } => dto.level = *level,
            Kind::Bullet { indent } => dto.indent = *indent,
            Kind::Numbered { indent, number } => {
                dto.indent = *indent;
                dto.number = *number;
            }
            Kind::Task { indent, checked } => {
                dto.indent = *indent;
                dto.checked = *checked;
            }
            Kind::Code { lang } => dto.lang = lang.clone(),
            Kind::Image { url, alt, .. } => {
                dto.url = url.clone();
                dto.alt = alt.clone();
            }
            _ => {}
        }
        dto
    }
}

impl Block {
    fn edit(&self, previous: Option<&Kind>) -> Result<BlockEdit, String> {
        let kind = match self.kind.as_str() {
            "paragraph" => Kind::Paragraph,
            "heading" => Kind::Heading {
                level: self.level.clamp(1, 6),
                setext: matches!(previous, Some(Kind::Heading { setext: true, level }) if *level == self.level),
            },
            "bullet" => Kind::Bullet {
                indent: self.indent,
            },
            "numbered" => Kind::Numbered {
                indent: self.indent,
                number: self.number.max(1),
            },
            "task" => Kind::Task {
                indent: self.indent,
                checked: self.checked,
            },
            "quote" => Kind::Quote,
            "code" => Kind::Code {
                lang: self.lang.clone(),
            },
            "divider" => Kind::Divider,
            "image" => Kind::Image {
                url: self.url.clone(),
                alt: self.alt.clone(),
                title: match previous {
                    Some(Kind::Image { title, .. }) => title.clone(),
                    _ => String::new(),
                },
            },
            "raw" => Kind::Raw,
            _ => return Err("Tipo de bloco desconhecido; o conteúdo foi preservado.".into()),
        };
        Ok(BlockEdit {
            id: self.id.clone(),
            kind,
            text: self.text.clone(),
            spans: self
                .spans
                .iter()
                .map(|span| roda_docs::Span {
                    start: span.start,
                    end: span.end,
                    key: span.key.clone(),
                    value: span.value.clone(),
                })
                .collect(),
        })
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    NewMutationId {},
    PageParseMarkdown {
        markdown: String,
    },
    PageBlockMarkdown {
        block: Block,
    },
    Items {},
    Item {
        item_id: String,
    },
    ItemAt {
        item_id: String,
        version: u32,
    },
    FileManifest {
        item_id: String,
        version: Option<u32>,
    },
    Page {
        item_id: String,
    },
    PageAt {
        item_id: String,
        version: u32,
    },
    PlanPage {
        item_id: String,
    },
    PageCreate {
        space_id: String,
        title: String,
    },
    PageImportMarkdown {
        space_id: String,
        path: String,
        markdown: String,
    },
    PageApply {
        item_id: String,
        mutation_id: String,
        edit_context: String,
        order: Vec<String>,
        changed: Vec<Block>,
    },
    PageCommit {
        item_id: String,
        note: String,
    },
    PageMarkdown {
        item_id: String,
    },
    TogglePlanLine {
        item_id: String,
        line_id: String,
        observed_hash: String,
    },
    EditPlanLine {
        item_id: String,
        line_id: String,
        text: String,
        cost_cents: String,
        observed_hash: String,
    },
    AddPlanLine {
        item_id: String,
        section_index: u32,
        text: String,
        cost_cents: String,
        observed_hash: String,
    },
    RemovePlanLine {
        item_id: String,
        line_id: String,
        observed_hash: String,
    },
    RestorePlanVersion {
        item_id: String,
        version: u32,
        observed_hash: String,
    },
    RestoreVersion {
        item_id: String,
        version: u32,
    },
    Undo {
        token: Undo,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Undo {
    item_id: String,
    restore_version: u32,
    observed_hash: String,
    result_version: u32,
    label: String,
}

#[derive(Clone)]
pub(crate) struct Version {
    pub(crate) content: ItemContent,
    pub(crate) event: Event,
    pub(crate) note: String,
    pub(crate) is_undo: bool,
    pub(crate) pending: bool,
    pub(crate) failure: Option<String>,
}

#[derive(Clone)]
pub(crate) struct Item {
    pub(crate) id: String,
    pub(crate) space: String,
    pub(crate) kind: ItemKind,
    pub(crate) origin: String,
    pub(crate) versions: Vec<Version>,
}

impl Item {
    pub(crate) fn current(&self) -> &Version {
        self.versions.last().expect("created item")
    }
    fn origin_hash(&self) -> String {
        // Signed content identity survives transport confirmation and resealing.
        hex::encode(Sha256::digest(&self.versions[0].event.content))
    }
    pub(crate) fn has_explicit_origin(&self) -> bool {
        let event = &self.versions[0].event;
        roda_log::event_from_content(
            event.content.clone(),
            event.sig.clone(),
            event.cert.clone(),
            0,
            String::new(),
            String::new(),
        )
        .is_ok_and(|source| matches!(source.body, EventBody::ItemCreated { .. }))
    }
}

struct PageRead {
    shadow: Page,
    live: Page,
    ready: bool,
    error: Option<String>,
    downloads: Vec<ChunkRef>,
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn hash(value: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(value))
}
fn cents(value: &str) -> Result<i64, String> {
    if value.is_empty() || value.trim() != value || value.starts_with('+') {
        return Err("Valor em centavos inválido.".into());
    }
    value
        .parse()
        .map_err(|_| "Valor em centavos fora do limite.".into())
}
fn total(plan: &PlanDoc) -> Option<i64> {
    plan.sections
        .iter()
        .flat_map(|section| &section.lines)
        .try_fold(0_i128, |sum, line| {
            sum.checked_add(i128::from(line.cost_cents))
        })
        .and_then(|sum| i64::try_from(sum).ok())
}

impl<S: Signer> Client<S> {
    pub(super) fn document_items(&self) -> Result<BTreeMap<String, Item>, String> {
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

    pub(super) fn document_item(&self, id: &str) -> Result<Item, String> {
        self.document_items()?
            .remove(id)
            .ok_or_else(|| "Arquivo indisponível neste aparelho.".into())
    }

    pub(super) fn document_writer(&self, space: &str) -> Result<(), String> {
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

    pub(super) fn queue_domain_body(
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
    pub(super) fn queue_domain_batch(
        &mut self,
        space: &str,
        bodies: Vec<EventBody>,
        now: i64,
    ) -> Result<(), String> {
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
            pending.order = Some(self.saved.outbox_sequence + index as u64 + 1);
            prepared.push((event.client_id.clone(), pending));
        }
        self.saved.outbox_sequence = end;
        self.saved.outbox.extend(prepared);
        Ok(())
    }

    fn read_page(&self, item: &Item, include_draft: bool) -> Result<PageRead, String> {
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

    fn page_context(&self, item: &Item, frontiers: Vec<u8>) -> Result<String, String> {
        let bytes = serde_json::to_vec(&Context {
            item: item.id.clone(),
            space: item.space.clone(),
            origin: item.origin_hash(),
            frontiers,
        })
        .map_err(error)?;
        Ok(format!("web-page-v1:{}", BASE64.encode(bytes)))
    }

    fn page_value(&self, item: &Item, read: &PageRead, historical: bool) -> Result<Value, String> {
        let can_edit = !historical && read.ready && self.document_writer(&item.space).is_ok();
        let content = match &item.current().content {
            ItemContent::Page(page) => page,
            _ => return Err("Esta versão não é uma página.".into()),
        };
        Ok(json!({
            "item_id": item.id, "space_id": item.space, "title": content.title, "path": content.path,
            "version": item.versions.len(), "ready": read.ready,
            "unsaved": !historical && read.live.content_hash() != read.shadow.content_hash(),
            "pending_sync": !historical && item.versions.iter().any(|version| version.pending),
            "save_error": read.error.clone().or_else(|| item.versions.iter().find_map(|version| version.failure.clone())),
            "can_edit": can_edit, "content_hash": read.live.content_hash(),
            "edit_context": if can_edit { self.page_context(item, read.live.frontiers().encode())? } else { String::new() },
            "blocks": read.live.blocks().iter().map(Block::from).collect::<Vec<_>>()
        }))
    }

    pub(super) fn persona_value(&self, id: &str) -> Value {
        let profile = self.saved.profiles.get(id);
        let name = profile
            .map(|profile| profile.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| {
                if id == self.account.identity {
                    self.account.name.clone()
                } else if let Some(profile) = profile.filter(|profile| !profile.handle.is_empty()) {
                    format!("@{}", profile.handle)
                } else {
                    "Desconhecido".into()
                }
            });
        let owner = profile.and_then(|profile| profile.owner.as_ref());
        let owner_profile = owner.and_then(|id| self.saved.profiles.get(id));
        let photo = self.fields_of(id).and_then(|(_, fields)| fields.photo);
        json!({ "id": id, "kind": profile.map(|profile| profile.kind).unwrap_or(roda_types::IdentityKind::Person),
            "name": name, "handle": profile.map(|profile| profile.handle.as_str()).unwrap_or(if id==self.account.identity {&self.account.handle}else{""}),
            "initials": name.split_whitespace().take(2).filter_map(|part| part.chars().next()).collect::<String>().to_uppercase(),
            "tint_hex": profile.map(|profile| profile.tint_hex.as_str()).unwrap_or("#8E8E93"),
            "glyph": profile.and_then(|profile| profile.glyph.as_ref()), "bio": profile.map(|profile| profile.bio.as_str()).unwrap_or(""),
            "owner_id": owner, "owner_name": owner_profile.map(|profile| &profile.name),
            "owner_tint_hex": owner_profile.map(|profile| &profile.tint_hex),
            "photo_sha256": photo.as_ref().map(|photo| &photo.sha256), "photo_mime": photo.as_ref().map(|photo| &photo.mime),
            "photo_ready": photo.as_ref().is_some_and(|photo| self.profile_photo_cached(photo)),
            "is_me": id == self.account.identity, "is_mine": owner.is_some_and(|owner| *owner == self.account.identity) })
    }

    pub(super) fn item_value(
        &self,
        item: &Item,
        at: Option<u32>,
        downloads: &mut Vec<ChunkRef>,
    ) -> Result<Value, String> {
        let mut selected = item.clone();
        if let Some(version) = at {
            if version == 0 || version as usize > item.versions.len() {
                return Err("Versão indisponível.".into());
            }
            selected.versions.truncate(version as usize);
        }
        let current = selected.current();
        let mut plan = None;
        let mut text = None;
        let mut path = String::new();
        let mut file = None;
        let mut title = current.content.title();
        if item.kind == ItemKind::Plan {
            let read = self.plan_read(&selected)?;
            downloads.extend(read.downloads.clone());
            title = read.document.title.clone();
            plan = Some(plan_value(&read.document)?);
        }
        match &current.content {
            ItemContent::Text { text: value } => text = Some(value.clone()),
            ItemContent::Plan(_) => {}
            ItemContent::Page(content) if item.kind == ItemKind::Page => {
                let read = self.read_page(&selected, at.is_none())?;
                downloads.extend(read.downloads.clone());
                text = Some(
                    read.live
                        .blocks()
                        .iter()
                        .map(|block| block.plain_text())
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                path = content.path.clone();
            }
            ItemContent::Page(_) if item.kind == ItemKind::Plan => {}
            ItemContent::File(document) => {
                path = document.path.clone();
                file = Some(json!({
                    "name": document.name, "path": document.path, "mime": document.mime,
                    "bytes": document.bytes.to_string(), "sha256": document.sha256,
                    "ready": document.chunks.iter().all(|chunk| self.saved.local_chunks.contains_key(&chunk.blob)),
                    "chunks": document.chunks.len(),
                    "chunks_here": document.chunks.iter().filter(|chunk| self.saved.local_chunks.contains_key(&chunk.blob)).count(),
                    "has_thumbnail": document.thumb.is_some(),
                }));
            }
            _ => {}
        }
        let (kind_id, kind_label) = match item.kind {
            ItemKind::Plan => ("plan", t("Plano", "Plan")),
            ItemKind::Task => ("task", t("Tarefa", "Task")),
            ItemKind::Note => ("note", t("Nota", "Note")),
            ItemKind::Page => ("page", t("Página", "Page")),
            ItemKind::File => ("file", t("Arquivo", "File")),
            ItemKind::App => ("app", t("Miniapp", "Miniapp")),
        };
        Ok(json!({ "id": item.id, "space_id": item.space,
            "space_title": self.metadata(&item.space).map(|metadata| metadata.0).unwrap_or_default(),
            "kind_id": kind_id, "kind_label": kind_label, "title": title, "origin": item.origin,
            "created_by": self.persona_value(&item.versions[0].event.author), "plan": plan, "text": text,
            "version": selected.versions.len(), "versions": item.versions.iter().enumerate().rev().map(|(index, version)| json!({
                "number": index + 1, "author": self.persona_value(&version.event.author),
                "at_ms": version.event.at_ms.to_string(), "note": version.note, "is_undo": version.is_undo,
                "total_cents": match &version.content { ItemContent::Plan(document) => total(document).map(|total| total.to_string()), _ => None },
            })).collect::<Vec<_>>(), "linked_requests": [], "app": self.app_state_value(&selected,self.view_now_ms.max(selected.current().event.at_ms))?, "path": path, "file": file }))
    }

    pub(super) fn item_card_value(&self, space: &str, id: &str) -> Result<Option<Value>, String> {
        let item = self.document_item(id)?;
        if item.space != space {
            return Ok(None);
        }
        let detail = self.item_value(&item, None, &mut Vec::new())?;
        let plan = detail["plan"].as_object();
        let lines = plan
            .and_then(|plan| plan.get("sections"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|section| section["lines"].as_array())
            .flatten()
            .collect::<Vec<_>>();
        Ok(Some(
            json!({"item_id":id,"title":detail["title"],"summary":plan.and_then(|plan|plan.get("summary")).and_then(Value::as_str)
            .or_else(||detail["app"]["headline"].as_str()).or_else(||detail["text"].as_str()).unwrap_or(""),
            "kind_id":detail["kind_id"],"kind_label":detail["kind_label"],"version":detail["version"],
            "total_cents":plan.and_then(|plan|plan.get("total_cents")),"budget_cents":plan.and_then(|plan|plan.get("budget_cents")),
            "line_count":lines.len(),"done_count":lines.iter().filter(|line|line["done"]==true).count(),"app":detail["app"]}),
        ))
    }

    fn page_content(
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

    pub(super) fn pending_document_uploads(&self) -> Vec<String> {
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

    pub fn document_request(
        &mut self,
        request: &str,
        now: i64,
    ) -> Result<DocumentResponse, String> {
        self.view_now_ms = now;
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
            Command::ItemAt { item_id, version } => self.item_value(
                &self.document_item(&item_id)?,
                Some(version),
                &mut downloads,
            )?,
            Command::FileManifest { item_id, version } => {
                let item = self.document_item(&item_id)?;
                if item.kind != ItemKind::File {
                    return Err("Este arquivo não contém mídia.".into());
                }
                let number = version.unwrap_or(item.versions.len() as u32);
                let current = number
                    .checked_sub(1)
                    .and_then(|index| item.versions.get(index as usize))
                    .ok_or("Versão indisponível.")?;
                let ItemContent::File(document) = &current.content else {
                    return Err("Esta versão não contém mídia.".into());
                };
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
                            .push(roda_types::PlanLine {
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

    fn create_page(
        &mut self,
        space: &str,
        path: &str,
        page: Page,
        origin: &str,
        now: i64,
    ) -> Result<String, String> {
        let mut title = page.title();
        if title.is_empty() {
            title = path
                .rsplit('/')
                .next()
                .unwrap_or("")
                .trim_end_matches(".md")
                .to_owned();
        }
        if title.is_empty() {
            title = t("Sem título", "Untitled");
        }
        let (content, uploads) = self.page_content(title, path.into(), page.snapshot())?;
        let item = browser_id("it", now);
        self.queue_domain_body(
            space,
            EventBody::ItemCreated {
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

    fn apply_page(
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
                "replayed": true, "applied_edit_context": self.page_context(item, receipt.frontiers.clone())?, "page": self.page_value(item, &read, false)? }),
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
                    spans: roda_docs::spans_of(&block.runs),
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
            content_hash: observed.content_hash(),
            frontiers: observed.frontiers().encode(),
            sequence: self
                .saved
                .documents
                .drafts
                .get(&item.id)
                .map_or(0, |draft| draft.receipts.len()),
        };
        let applied_context = self.page_context(item, receipt.frontiers.clone())?;
        let content_hash = receipt.content_hash.clone();
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
            json!({ "mutation_hash": request_hash, "mutation_id": mutation, "applied_content_hash": content_hash,
            "replayed": false, "applied_edit_context": applied_context, "page": self.page_value(item, &read, false)? }),
        )
    }

    fn commit_page(&mut self, item: &Item, note: &str, now: i64) -> Result<bool, String> {
        self.document_writer(&item.space)?;
        let read = self.read_page(item, true)?;
        if !read.ready {
            return Err("A página ainda está chegando.".into());
        }
        if read.live.content_hash() == read.shadow.content_hash() {
            return Ok(false);
        }
        let path = match &item.versions[0].content {
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
            EventBody::ItemVersioned {
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

fn fold_item(
    items: &mut BTreeMap<String, Item>,
    space: &str,
    event: &Event,
    pending: bool,
    failure: Option<String>,
) -> Result<(), String> {
    match &event.body {
        EventBody::ItemCreated {
            item,
            kind,
            content,
            origin,
        } => {
            if let Some(existing) = items.get(item) {
                if existing.space != space {
                    return Err(
                        "Dois espaços reivindicam o mesmo arquivo; nenhuma versão foi escolhida."
                            .into(),
                    );
                }
                return Ok(());
            }
            items.insert(
                item.clone(),
                Item {
                    id: item.clone(),
                    space: space.into(),
                    kind: *kind,
                    origin: origin.clone(),
                    versions: vec![Version {
                        content: content.clone(),
                        event: event.clone(),
                        note: origin.clone(),
                        is_undo: false,
                        pending,
                        failure,
                    }],
                },
            );
        }
        EventBody::ItemVersioned {
            item,
            content,
            note,
        } => {
            if let Some(item) = items.get_mut(item).filter(|item| item.space == space) {
                if item.kind == ItemKind::File && item.versions.iter().any(|version| matches!(&version.content,ItemContent::File(file) if file.is_voice())) && item.versions[0].event.author != event.author { return Ok(()); }
                let content =
                    roda_apps::reconcile_concurrent(&item.current().content, content, event.at_ms)
                        .unwrap_or_else(|| content.clone());
                item.versions.push(Version {
                    content,
                    event: event.clone(),
                    note: note.clone(),
                    is_undo: false,
                    pending,
                    failure,
                });
            }
        }
        EventBody::ItemReverted {
            item,
            to_version,
            note,
        } => {
            if let Some(item) = items.get_mut(item).filter(|item| item.space == space) {
                if let Some(content) = to_version
                    .checked_sub(1)
                    .and_then(|index| item.versions.get(index as usize))
                    .map(|version| version.content.clone())
                {
                    item.versions.push(Version {
                        content,
                        event: event.clone(),
                        note: note.clone(),
                        is_undo: true,
                        pending,
                        failure,
                    });
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn plan_value(document: &PlanDoc) -> Result<Value, String> {
    let total =
        total(document).ok_or("O total do plano excede o limite; as linhas foram preservadas.")?;
    Ok(
        json!({ "title": document.title, "summary": document.summary,
        "budget_cents": document.budget_cents.map(|cents| cents.to_string()), "total_cents": total.to_string(),
        "sections": document.sections.iter().map(|section| json!({ "title": section.title,
            "lines": section.lines.iter().map(|line| json!({ "id": line.id, "text": line.text, "cost_cents": line.cost_cents.to_string(), "done": line.done })).collect::<Vec<_>>() })).collect::<Vec<_>>() }),
    )
}

fn valid_line(text: String, cost: i64) -> Result<String, String> {
    let text = text.trim().to_owned();
    if text.is_empty() || cost < 0 {
        return Err("Informe o texto e um custo positivo para a linha.".into());
    }
    Ok(text)
}

fn unique_line_mut<'a>(
    plan: &'a mut PlanDoc,
    id: &str,
) -> Result<&'a mut roda_types::PlanLine, String> {
    if plan
        .sections
        .iter()
        .flat_map(|section| &section.lines)
        .filter(|line| line.id == id)
        .count()
        != 1
    {
        return Err("A identidade desta linha é ambígua ou está indisponível.".into());
    }
    plan.line_mut(id)
        .ok_or_else(|| "Linha indisponível.".into())
}

mod plans;

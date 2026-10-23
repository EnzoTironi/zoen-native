//! Browser document adapter. Signed Space history is canonical; the existing encrypted
//! browser snapshot owns only disposable projections, downloaded payloads and local drafts.
//! Pages use documents' exact Loro/Markdown codec and share native Item events and blobs.

use super::*;
use ::documents::{BlockEdit, Frontiers, Kind, Page};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use localization::{t, tr};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use types::{ChunkRef, ItemContent, ItemKind, PageContent, PlanDoc};

mod apps;
mod discussion;
mod dispatch;
mod exports;
mod pages;
mod storage;
mod views;

const INLINE_MAX: usize = 48 << 10;
const MAX_PAGE_BYTES: usize = 32 << 20;
const MAX_REQUEST_BYTES: usize = 1 << 20;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Journal {
    drafts: BTreeMap<String, Draft>,
    /// Hash-addressed verified page downloads; never an independent editable document.
    blobs: BTreeMap<String, Vec<u8>>,
    #[serde(default)]
    drawings: apps::Journal,
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

impl From<&::documents::Block> for Block {
    fn from(block: &::documents::Block) -> Self {
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
            spans: ::documents::spans_of(&block.runs)
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
                .map(|span| ::documents::Span {
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
    ItemDiscussion {
        space_id: String,
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
    PageExportVersion {
        space_id: String,
        item_id: String,
        version: u32,
        observed_hash: String,
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
        event_log::event_from_content(
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

/// Views refer to the exact source event accepted by the existing Item fold.
/// The four-part key keeps conversation, author, optimistic id and canonical hash
/// separate, including while a pending write receives its relay confirmation.
pub(super) fn item_edits(items: &BTreeMap<String, Item>) -> BTreeMap<ItemEditKey, ItemEditView> {
    let mut edits = BTreeMap::new();
    for item in items.values() {
        let title = item.current().content.title();
        for (index, version) in item.versions.iter().enumerate().skip(1) {
            let source_item = match &version.event.body {
                EventBody::ItemVersioned { item, .. } | EventBody::ItemReverted { item, .. } => {
                    item
                }
                _ => continue,
            };
            if *source_item != item.id || version.event.space != item.space {
                continue;
            }
            let Ok(number) = u32::try_from(index + 1) else {
                continue;
            };
            edits.insert(
                item_edit_key(&version.event),
                ItemEditView {
                    space_id: item.space.clone(),
                    item_id: item.id.clone(),
                    title: title.clone(),
                    version: number,
                    note: version.note.clone(),
                    is_undo: version.is_undo,
                },
            );
        }
    }
    edits
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
                    miniapps::reconcile_concurrent(&item.current().content, content, event.at_ms)
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

fn unique_line_mut<'a>(plan: &'a mut PlanDoc, id: &str) -> Result<&'a mut types::PlanLine, String> {
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

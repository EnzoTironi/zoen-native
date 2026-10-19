//! Pages (ADR 0040): a Loro document per page, its history in the Space log.
//!
//! Version 1 of a page carries a snapshot; every later version carries the Loro updates
//! made since the previous one. Payloads over 48 KB go to an encrypted blob instead of the
//! event. Each device keeps two copies of a page in memory: `shadow`, exactly the versions
//! in the log, and `live`, the shadow plus this person's edits not yet saved. Saving
//! exports `live` minus `shadow` as the next version.

use std::collections::HashMap;

use base64::Engine as _;
use roda_docs::{BlockEdit, Kind, Page};
use roda_types::*;

use crate::engine::{Engine, ItemState, R};
use crate::i18n::t;
use crate::CoreError;

mod drafts;
mod input;
pub use input::{
    PageInputAck, PageInputApplyResult, PageInputBindingDto, PageInputCaptureDto, PageInputDto,
    PageInputMode, PageInputTarget,
};
mod plan;
pub(crate) use plan::projection::{
    BeforeEvent, Currentness, PlanOrigin, PlanRead, PlanReplay, Projection,
};
pub(crate) use plan::PlanSnapshot;
pub use plan::{
    PlanPageBlockDto, PlanPageContentDto, PlanPageDto, PlanPageReadSourceDto, PlanProseKindDto,
};
mod session;
#[cfg(test)]
mod tests;

use session::{CommitStatus, DocumentEdit, DocumentView, PageSession, SessionRead};

/// Largest payload that rides inside the event (base64 adds a third; envelopes cap at 90 KB).
const INLINE_MAX: usize = 48 * 1024;

#[derive(Debug, Clone, PartialEq, serde::Serialize, uniffi::Record, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct TextSpanDto {
    /// UTF-16 offsets into the block's text.
    pub start: u32,
    pub end: u32,
    /// "b" bold, "i" italic, "s" strike, "c" code, "a" link (value = URL), "hb" hard break.
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, uniffi::Record, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct PageBlockDto {
    pub id: String,
    /// paragraph, heading, bullet, numbered, task, quote, code, divider, image, raw
    pub kind: String,
    pub level: u32,
    pub indent: u32,
    pub number: u32,
    pub checked: bool,
    pub lang: String,
    pub url: String,
    pub alt: String,
    /// Line breaks inside a block are U+2028.
    pub text: String,
    pub spans: Vec<TextSpanDto>,
}

/// A Markdown file to import as a page.
#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MarkdownFileDto {
    /// Where it sits in the Space's files ("notes/trip.md").
    pub path: String,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct PageDto {
    pub item_id: String,
    pub space_id: String,
    pub title: String,
    pub path: String,
    pub version: u32,
    /// False while a part of the page is still downloading.
    pub ready: bool,
    /// Edits on this device not saved as a version yet.
    pub unsaved: bool,
    pub input_pending: bool,
    pub input_revision: String,
    pub pending_sync: bool,
    pub save_error: Option<String>,
    /// Whether the current account may edit this page now.
    pub can_edit: bool,
    /// Semantic content hash, including block order and stable ids.
    pub content_hash: String,
    /// Opaque CRDT context for the exact blocks returned by this read.
    pub edit_context: String,
    pub blocks: Vec<PageBlockDto>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct PageEditResult {
    pub mutation_hash: String,
    pub mutation_id: String,
    pub applied_content_hash: String,
    pub replayed: bool,
    /// This editor's accepted state, before unseen edits merge into `page`.
    pub applied_edit_context: String,
    /// Current merged state; it may include changes accepted after this mutation.
    pub page: PageDto,
}

#[derive(Default)]
pub(crate) struct PageCache {
    pub(crate) sessions: std::sync::Mutex<HashMap<ItemId, PageSession>>,
    pub(crate) epoch: std::sync::Arc<()>,
}

fn invalid(pt: &str, en: &str) -> CoreError {
    CoreError::Invalid { reason: t(pt, en) }
}

fn doc_err(e: roda_docs::DocError) -> CoreError {
    CoreError::Invalid {
        reason: e.to_string(),
    }
}

pub(crate) fn block_dto(b: &roda_docs::Block) -> PageBlockDto {
    let mut d = PageBlockDto {
        id: b.id.clone(),
        kind: b.kind.id().into(),
        level: 0,
        indent: 0,
        number: 0,
        checked: false,
        lang: String::new(),
        url: String::new(),
        alt: String::new(),
        text: b.plain_text(),
        spans: roda_docs::spans_of(&b.runs)
            .into_iter()
            .map(|s| TextSpanDto {
                start: s.start,
                end: s.end,
                key: s.key,
                value: s.value,
            })
            .collect(),
    };
    match &b.kind {
        Kind::Heading { level, .. } => d.level = *level,
        Kind::Bullet { indent } => d.indent = *indent,
        Kind::Numbered { indent, number } => {
            d.indent = *indent;
            d.number = *number
        }
        Kind::Task { indent, checked } => {
            d.indent = *indent;
            d.checked = *checked
        }
        Kind::Code { lang } => d.lang = lang.clone(),
        Kind::Image { url, alt, .. } => {
            d.url = url.clone();
            d.alt = alt.clone()
        }
        _ => {}
    }
    d
}

fn kind_from_dto(d: &PageBlockDto, prev: Option<&Kind>) -> Kind {
    match d.kind.as_str() {
        "heading" => Kind::Heading {
            level: d.level.clamp(1, 6),
            setext: matches!(prev, Some(Kind::Heading { setext: true, level }) if *level == d.level),
        },
        "bullet" => Kind::Bullet { indent: d.indent },
        "numbered" => Kind::Numbered {
            indent: d.indent,
            number: d.number.max(1),
        },
        "task" => Kind::Task {
            indent: d.indent,
            checked: d.checked,
        },
        "quote" => Kind::Quote,
        "code" => Kind::Code {
            lang: d.lang.clone(),
        },
        "divider" => Kind::Divider,
        "image" => Kind::Image {
            url: d.url.clone(),
            alt: d.alt.clone(),
            title: match prev {
                Some(Kind::Image { title, .. }) => title.clone(),
                _ => String::new(),
            },
        },
        "raw" => Kind::Raw,
        _ => Kind::Paragraph,
    }
}

pub(crate) fn parse_editor_markdown(markdown: &str) -> R<Vec<PageBlockDto>> {
    Ok(Page::from_markdown(markdown)
        .map_err(doc_err)?
        .blocks()
        .iter()
        .map(block_dto)
        .collect())
}

pub(crate) fn editor_block_markdown(block: &PageBlockDto) -> R<String> {
    let page = Page::new();
    page.apply(
        std::slice::from_ref(&block.id),
        &[roda_docs::BlockEdit {
            id: block.id.clone(),
            kind: kind_from_dto(block, None),
            text: block.text.clone(),
            spans: block
                .spans
                .iter()
                .map(|span| roda_docs::Span {
                    start: span.start,
                    end: span.end,
                    key: span.key.clone(),
                    value: span.value.clone(),
                })
                .collect(),
        }],
    )
    .map_err(doc_err)?;
    Ok(page.canonical_markdown())
}

impl Engine {
    fn page_payload(&self, c: &PageContent) -> Option<Vec<u8>> {
        if let Some(b) = &c.blob {
            return self.media(&b.sha256).ok().flatten();
        }
        base64::engine::general_purpose::STANDARD
            .decode(&c.loro)
            .ok()
    }

    /// Brings the session for `item` up to date with the log; returns false if not a page.
    fn with_page<T>(
        &self,
        item: &str,
        f: impl FnOnce(&ItemState, &mut PageSession) -> R<T>,
    ) -> R<T> {
        self.with_document(item, ItemKind::Page, f)
    }

    fn with_document<T>(
        &self,
        item: &str,
        kind: ItemKind,
        f: impl FnOnce(&ItemState, &mut PageSession) -> R<T>,
    ) -> R<T> {
        let it = self.item_state(item)?;
        if it.kind != kind {
            return Err(invalid(
                "este Item não é uma página",
                "this Item is not a page",
            ));
        }
        let mut sessions = self
            .pages
            .sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let session = match sessions.entry(item.to_string()) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(PageSession::open(self, item)?)
            }
        };
        f(it, session)
    }

    pub(crate) fn plan_snapshot(&self, item: &str) -> R<plan::PlanSnapshot> {
        let it = self.item_state(item)?;
        if it.kind != ItemKind::Plan {
            return Err(invalid(
                "este Item não é um plano",
                "this Item is not a plan",
            ));
        }
        let has_creation = self.logs.get(&it.space).is_some_and(|log| {
            log.events().iter().any(|event| {
                matches!(&event.body, EventBody::ItemCreated { item: id, .. } if id == item)
            })
        });
        if !has_creation {
            if let ItemContent::Plan(document) = &it.current().content {
                return Ok(plan::PlanSnapshot::legacy(document, it.current()));
            }
        }
        self.with_document(item, ItemKind::Plan, |_, session| {
            session.plan_snapshot(self)
        })
    }

    pub(crate) fn require_legacy_plan(&self, item: &str) -> R<()> {
        let fold = self.plan_timeline(item)?;
        let pending_cutover = fold.slots.iter().any(|slot| {
            matches!(slot.acceptance, plan::Acceptance::Pending)
                && matches!(&slot.content, Some(ItemContent::Page(page)) if page.manifest.is_some())
        });
        if fold.frozen_at.is_some() || fold.waiting || pending_cutover {
            return Err(invalid(
                "este plano precisa do editor de página; a edição antiga não foi salva",
                "this Plan requires the page editor; the legacy edit was not saved",
            ));
        }
        Ok(())
    }

    pub(crate) fn plan_total(document: &PlanDoc) -> Option<i64> {
        plan::checked_total(document)
    }

    fn page_write_author(&self, item: &str) -> R<String> {
        let me = self.me_id()?;
        let it = self
            .state
            .items
            .get(item)
            .ok_or_else(|| CoreError::NotFound {
                what: "Item".into(),
            })?;
        if !self.page_writer(&it.space, &me) {
            return Err(CoreError::Forbidden {
                reason: t("só membros editam páginas", "only members can edit pages"),
            });
        }
        self.author_for(&me)?;
        Ok(me)
    }

    fn page_writer(&self, space: &str, actor: &str) -> bool {
        self.state.spaces.get(space).is_some_and(|s| {
            s.members
                .iter()
                .any(|(id, role)| id == actor && *role != Role::Reader)
        })
    }

    fn page_dto(&self, it: &ItemState, read: SessionRead) -> R<PageDto> {
        let DocumentView::BinaryPage { blocks } = read.view else {
            return Err(invalid(
                "este Item não é uma página",
                "this Item is not a page",
            ));
        };
        if read.tag != session::DocumentTag::BinaryPageV1 {
            return Err(invalid(
                "este Item não é uma página",
                "this Item is not a page",
            ));
        }
        let title = match it.versions.last().map(|v| &v.content) {
            Some(ItemContent::Page(content)) => content.title.clone(),
            _ => String::new(),
        };
        let path = match it.versions.first().map(|v| &v.content) {
            Some(ItemContent::Page(content)) => content.path.clone(),
            _ => String::new(),
        };
        Ok(PageDto {
            item_id: it.id.clone(),
            space_id: it.space.clone(),
            title,
            path,
            version: it.versions.len() as u32,
            ready: read.ready,
            unsaved: read.unsaved,
            input_pending: read.input_pending,
            input_revision: read.input_revision.to_string(),
            pending_sync: read.pending_sync,
            save_error: read.save_error,
            can_edit: read.can_edit,
            content_hash: read.content_hash,
            edit_context: read.edit_context,
            blocks: blocks.iter().map(block_dto).collect(),
        })
    }

    fn page_content(
        &mut self,
        space: &str,
        title: String,
        path: &str,
        bytes: Vec<u8>,
    ) -> R<PageContent> {
        let (loro, blob) = if bytes.len() > INLINE_MAX {
            (String::new(), Some(self.page_blob(space, &bytes)?))
        } else {
            (
                base64::engine::general_purpose::STANDARD.encode(&bytes),
                None,
            )
        };
        Ok(PageContent {
            title,
            path: path.to_string(),
            loro,
            blob,
            manifest: None,
        })
    }

    fn new_page(&mut self, space: &str, path: &str, page: Page, origin: &str) -> R<String> {
        let me = self.me_id()?;
        if !self.page_writer(space, &me) {
            return Err(CoreError::Forbidden {
                reason: t("só membros criam páginas", "only members can create pages"),
            });
        }
        let mut title = page.title();
        if title.is_empty() {
            title = path
                .rsplit('/')
                .next()
                .unwrap_or("")
                .trim_end_matches(".md")
                .to_string();
        }
        if title.is_empty() {
            title = t("Sem título", "Untitled");
        }
        let content = self.page_content(space, title, path, page.snapshot())?;
        let item = new_id("it");
        self.append(
            space,
            &me,
            EventBody::ItemCreated {
                item: item.clone(),
                kind: ItemKind::Page,
                content: ItemContent::Page(content),
                origin: origin.into(),
            },
        )?;
        self.index_dirty = true;
        Ok(item)
    }

    /// A page made from a Markdown file; exporting it untouched gives the same bytes back.
    pub fn page_import_markdown(&mut self, space: &str, path: &str, markdown: &str) -> R<String> {
        let page = Page::from_markdown(markdown).map_err(doc_err)?;
        self.new_page(space, path, page, &t("Importado", "Imported"))
    }

    /// An empty page with a title.
    pub fn page_create(&mut self, space: &str, title: &str) -> R<String> {
        let md = if title.trim().is_empty() {
            String::new()
        } else {
            format!("# {}\n", title.trim())
        };
        let page = Page::from_markdown(&md).map_err(doc_err)?;
        let order: Vec<String> = page.blocks().iter().map(|b| b.id.clone()).collect();
        // New pages have no source to preserve: drop it so the first edit isn't special.
        let fresh = Page::new();
        let mut edits = Vec::new();
        for b in page.blocks() {
            edits.push(BlockEdit {
                id: b.id.clone(),
                kind: b.kind.clone(),
                text: b.plain_text(),
                spans: vec![],
            });
        }
        let mut order = order;
        let extra = format!("{:016x}", rand_u64());
        order.push(extra.clone());
        edits.push(BlockEdit {
            id: extra,
            kind: Kind::Paragraph,
            text: String::new(),
            spans: vec![],
        });
        fresh.apply(&order, &edits).map_err(doc_err)?;
        self.new_page(space, "", fresh, &t("Criado", "Created"))
    }

    pub fn page(&self, item: &str) -> R<PageDto> {
        self.with_page(item, |it, session| self.page_dto(it, session.read(self)?))
    }

    /// The page as it read at `version` (for looking back before restoring).
    pub fn page_at(&self, item: &str, version: u32) -> R<PageDto> {
        self.with_page(item, |it, session| {
            let read = session.read(self)?;
            let old = session.binary_at(version)?;
            let mut dto = self.page_dto(it, read)?;
            dto.version = version;
            dto.unsaved = false;
            dto.can_edit = false;
            dto.content_hash = old.content_hash();
            dto.edit_context = String::new();
            dto.pending_sync = false;
            dto.save_error = None;
            if let Some(ItemContent::Page(content)) = it
                .versions
                .get((version as usize).saturating_sub(1))
                .map(|v| &v.content)
            {
                dto.title = content.title.clone();
            }
            dto.blocks = old.blocks().iter().map(block_dto).collect();
            Ok(dto)
        })
    }

    /// Applies an editor's state to this device's copy (not saved as a version yet).
    pub fn page_apply_from(
        &self,
        item: &str,
        mutation_id: &str,
        context: &str,
        order: &[String],
        changed: &[PageBlockDto],
    ) -> R<PageEditResult> {
        self.with_page(item, |it, session| {
            let accepted = session.apply_from(
                self,
                mutation_id,
                context,
                DocumentEdit::BinaryPage { order, changed },
            )?;
            Ok(PageEditResult {
                mutation_hash: accepted.request_hash,
                mutation_id: accepted.mutation_id,
                applied_content_hash: accepted.applied_content_hash,
                replayed: accepted.replayed,
                applied_edit_context: accepted.applied_edit_context,
                page: self.page_dto(it, accepted.current)?,
            })
        })
    }

    /// Queues one new version; an unchanged or already pending page returns false.
    pub fn page_commit(&mut self, item: &str, note: &str) -> R<bool> {
        self.page_write_author(item)?;
        if self.item_state(item)?.kind != ItemKind::Page {
            return Err(invalid(
                "este Item não é uma página",
                "this Item is not a page",
            ));
        }
        // Detach the one owner before append: an actual storage rollback calls reload,
        // which must be able to clear the cache without holding its mutex here.
        let cached = self
            .pages
            .sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(item);
        let mut session = match cached {
            Some(session) => session,
            None => PageSession::open(self, item)?,
        };
        let note = if note.is_empty() {
            t("Editou", "Edited")
        } else {
            note.into()
        };
        let result = session.commit(self, &note);
        self.pages
            .sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(item.into(), session);
        let queued = matches!(result?, CommitStatus::Queued(_));
        if queued {
            self.index_dirty = true;
        }
        Ok(queued)
    }

    /// Makes the page read as it did at version, as a new version.
    pub(crate) fn page_restore(&mut self, item: &str, version: u32, note: &str) -> R<()> {
        self.with_page(item, |_, session| session.restore_binary(self, version))?;
        self.page_commit(item, note)?;
        Ok(())
    }

    pub fn page_markdown(&self, item: &str) -> R<String> {
        self.with_page(item, |_, session| session.binary_markdown(self))
    }

    pub(crate) fn page_text(&self, item: &str) -> String {
        self.with_page(item, |_, session| session.binary_text(self))
            .unwrap_or_default()
    }
}

fn rand_u64() -> u64 {
    let mut b = [0u8; 8];
    getrandom::getrandom(&mut b).expect("system randomness");
    u64::from_le_bytes(b)
}

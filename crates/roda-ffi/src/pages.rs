//! Pages (ADR 0040): a Loro document per page, its history in the Space log.
//!
//! Version 1 of a page carries a snapshot; every later version carries the Loro updates
//! made since the previous one. Payloads over 48 KB go to an encrypted blob instead of the
//! event. Each device keeps two copies of a page in memory: `shadow`, exactly the versions
//! in the log, and `live`, the shadow plus this person's edits not yet saved. Saving
//! exports `live` minus `shadow` as the next version.

use std::collections::HashMap;

use base64::Engine as _;
use roda_docs::{BlockEdit, Kind, Page, Span};
use roda_types::*;

use crate::engine::{Engine, ItemState, R};
use crate::i18n::t;
use crate::CoreError;

mod drafts;
#[cfg(test)]
mod tests;

/// Largest payload that rides inside the event (base64 adds a third; envelopes cap at 90 KB).
const INLINE_MAX: usize = 48 * 1024;

#[derive(Debug, Clone, PartialEq, serde::Serialize, uniffi::Record)]
pub struct TextSpanDto {
    /// UTF-16 offsets into the block's text.
    pub start: u32,
    pub end: u32,
    /// "b" bold, "i" italic, "s" strike, "c" code, "a" link (value = URL), "hb" hard break.
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, uniffi::Record)]
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
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MarkdownFileDto {
    /// Where it sits in the Space's files ("notes/trip.md").
    pub path: String,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
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

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
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

pub(crate) struct PageSession {
    shadow: Page,
    live: Page,
    applied: usize,
    /// Shadow's frontiers after each applied version (index 0 = version 1).
    frontiers: Vec<roda_docs::Frontiers>,
    blocked: bool,
    dirty: bool,
    owner: Option<String>,
    draft_loaded: bool,
    journal: drafts::Journal,
    draft_error: Option<String>,
    /// Plain text for lists and search, until the page changes.
    text: Option<String>,
}

#[derive(Default)]
pub(crate) struct PageCache {
    pub(crate) sessions: std::sync::Mutex<HashMap<ItemId, PageSession>>,
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
    fn page_sync(&self, it: &ItemState, s: &mut PageSession) -> R<()> {
        while s.applied < it.versions.len() {
            let ItemContent::Page(c) = &it.versions[s.applied].content else {
                // An old undo copied another version's payload: nothing new to import.
                s.frontiers.push(s.shadow.frontiers());
                s.applied += 1;
                continue;
            };
            let Some(bytes) = self.page_payload(c) else {
                s.blocked = true;
                return Ok(());
            };
            let shadow = s.shadow.at(&s.shadow.frontiers()).map_err(doc_err)?;
            let live = s.live.at(&s.live.frontiers()).map_err(doc_err)?;
            if shadow.import(&bytes).is_err() || live.import(&bytes).is_err() {
                s.blocked = true;
                return Ok(());
            }
            s.shadow = shadow;
            s.live = live;
            s.text = None;
            s.frontiers.push(s.shadow.frontiers());
            s.applied += 1;
        }
        s.blocked = false;
        s.dirty = s.live.content_hash() != s.shadow.content_hash();
        Ok(())
    }

    fn with_page<T>(
        &self,
        item: &str,
        f: impl FnOnce(&ItemState, &mut PageSession) -> R<T>,
    ) -> R<T> {
        let it = self
            .state
            .items
            .get(item)
            .ok_or_else(|| CoreError::NotFound {
                what: "Item".into(),
            })?;
        if it.kind != ItemKind::Page {
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
        let s = sessions
            .entry(item.to_string())
            .or_insert_with(|| PageSession {
                shadow: Page::new(),
                live: Page::new(),
                applied: 0,
                frontiers: Vec::new(),
                blocked: false,
                dirty: false,
                owner: self.me.clone(),
                draft_loaded: false,
                journal: Default::default(),
                draft_error: None,
                text: None,
            });
        if s.owner != self.me {
            *s = PageSession {
                shadow: Page::new(),
                live: Page::new(),
                applied: 0,
                frontiers: Vec::new(),
                blocked: false,
                dirty: false,
                owner: self.me.clone(),
                draft_loaded: false,
                journal: Default::default(),
                draft_error: None,
                text: None,
            };
        }
        self.page_sync(it, s)?;
        if !s.draft_loaded {
            match self.load_page_journal(it) {
                Ok(journal) => {
                    let restored = if let Some(encoded) = &journal.snapshot {
                        s.live
                            .at(&s.live.frontiers())
                            .map_err(doc_err)
                            .and_then(|live| {
                                let bytes = base64::engine::general_purpose::STANDARD
                                    .decode(encoded)
                                    .map_err(|_| invalid("rascunho inválido", "invalid draft"))?;
                                live.import(&bytes).map_err(doc_err)?;
                                Ok(live)
                            })
                    } else {
                        s.live.at(&s.live.frontiers()).map_err(doc_err)
                    };
                    match restored {
                        Ok(live) => {
                            s.live = live;
                            s.journal = journal;
                            s.dirty = s.live.content_hash() != s.shadow.content_hash();
                            s.text = None;
                            s.draft_error = None;
                            s.draft_loaded = true;
                        }
                        Err(_) => s.draft_error = Some(t(
                            "não foi possível recuperar o rascunho; a versão salva continua disponível",
                            "could not recover the draft; the saved version is still available",
                        )),
                    }
                }
                Err(_) => {
                    s.draft_error = Some(t(
                        "o rascunho está bloqueado; a versão salva continua disponível",
                        "the draft is locked; the saved version is still available",
                    ))
                }
            }
        }
        if s.draft_loaded && !s.blocked && !s.dirty && s.journal.has_draft {
            let acknowledged = s.journal.commit.as_ref().is_some_and(|id| {
                self.logs
                    .get(&it.space)
                    .is_some_and(|log| log.events().iter().any(|event| &event.client_id == id))
            });
            let outstanding = self.page_outbox_status(it, s)?.0;
            if acknowledged
                || (s.journal.commit.is_none()
                    && !outstanding
                    && self.page_failed_commit(it, s)?.is_none())
            {
                let mut journal = s.journal.clone();
                journal.has_draft = false;
                journal.commit = None;
                if self.save_page_journal(it, &journal).is_ok() {
                    s.journal = journal;
                }
            }
        }
        f(it, s)
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

    fn page_outbox_status(&self, it: &ItemState, s: &PageSession) -> R<(bool, Option<String>)> {
        let pending = self.store.outbox()?;
        let matches_page = |p: &&roda_store::Pending| match &p.event.body {
            EventBody::ItemVersioned { item, .. } | EventBody::ItemCreated { item, .. } => {
                item == &it.id
            }
            _ => false,
        };
        let queue: Vec<_> = pending.iter().filter(matches_page).collect();
        let pending_sync = queue.iter().any(|p| !p.failed);
        let error = if !s.journal.has_draft {
            None
        } else {
            queue
                .iter()
                .rev()
                .find(|p| {
                    p.failed
                        && s.journal
                            .commit
                            .as_ref()
                            .is_none_or(|id| id == &p.event.client_id)
                })
                .map(|p| {
                    p.last_error
                        .clone()
                        .unwrap_or_else(|| t("não foi possível salvar", "could not save"))
                })
        };
        Ok((pending_sync, error))
    }

    fn page_failed_commit(&self, it: &ItemState, s: &PageSession) -> R<Option<String>> {
        Ok(self.page_outbox_status(it, s)?.1)
    }

    fn page_dto(&self, it: &ItemState, s: &PageSession) -> R<PageDto> {
        let title = match it.versions.last().map(|v| &v.content) {
            Some(ItemContent::Page(c)) => c.title.clone(),
            _ => String::new(),
        };
        let path = match it.versions.first().map(|v| &v.content) {
            Some(ItemContent::Page(c)) => c.path.clone(),
            _ => String::new(),
        };
        let can_edit =
            self.page_write_author(&it.id).is_ok() && !s.blocked && s.draft_error.is_none();
        let (pending_sync, failed) = self.page_outbox_status(it, s)?;
        Ok(PageDto {
            item_id: it.id.clone(),
            space_id: it.space.clone(),
            title,
            path,
            version: it.versions.len() as u32,
            ready: !s.blocked,
            unsaved: s.dirty || s.draft_error.is_some(),
            pending_sync,
            save_error: s.draft_error.clone().or(failed),
            can_edit,
            content_hash: s.live.content_hash(),
            edit_context: if can_edit {
                self.page_edit_context(it, s)?
            } else {
                String::new()
            },
            blocks: s.live.blocks().iter().map(block_dto).collect(),
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
        self.with_page(item, |it, s| self.page_dto(it, s))
    }

    /// The page as it read at `version` (for looking back before restoring).
    pub fn page_at(&self, item: &str, version: u32) -> R<PageDto> {
        if version == 0 {
            return Err(invalid("versão não encontrada", "version not found"));
        }
        let mut dto = self.page(item)?;
        self.with_page(item, |it, s| {
            let Some(f) = s.frontiers.get(version.saturating_sub(1) as usize).cloned() else {
                return Err(invalid("versão não encontrada", "version not found"));
            };
            let old = s.shadow.at(&f).map_err(doc_err)?;
            dto.version = version;
            dto.unsaved = false;
            dto.can_edit = false;
            dto.content_hash = old.content_hash();
            dto.edit_context = String::new();
            dto.pending_sync = false;
            dto.save_error = None;
            if let Some(ItemContent::Page(c)) = it
                .versions
                .get((version as usize).saturating_sub(1))
                .map(|v| &v.content)
            {
                dto.title = c.title.clone();
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
        self.page_write_author(item)?;
        if mutation_id.is_empty()
            || mutation_id.len() > 128
            || !mutation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(invalid(
                "identificador de edição inválido",
                "invalid edit identifier",
            ));
        }
        if context.len() > 2 * 1024 * 1024 {
            return Err(invalid(
                "contexto de edição inválido",
                "invalid editing context",
            ));
        }
        self.with_page(item, |it, s| {
            if s.blocked || s.draft_error.is_some() {
                return Err(invalid(
                    "a página ainda está chegando",
                    "the page is still arriving",
                ));
            }
            let decoded = self.decode_page_edit_context(it, context)?;
            let fingerprint = drafts::request_hash(mutation_id, context, order, changed)?;
            if let Some(receipt) = s.journal.receipt(mutation_id) {
                if receipt.fingerprint != fingerprint {
                    return Err(CoreError::Stale {
                        reason: t(
                            "este identificador já foi usado por outra edição",
                            "this identifier was already used for another edit",
                        ),
                    });
                }
                return Ok(PageEditResult {
                    mutation_id: mutation_id.into(),
                    mutation_hash: fingerprint,
                    applied_content_hash: receipt.content_hash.clone(),
                    replayed: true,
                    applied_edit_context: self.page_context_at(
                        it,
                        receipt.accepted_generation,
                        receipt.frontiers.clone(),
                    )?,
                    page: self.page_dto(it, s)?,
                });
            }
            if decoded.generation < s.journal.minimum_generation {
                return Err(CoreError::Stale {
                    reason: t(
                        "esta edição ficou antiga; releia a página antes de tentar novamente",
                        "this edit is too old; read the page again before retrying",
                    ),
                });
            }
            if decoded.generation > s.journal.generation {
                return Err(invalid(
                    "contexto de edição inválido",
                    "invalid editing context",
                ));
            }
            let frontiers = roda_docs::Frontiers::decode(&decoded.frontiers)
                .map_err(|_| invalid("contexto de edição inválido", "invalid editing context"))?;
            let desired = s.live.at(&frontiers).map_err(doc_err)?;
            let current: HashMap<String, Kind> = desired
                .blocks()
                .into_iter()
                .map(|b| (b.id, b.kind))
                .collect();
            let edits: Vec<BlockEdit> = changed
                .iter()
                .map(|d| BlockEdit {
                    id: d.id.clone(),
                    kind: kind_from_dto(d, current.get(&d.id)),
                    text: d.text.clone(),
                    spans: d
                        .spans
                        .iter()
                        .map(|x| Span {
                            start: x.start,
                            end: x.end,
                            key: x.key.clone(),
                            value: x.value.clone(),
                        })
                        .collect(),
                })
                .collect();
            let context_hash = drafts::context_hash(context);
            let (observed, base) = if let Some(prior) = s.journal.continuation(&context_hash) {
                // A caller can type again before receiving an earlier save's result.
                // Continue its own accepted branch; don't generate concurrent copies
                // of text it already inserted. Fill unchanged blocks from its read.
                desired.apply(order, &edits).map_err(doc_err)?;
                let at = roda_docs::Frontiers::decode(&prior.frontiers)
                    .map_err(|_| invalid("rascunho inválido", "invalid draft"))?;
                let observed = s.live.at(&at).map_err(doc_err)?;
                let base = observed.version();
                let complete: Vec<_> = desired
                    .blocks()
                    .into_iter()
                    .map(|b| BlockEdit {
                        id: b.id.clone(),
                        kind: b.kind.clone(),
                        text: b.plain_text(),
                        spans: roda_docs::spans_of(&b.runs),
                    })
                    .collect();
                observed.apply(order, &complete).map_err(doc_err)?;
                (observed, base)
            } else {
                let base = desired.version();
                desired.apply(order, &edits).map_err(doc_err)?;
                (desired, base)
            };
            let updates = observed.updates_since(&base);
            let live = s.live.at(&s.live.frontiers()).map_err(doc_err)?;
            if !updates.is_empty() {
                live.import(&updates).map_err(doc_err)?;
            }
            let hash = observed.content_hash();
            let mut journal = s.journal.clone();
            let applied_frontiers = observed.frontiers().encode();
            journal.accept(
                mutation_id.into(),
                fingerprint.clone(),
                context_hash,
                hash.clone(),
                decoded.generation,
                applied_frontiers.clone(),
            )?;
            journal.snapshot =
                Some(base64::engine::general_purpose::STANDARD.encode(live.snapshot()));
            journal.has_draft = true;
            journal.commit = None;
            self.save_page_journal(it, &journal)?;
            s.journal = journal;
            s.live = live;
            s.dirty = s.live.content_hash() != s.shadow.content_hash();
            s.text = None;
            Ok(PageEditResult {
                mutation_id: mutation_id.into(),
                mutation_hash: fingerprint,
                applied_content_hash: hash,
                replayed: false,
                applied_edit_context: self.page_context_at(
                    it,
                    s.journal.generation,
                    applied_frontiers,
                )?,
                page: self.page_dto(it, s)?,
            })
        })
    }

    /// Saves this device's unsaved edits as the next version. Returns false if nothing
    /// changed.
    pub fn page_commit(&mut self, item: &str, note: &str) -> R<bool> {
        let me = self.page_write_author(item)?;
        let pending = self.with_page(item, |it, s| {
            if s.blocked || s.draft_error.is_some() {
                return Err(invalid(
                    "a página ainda está chegando",
                    "the page is still arriving",
                ));
            }
            if s.live.content_hash() == s.shadow.content_hash() {
                s.dirty = false;
                return Ok(None);
            }
            // A later delta must not depend on an optimistic version that the relay
            // can still refuse. The journal keeps newer edits while this save resolves.
            if self.page_outbox_status(it, s)?.0 {
                return Ok(None);
            }
            let bytes = s.live.updates_since(&s.shadow.version());
            let path = match &it.versions.first().map(|v| &v.content) {
                Some(ItemContent::Page(c)) => c.path.clone(),
                _ => String::new(),
            };
            Ok(Some((bytes, s.live.title(), it.space.clone(), path)))
        })?;
        let Some((bytes, title, space, path)) = pending else {
            return Ok(false);
        };
        let title = if title.is_empty() {
            t("Sem título", "Untitled")
        } else {
            title
        };
        if bytes.is_empty() {
            return Err(CoreError::Storage {
                reason: t(
                    "não foi possível salvar a página",
                    "could not save the page",
                ),
            });
        }
        let content = self.page_content(&space, title, &path, bytes)?;
        let note = if note.is_empty() {
            t("Editou", "Edited")
        } else {
            note.to_string()
        };
        let event = self.append(
            &space,
            &me,
            EventBody::ItemVersioned {
                item: item.into(),
                content: ItemContent::Page(content),
                note,
            },
        )?;
        self.with_page(item, |it, s| {
            let mut journal = s.journal.clone();
            journal.commit = Some(event.client_id.clone());
            self.save_page_journal(it, &journal)?;
            s.journal = journal;
            s.dirty = false;
            Ok(())
        })?;
        self.index_dirty = true;
        Ok(true)
    }

    /// Makes the page read as it did at `version`, as a new version (nothing is lost).
    pub(crate) fn page_restore(&mut self, item: &str, version: u32, note: &str) -> R<()> {
        self.page_write_author(item)?;
        if version == 0 {
            return Err(invalid("versão não encontrada", "version not found"));
        }
        self.with_page(item, |it, s| {
            if s.blocked || s.draft_error.is_some() {
                return Err(invalid(
                    "a página ainda está chegando",
                    "the page is still arriving",
                ));
            }
            let Some(f) = s.frontiers.get(version.saturating_sub(1) as usize).cloned() else {
                return Err(invalid("versão não encontrada", "version not found"));
            };
            let live = s.live.at(&s.live.frontiers()).map_err(doc_err)?;
            live.revert_to(&f).map_err(doc_err)?;
            let mut journal = s.journal.clone();
            // Restoring intentionally replaces local drafts. An older native draft
            // that failed to clear from secure storage must not silently undo it.
            journal.invalidate_contexts()?;
            journal.snapshot =
                Some(base64::engine::general_purpose::STANDARD.encode(live.snapshot()));
            journal.has_draft = true;
            journal.commit = None;
            self.save_page_journal(it, &journal)?;
            s.journal = journal;
            s.live = live;
            s.dirty = s.live.content_hash() != s.shadow.content_hash();
            s.text = None;
            Ok(())
        })?;
        self.page_commit(item, note)?;
        Ok(())
    }

    pub fn page_markdown(&self, item: &str) -> R<String> {
        self.with_page(item, |_, s| {
            if s.blocked {
                return Err(invalid(
                    "a página ainda está chegando",
                    "the page is still arriving",
                ));
            }
            Ok(s.live.to_markdown())
        })
    }

    /// The page's text for search and summaries ("" while it's arriving).
    pub(crate) fn page_text(&self, item: &str) -> String {
        self.with_page(item, |_, s| {
            if let Some(t) = &s.text {
                return Ok(t.clone());
            }
            let t = s
                .live
                .blocks()
                .iter()
                .map(|b| b.plain_text().replace('\u{2028}', " "))
                .collect::<Vec<_>>()
                .join("\n");
            if !s.blocked {
                s.text = Some(t.clone());
            }
            Ok(t)
        })
        .unwrap_or_default()
    }
}

fn rand_u64() -> u64 {
    let mut b = [0u8; 8];
    getrandom::getrandom(&mut b).expect("system randomness");
    u64::from_le_bytes(b)
}

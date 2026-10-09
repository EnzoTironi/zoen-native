//! Pages (ADR 0023): a Loro document per page, its history in the Space log.
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

/// Largest payload that rides inside the event (base64 adds a third; envelopes cap at 90 KB).
const INLINE_MAX: usize = 48 * 1024;

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TextSpanDto {
    /// UTF-16 offsets into the block's text.
    pub start: u32,
    pub end: u32,
    /// "b" bold, "i" italic, "s" strike, "c" code, "a" link (value = URL), "hb" hard break.
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
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
    pub blocks: Vec<PageBlockDto>,
}

pub(crate) struct PageSession {
    shadow: Page,
    live: Page,
    applied: usize,
    /// Shadow's frontiers after each applied version (index 0 = version 1).
    frontiers: Vec<roda_docs::Frontiers>,
    blocked: bool,
    dirty: bool,
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
    fn page_sync(&self, it: &ItemState, s: &mut PageSession) {
        while s.applied < it.versions.len() {
            let ItemContent::Page(c) = &it.versions[s.applied].content else {
                // An old undo copied another version's payload: nothing new to import.
                s.frontiers.push(s.shadow.frontiers());
                s.applied += 1;
                continue;
            };
            let Some(bytes) = self.page_payload(c) else {
                s.blocked = true;
                return;
            };
            let _ = s.shadow.import(&bytes);
            let _ = s.live.import(&bytes);
            s.frontiers.push(s.shadow.frontiers());
            s.applied += 1;
        }
        s.blocked = false;
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
            });
        self.page_sync(it, s);
        f(it, s)
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
        if !self.is_member(space, &me) {
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
        self.with_page(item, |it, s| {
            let title = match &it.versions.last().map(|v| &v.content) {
                Some(ItemContent::Page(c)) => c.title.clone(),
                _ => String::new(),
            };
            let path = match &it.versions.first().map(|v| &v.content) {
                Some(ItemContent::Page(c)) => c.path.clone(),
                _ => String::new(),
            };
            Ok(PageDto {
                item_id: it.id.clone(),
                space_id: it.space.clone(),
                title,
                path,
                version: it.versions.len() as u32,
                ready: !s.blocked,
                unsaved: s.dirty,
                blocks: s.live.blocks().iter().map(block_dto).collect(),
            })
        })
    }

    /// The page as it read at `version` (for looking back before restoring).
    pub fn page_at(&self, item: &str, version: u32) -> R<PageDto> {
        let mut dto = self.page(item)?;
        self.with_page(item, |it, s| {
            let Some(f) = s.frontiers.get(version.saturating_sub(1) as usize).cloned() else {
                return Err(invalid("versão não encontrada", "version not found"));
            };
            let old = s.shadow.at(&f).map_err(doc_err)?;
            dto.version = version;
            dto.unsaved = false;
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
    pub fn page_apply(&self, item: &str, order: &[String], changed: &[PageBlockDto]) -> R<()> {
        self.with_page(item, |_, s| {
            if s.blocked {
                return Err(invalid(
                    "a página ainda está chegando",
                    "the page is still arriving",
                ));
            }
            let current: HashMap<String, Kind> = s
                .live
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
            s.live.apply(order, &edits).map_err(doc_err)?;
            s.dirty = true;
            Ok(())
        })
    }

    /// Saves this device's unsaved edits as the next version. Returns false if nothing
    /// changed.
    pub fn page_commit(&mut self, item: &str, note: &str) -> R<bool> {
        let me = self.me_id()?;
        let pending = self.with_page(item, |it, s| {
            let bytes = s.live.updates_since(&s.shadow.version());
            s.dirty = false;
            let path = match &it.versions.first().map(|v| &v.content) {
                Some(ItemContent::Page(c)) => c.path.clone(),
                _ => String::new(),
            };
            Ok((bytes, s.live.title(), it.space.clone(), path))
        })?;
        let (bytes, title, space, path) = pending;
        if bytes.is_empty() {
            return Ok(false);
        }
        let title = if title.is_empty() {
            t("Sem título", "Untitled")
        } else {
            title
        };
        let content = self.page_content(&space, title, &path, bytes)?;
        let note = if note.is_empty() {
            t("Editou", "Edited")
        } else {
            note.to_string()
        };
        self.append(
            &space,
            &me,
            EventBody::ItemVersioned {
                item: item.into(),
                content: ItemContent::Page(content),
                note,
            },
        )?;
        self.index_dirty = true;
        Ok(true)
    }

    /// Makes the page read as it did at `version`, as a new version (nothing is lost).
    pub(crate) fn page_restore(&mut self, item: &str, version: u32, note: &str) -> R<()> {
        self.with_page(item, |_, s| {
            let Some(f) = s.frontiers.get(version.saturating_sub(1) as usize).cloned() else {
                return Err(invalid("versão não encontrada", "version not found"));
            };
            s.live.revert_to(&f).map_err(doc_err)?;
            s.dirty = true;
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
            Ok(s.live
                .blocks()
                .iter()
                .map(|b| b.plain_text().replace('\u{2028}', " "))
                .collect::<Vec<_>>()
                .join("\n"))
        })
        .unwrap_or_default()
    }
}

fn rand_u64() -> u64 {
    let mut b = [0u8; 8];
    getrandom::getrandom(&mut b).expect("system randomness");
    u64::from_le_bytes(b)
}

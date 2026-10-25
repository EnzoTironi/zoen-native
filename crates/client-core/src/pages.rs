//! Pages (ADR 0040): a Loro document per page, its history in the Space log.
//!
//! Version 1 of a page carries a snapshot; every later version carries the Loro updates
//! made since the previous one. Payloads over 48 KB go to an encrypted blob instead of the
//! event. Each device keeps two copies of a page in memory: `shadow`, exactly the versions
//! in the log, and `live`, the shadow plus this person's edits not yet saved. Saving
//! exports `live` minus `shadow` as the next version.

#[cfg(test)]
use types::{
    ActionClass, AgentRequest, ChunkRef, Identity, IdentityKind, PageManifest, PlanLine,
    PlanSection, Privacy, RequestResolution, Seen, SpaceKind, GENESIS_PREV,
};

use std::collections::HashMap;

use base64::Engine as _;
use documents::{BlockEdit, Kind, Page};
use types::{new_id, EventBody, ItemContent, ItemId, ItemKind, PageContent, PlanDoc, Role};

use crate::engine::{Engine, ItemState, R};
use crate::i18n::t;
use crate::CoreError;

mod drafts;
mod exports;
#[cfg(any(test, feature = "client-types"))]
pub(crate) use exports::PageVersionExport;
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
#[cfg(test)]
mod catchup_projection_tests;
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
    /// Heading syntax. Absent only in legacy editor requests; false is explicit ATX.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    #[uniffi(default = None)]
    pub setext: Option<bool>,
    /// Image title. Absent legacy requests preserve the existing canonical title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    #[uniffi(default = None)]
    pub title: Option<String>,
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

fn doc_err(e: documents::DocError) -> CoreError {
    CoreError::Invalid {
        reason: e.to_string(),
    }
}

pub(crate) fn block_dto(b: &documents::Block) -> PageBlockDto {
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
        setext: None,
        title: None,
        text: b.plain_text(),
        spans: documents::spans_of(&b.runs)
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
        Kind::Heading { level, setext } => {
            d.level = *level;
            d.setext = Some(*setext);
        }
        Kind::Bullet { indent } => d.indent = *indent,
        Kind::Numbered { indent, number } => {
            d.indent = *indent;
            d.number = *number;
        }
        Kind::Task { indent, checked } => {
            d.indent = *indent;
            d.checked = *checked;
        }
        Kind::Code { lang } => d.lang = lang.clone(),
        Kind::Image { url, alt, title } => {
            d.url = url.clone();
            d.alt = alt.clone();
            d.title = Some(title.clone());
        }
        _ => {}
    }
    d
}

fn kind_from_dto(d: &PageBlockDto, prev: Option<&Kind>) -> R<Kind> {
    if d.kind == "heading"
        && d.setext.is_some()
        && (!(1..=6).contains(&d.level) || (d.setext == Some(true) && d.level > 2))
    {
        return Err(CoreError::Invalid {
            reason: "invalid heading syntax or level".into(),
        });
    }
    Ok(match d.kind.as_str() {
        "paragraph" => Kind::Paragraph,
        "heading" => Kind::Heading {
            level: d.level.clamp(1, 6),
            setext: d.setext.unwrap_or(
                matches!(prev, Some(Kind::Heading { setext: true, level }) if *level == d.level),
            ),
        },
        "bullet" => Kind::Bullet { indent: d.indent },
        "numbered" => Kind::Numbered {
            indent: d.indent,
            number: d.number,
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
            title: d.title.clone().unwrap_or_else(|| match prev {
                Some(Kind::Image { title, .. }) => title.clone(),
                _ => String::new(),
            }),
        },
        "raw" => Kind::Raw,
        _ => {
            return Err(CoreError::Invalid {
                reason: "unsupported page block kind".into(),
            })
        }
    })
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
    editor_blocks_markdown(std::slice::from_ref(block))
}

pub(crate) fn editor_blocks_markdown(blocks: &[PageBlockDto]) -> R<String> {
    let page = Page::new();
    page.apply_editor(
        &blocks
            .iter()
            .map(|block| block.id.clone())
            .collect::<Vec<_>>(),
        &blocks
            .iter()
            .map(|block| {
                Ok(documents::BlockEdit {
                    id: block.id.clone(),
                    kind: kind_from_dto(block, None)?,
                    text: block.text.clone(),
                    spans: block
                        .spans
                        .iter()
                        .map(|span| documents::Span {
                            start: span.start,
                            end: span.end,
                            key: span.key.clone(),
                            value: span.value.clone(),
                        })
                        .collect(),
                })
            })
            .collect::<R<Vec<_>>>()?,
    )
    .map_err(doc_err)?;
    page.canonical_markdown().map_err(doc_err)
}

fn rand_u64() -> R<u64> {
    rand_u64_with(getrandom::getrandom)
}

fn rand_u64_with(fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>) -> R<u64> {
    let mut b = [0u8; 8];
    fill(&mut b).map_err(|error| CoreError::Storage {
        message: error.to_string(),
    })?;
    Ok(u64::from_le_bytes(b))
}

mod authoring;
mod plans;
mod publication;
mod reading;

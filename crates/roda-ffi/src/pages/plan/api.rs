//! Lossless client reads of the shared Plan session. Write admission stays closed.

use roda_docs::plan::{PlanBlock, PlanBlockContent};
use roda_docs::Kind;
use roda_types::ItemKind;

use crate::engine::{Engine, R};
use crate::pages::session::{DocumentTag, DocumentView};
use crate::pages::{invalid, TextSpanDto};

#[derive(Debug, Clone, PartialEq, serde::Serialize, uniffi::Enum)]
pub enum PlanProseKindDto {
    Paragraph,
    Heading {
        level: u32,
        setext: bool,
    },
    Bullet {
        indent: u32,
    },
    Numbered {
        indent: u32,
        number: u32,
    },
    Task {
        indent: u32,
        checked: bool,
    },
    Quote,
    Code {
        lang: String,
    },
    Divider,
    Image {
        url: String,
        alt: String,
        title: String,
    },
    Raw,
}

impl From<&Kind> for PlanProseKindDto {
    fn from(kind: &Kind) -> Self {
        match kind {
            Kind::Paragraph => Self::Paragraph,
            Kind::Heading { level, setext } => Self::Heading {
                level: *level,
                setext: *setext,
            },
            Kind::Bullet { indent } => Self::Bullet { indent: *indent },
            Kind::Numbered { indent, number } => Self::Numbered {
                indent: *indent,
                number: *number,
            },
            Kind::Task { indent, checked } => Self::Task {
                indent: *indent,
                checked: *checked,
            },
            Kind::Quote => Self::Quote,
            Kind::Code { lang } => Self::Code { lang: lang.clone() },
            Kind::Divider => Self::Divider,
            Kind::Image { url, alt, title } => Self::Image {
                url: url.clone(),
                alt: alt.clone(),
                title: title.clone(),
            },
            Kind::Raw => Self::Raw,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, uniffi::Enum)]
pub enum PlanPageContentDto {
    Title {
        budget_cents: Option<i64>,
    },
    Summary,
    Section,
    Line {
        legacy_id: String,
        cost_cents: i64,
        done: bool,
    },
    Prose {
        kind: PlanProseKindDto,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, uniffi::Record)]
pub struct PlanPageBlockDto {
    /// CRDT block identity; it is distinct from a line's possibly duplicated legacy ID.
    pub id: String,
    pub content: PlanPageContentDto,
    /// Exact raw text. Presentation must not normalize it before interpreting spans.
    pub text: String,
    /// UTF-16 scalar offsets into `text`, using the same marks as ordinary Pages.
    pub spans: Vec<TextSpanDto>,
}

impl From<&PlanBlock> for PlanPageBlockDto {
    fn from(block: &PlanBlock) -> Self {
        let content = match &block.content {
            PlanBlockContent::Title { budget_cents } => PlanPageContentDto::Title {
                budget_cents: *budget_cents,
            },
            PlanBlockContent::Summary => PlanPageContentDto::Summary,
            PlanBlockContent::Section => PlanPageContentDto::Section,
            PlanBlockContent::Line {
                legacy_id,
                cost_cents,
                done,
            } => PlanPageContentDto::Line {
                legacy_id: legacy_id.clone(),
                cost_cents: *cost_cents,
                done: *done,
            },
            PlanBlockContent::Prose { kind } => PlanPageContentDto::Prose { kind: kind.into() },
        };
        Self {
            id: block.id.clone(),
            content,
            text: block.plain_text(),
            spans: roda_docs::spans_of(&block.runs)
                .into_iter()
                .map(|span| TextSpanDto {
                    start: span.start,
                    end: span.end,
                    key: span.key,
                    value: span.value,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlanPageDto {
    pub item_id: String,
    pub space_id: String,
    /// Last semantically accepted shared version; unsaved blocks can be newer locally.
    pub version: u32,
    pub ready: bool,
    pub unsaved: bool,
    pub pending_sync: bool,
    pub save_error: Option<String>,
    pub content_hash: String,
    /// None means the exact mathematical total is outside i64, never truncated or zero.
    pub total_cents: Option<i64>,
    pub blocks: Vec<PlanPageBlockDto>,
}

impl Engine {
    /// Reads the local document session, including a clearly labelled private draft.
    /// Shared cards/search continue to use the accepted shadow via `plan_snapshot`.
    /// This read does not authorize a Plan mutation or expose an editing capability.
    pub fn plan_page(&self, item: &str) -> R<PlanPageDto> {
        self.with_document(item, ItemKind::Plan, |it, session| {
            let read = session.read(self)?;
            if !matches!(read.tag, DocumentTag::PlanV1(_)) {
                return Err(invalid(
                    "este Item não é um plano",
                    "this Item is not a Plan",
                ));
            }
            let DocumentView::Plan { blocks } = read.view else {
                return Err(invalid(
                    "este Item não é um plano",
                    "this Item is not a Plan",
                ));
            };
            let total_cents = blocks
                .iter()
                .filter_map(|block| match block.content {
                    PlanBlockContent::Line { cost_cents, .. } => Some(cost_cents),
                    _ => None,
                })
                .try_fold(0_i128, |total, cents| total.checked_add(i128::from(cents)))
                .and_then(|total| i64::try_from(total).ok());
            Ok(PlanPageDto {
                item_id: it.id.clone(),
                space_id: it.space.clone(),
                version: read.version,
                ready: read.ready,
                unsaved: read.unsaved,
                pending_sync: read.pending_sync,
                save_error: read.save_error,
                content_hash: read.content_hash,
                total_cents,
                blocks: blocks.iter().map(PlanPageBlockDto::from).collect(),
            })
        })
    }
}

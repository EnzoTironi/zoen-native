//! Lossless Plan reads and legacy control admission. Canonical writes stay closed.

use roda_docs::plan::{PlanBlock, PlanBlockContent};
use roda_docs::Kind;
use roda_types::{ItemContent, ItemKind, PlanDoc};
use sha2::{Digest, Sha256};

use crate::engine::{Engine, ItemState, R};
use crate::pages::session::{DocumentTag, DocumentView};
use crate::pages::{invalid, TextSpanDto};

#[derive(Debug, Clone, PartialEq, serde::Serialize, uniffi::Enum, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
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

#[derive(Debug, Clone, PartialEq, serde::Serialize, uniffi::Enum, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
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

#[derive(Debug, Clone, PartialEq, serde::Serialize, uniffi::Record, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct PlanPageBlockDto {
    /// CRDT identity in DocumentSession; a presentation ID in LocalPendingLegacy.
    /// Both are distinct from a line's possibly duplicated legacy ID.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum PlanPageReadSourceDto {
    DocumentSession,
    /// This device's signed, durable legacy intent, without a shared manifest/edit context.
    LocalPendingLegacy,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct PlanPageDto {
    pub item_id: String,
    pub space_id: String,
    pub source: PlanPageReadSourceDto,
    /// Last semantically accepted shared version, or zero before creation confirms.
    pub version: u32,
    /// Presentation is available; LocalPendingLegacy does not imply confirmed content.
    pub ready: bool,
    /// Existing legacy controls only; canonical writes remain closed.
    /// Uses the actual legacy gates; private or unreadable sessions remain read-only.
    pub legacy_edit_admitted: bool,
    pub unsaved: bool,
    pub input_pending: bool,
    pub input_revision: String,
    pub pending_sync: bool,
    pub save_error: Option<String>,
    /// Admitted PlanDoc binding. LocalPendingLegacy and DocumentSession share it.
    /// A pure relay confirmation does not change it, and a private session draft does
    /// not move it. Title, budget, summary, section, text, cents, completion, and line
    /// order of that PlanDoc do.
    pub content_hash: String,
    /// None means the exact mathematical total is outside i64, never truncated or zero.
    pub total_cents: Option<i64>,
    pub blocks: Vec<PlanPageBlockDto>,
}

impl Engine {
    fn legacy_edit_admitted(&self, it: &ItemState) -> bool {
        if it.kind != ItemKind::Plan || self.require_legacy_plan(&it.id).is_err() {
            return false;
        }
        let ItemContent::Plan(document) = &it.current().content else {
            return false;
        };
        if Self::plan_total(document).is_none() {
            return false;
        }
        let Ok(actor) = self.me_id() else {
            return false;
        };
        if !self.state.writer(&it.space, &actor) {
            return false;
        }
        let Ok(author) = self.author_for(&actor) else {
            return false;
        };
        !self.net.synced.contains(&it.space)
            || self
                .net
                .account
                .as_ref()
                .is_none_or(|account| account.identity == author.identity)
    }

    fn local_legacy_plan_page(
        &self,
        it: &ItemState,
        accepted_version: u32,
        legacy_edit_admitted: bool,
    ) -> R<PlanPageDto> {
        let ItemContent::Plan(document) = &it.current().content else {
            return Err(invalid(
                "este Item não é um plano",
                "this Item is not a Plan",
            ));
        };
        let blocks = local_legacy_blocks(it, document);
        Ok(PlanPageDto {
            item_id: it.id.clone(),
            space_id: it.space.clone(),
            source: PlanPageReadSourceDto::LocalPendingLegacy,
            version: accepted_version,
            ready: true,
            legacy_edit_admitted,
            unsaved: false,
            input_pending: false,
            input_revision: "0".to_owned(),
            pending_sync: true,
            save_error: None,
            content_hash: plan_observed_hash(&blocks),
            total_cents: Self::plan_total(document),
            blocks,
        })
    }

    /// Reads the local document session, including a clearly labelled private draft.
    /// Shared cards/search continue to use the accepted shadow via `plan_snapshot`.
    /// Legacy control admission is read-only; mutations still validate before storage.
    pub fn plan_page(&self, item: &str) -> R<PlanPageDto> {
        let it = self.item_state(item)?;
        if it.kind == ItemKind::Plan {
            let fold = self.plan_timeline(item)?;
            if fold.origin_tag().is_none()
                && fold.pending_legacy
                && !fold.waiting
                && self.require_legacy_plan(item).is_ok()
                && matches!(&it.current().content, ItemContent::Plan(_))
            {
                return self.local_legacy_plan_page(it, 0, self.legacy_edit_admitted(it));
            }
        }
        self.with_document(item, ItemKind::Plan, |it, session| {
            let read = session.read(self)?;
            if !matches!(read.tag, DocumentTag::PlanV1(_)) {
                return Err(invalid(
                    "este Item não é um plano",
                    "this Item is not a Plan",
                ));
            }
            let legacy_edit_admitted =
                !read.unsaved && !session.legacy_read_blocked() && self.legacy_edit_admitted(it);
            let fold = self.plan_timeline(item)?;
            if fold.pending_legacy
                && !read.unsaved
                && !session.legacy_read_blocked()
                && self.require_legacy_plan(item).is_ok()
                && matches!(&it.current().content, ItemContent::Plan(_))
            {
                let mut local =
                    self.local_legacy_plan_page(it, read.version, legacy_edit_admitted)?;
                // A pending legacy projection does not erase an already
                // authenticated input slot or grant authority to release it.
                local.input_pending = read.input_pending;
                local.input_revision = read.input_revision.to_string();
                return Ok(local);
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
            let blocks: Vec<_> = blocks.iter().map(PlanPageBlockDto::from).collect();
            // The binding is over the PlanDoc the writers mutate. The session-block fallback
            // is reachable: a Plan Item whose current version is Page content (an accepted
            // PlanV1 document version, see tests/prefix.rs) has no PlanDoc. No legacy writer
            // can admit an edit there (`edit_plan` needs a PlanDoc), so this value only has
            // to be stable and distinct for the read; it is never matched by a write.
            let content_hash = plan_doc_binding(it).unwrap_or_else(|| plan_observed_hash(&blocks));
            Ok(PlanPageDto {
                item_id: it.id.clone(),
                space_id: it.space.clone(),
                source: PlanPageReadSourceDto::DocumentSession,
                version: read.version,
                ready: read.ready,
                legacy_edit_admitted,
                unsaved: read.unsaved,
                input_pending: read.input_pending,
                input_revision: read.input_revision.to_string(),
                pending_sync: read.pending_sync,
                save_error: read.save_error,
                content_hash,
                total_cents,
                blocks,
            })
        })
    }
}

/// The admitted PlanDoc binding of the current version, the same value `plan_page`
/// reports as `content_hash`.
fn plan_doc_binding(it: &ItemState) -> Option<String> {
    match &it.current().content {
        ItemContent::Plan(document) => Some(plan_observed_hash(&local_legacy_blocks(it, document))),
        _ => None,
    }
}

impl Engine {
    /// Cheap binding read for the undo token: hashes the PlanDoc only.
    pub(crate) fn plan_doc_hash(&self, item: &str) -> R<String> {
        plan_doc_binding(self.item_state(item)?)
            .ok_or_else(|| invalid("este Item não é um plano", "this Item is not a Plan"))
    }
}

fn local_legacy_blocks(it: &ItemState, document: &PlanDoc) -> Vec<PlanPageBlockDto> {
    let prefix = format!("local-legacy:{}:v{}", it.id, it.current().number);
    let block = |suffix: &str, content, text: &str| PlanPageBlockDto {
        id: format!("{prefix}:{suffix}"),
        content,
        text: text.into(),
        spans: vec![],
    };
    let mut blocks = vec![
        block(
            "title",
            PlanPageContentDto::Title {
                budget_cents: document.budget_cents,
            },
            &document.title,
        ),
        block("summary", PlanPageContentDto::Summary, &document.summary),
    ];
    for (section_index, section) in document.sections.iter().enumerate() {
        blocks.push(block(
            &format!("section:{section_index}"),
            PlanPageContentDto::Section,
            &section.title,
        ));
        for (line_index, line) in section.lines.iter().enumerate() {
            blocks.push(block(
                &format!("section:{section_index}:line:{line_index}"),
                PlanPageContentDto::Line {
                    legacy_id: line.id.clone(),
                    cost_cents: line.cost_cents,
                    done: line.done,
                },
                &line.text,
            ));
        }
    }
    blocks
}

/// One edit binding for both read sources, over the PlanDoc the writers mutate.
/// Presentation and CRDT block ids stay out, so a pure relay confirmation keeps the value.
fn plan_observed_hash(blocks: &[PlanPageBlockDto]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"zoen-plan-observed-v1\0");
    hasher.update((blocks.len() as u64).to_le_bytes());
    for block in blocks {
        hash_observed_block(&mut hasher, block);
    }
    hex::encode(hasher.finalize())
}

fn hash_observed_str(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn hash_observed_block(hasher: &mut Sha256, block: &PlanPageBlockDto) {
    match &block.content {
        PlanPageContentDto::Title { budget_cents } => {
            hasher.update([1]);
            match budget_cents {
                None => hasher.update([0]),
                Some(cents) => {
                    hasher.update([1]);
                    hasher.update(cents.to_le_bytes());
                }
            }
        }
        PlanPageContentDto::Summary => hasher.update([2]),
        PlanPageContentDto::Section => hasher.update([3]),
        PlanPageContentDto::Line {
            legacy_id,
            cost_cents,
            done,
        } => {
            hasher.update([4]);
            hash_observed_str(hasher, legacy_id);
            hasher.update(cost_cents.to_le_bytes());
            hasher.update([u8::from(*done)]);
        }
        PlanPageContentDto::Prose { kind } => {
            hasher.update([5]);
            hash_observed_prose(hasher, kind);
        }
    }
    hash_observed_str(hasher, &block.text);
    hasher.update((block.spans.len() as u64).to_le_bytes());
    for span in &block.spans {
        hasher.update(span.start.to_le_bytes());
        hasher.update(span.end.to_le_bytes());
        hash_observed_str(hasher, &span.key);
        hash_observed_str(hasher, &span.value);
    }
}

fn hash_observed_prose(hasher: &mut Sha256, kind: &PlanProseKindDto) {
    match kind {
        PlanProseKindDto::Paragraph => hasher.update([1]),
        PlanProseKindDto::Heading { level, setext } => {
            hasher.update([2]);
            hasher.update(level.to_le_bytes());
            hasher.update([u8::from(*setext)]);
        }
        PlanProseKindDto::Bullet { indent } => {
            hasher.update([3]);
            hasher.update(indent.to_le_bytes());
        }
        PlanProseKindDto::Numbered { indent, number } => {
            hasher.update([4]);
            hasher.update(indent.to_le_bytes());
            hasher.update(number.to_le_bytes());
        }
        PlanProseKindDto::Task { indent, checked } => {
            hasher.update([5]);
            hasher.update(indent.to_le_bytes());
            hasher.update([u8::from(*checked)]);
        }
        PlanProseKindDto::Quote => hasher.update([6]),
        PlanProseKindDto::Code { lang } => {
            hasher.update([7]);
            hash_observed_str(hasher, lang);
        }
        PlanProseKindDto::Divider => hasher.update([8]),
        PlanProseKindDto::Image { url, alt, title } => {
            hasher.update([9]);
            hash_observed_str(hasher, url);
            hash_observed_str(hasher, alt);
            hash_observed_str(hasher, title);
        }
        PlanProseKindDto::Raw => hasher.update([10]),
    }
}

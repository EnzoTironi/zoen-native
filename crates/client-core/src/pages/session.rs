//! One document session owns canonical state, drafts, receipts and publication.
//! Public Page adapters admit `BinaryPage` only until Plan consumers and clients migrate.

use base64::Engine as _;
use documents::plan::{PlanBlock, PlanBlockEdit, PlanDocument, PlanRevision, PlanUpdate};
use documents::{Block, BlockEdit, Frontiers, Page};
use std::sync::Arc;
use types::{Event, EventBody, ItemContent, ItemKind, PageManifest, Role, SpaceId};

use crate::engine::{Engine, ItemState, R};
use crate::i18n::t;
use crate::CoreError;

use super::drafts::{
    self,
    tagged::{
        decode_bytes, decode_context, encode_bytes, invalid, PlanCommitRef, PlanJournalV1,
        PlanOrigin, PlanReceiptV1, PlanSnapshotV1, MAX_CONTEXT_BYTES,
    },
};
use super::plan::{self, Acceptance, PlanFold};
use super::PageBlockDto;

mod agent;

pub(super) enum SessionDocument {
    BinaryPage(Page),
    Plan(PlanDocument),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DocumentTag {
    BinaryPageV1,
    PlanV1(PlanOrigin),
}

pub(super) enum DocumentCursor {
    BinaryPage(Frontiers),
    Plan(PlanRevision),
}

impl SessionDocument {
    fn cursor(&self) -> R<DocumentCursor> {
        match self {
            Self::BinaryPage(doc) => Ok(DocumentCursor::BinaryPage(doc.frontiers())),
            Self::Plan(doc) => Ok(DocumentCursor::Plan(doc.revision().map_err(plan_error)?)),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum DocumentView {
    BinaryPage { blocks: Vec<Block> },
    Plan { blocks: Vec<PlanBlock> },
}

pub(super) enum DocumentEdit<'a> {
    BinaryPage {
        order: &'a [String],
        changed: &'a [PageBlockDto],
    },
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Public Plan edit admission awaits typed clients and compatibility migration"
        )
    )]
    Plan {
        order: &'a [String],
        changed: &'a [PlanBlockEdit],
    },
}

pub(super) enum SessionJournal {
    BinaryPage(Box<drafts::Journal>),
    Plan(Box<PlanJournalV1>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SessionBlock {
    Waiting(String),
    DraftLocked(String),
    PendingLegacy,
}

struct BinaryHistory {
    epoch: Arc<()>,
    space: SpaceId,
    origin: String,
    applied: usize,
    frontiers: Vec<Frontiers>,
}

#[derive(PartialEq, Eq)]
struct PendingPlanInput {
    source: plan::Source,
    failed: bool,
    last_error: Option<String>,
}

#[derive(PartialEq, Eq)]
struct PlanInputs {
    confirmed: Vec<plan::Source>,
    pending: Vec<PendingPlanInput>,
}

impl PlanInputs {
    fn read(engine: &Engine, it: &ItemState) -> R<Self> {
        let mut logs: Vec<_> = engine.logs.iter().collect();
        logs.sort_by_key(|(space, _)| *space);
        let mut confirmed = Vec::new();
        for (space, log) in logs {
            for event in log.events() {
                let relevant = if space == &it.space {
                    plan::item_event(event, &it.id)
                        || matches!(
                            event.body,
                            EventBody::SpaceCreated { .. }
                                | EventBody::MemberAdded { .. }
                                | EventBody::MemberRemoved { .. }
                        )
                } else {
                    matches!(&event.body, EventBody::ItemCreated { item, .. } if item == &it.id)
                };
                if relevant {
                    confirmed.push(plan::source(space, &it.id, event, true));
                }
            }
        }
        let pending = engine
            .store
            .outbox()?
            .into_iter()
            .filter(|p| p.event.space == it.space && plan::item_event(&p.event, &it.id))
            .map(|p| PendingPlanInput {
                source: plan::source(&it.space, &it.id, &p.event, false),
                failed: p.failed,
                last_error: p.last_error,
            })
            .collect();
        Ok(Self { confirmed, pending })
    }
}

struct PlanHistory {
    epoch: Arc<()>,
    space: SpaceId,
    origin: String,
    inputs: PlanInputs,
    metadata: PlanFold,
}

pub(crate) struct PageSession {
    item: String,
    tag: DocumentTag,
    shadow: SessionDocument,
    live: SessionDocument,
    owner: Option<String>,
    journal: SessionJournal,
    blocked: Option<SessionBlock>,
    binary_history: Option<BinaryHistory>,
    plan_history: Option<PlanHistory>,
    text: Option<(String, String)>,
    // Failure injection is entirely after actual durable writes, never fake storage.
    #[cfg(test)]
    lose_apply_response: bool,
    #[cfg(test)]
    lose_commit_response: bool,
}

pub(super) struct SessionRead {
    pub tag: DocumentTag,
    pub view: DocumentView,
    pub version: u32,
    pub content_hash: String,
    pub edit_context: String,
    pub unsaved: bool,
    pub input_pending: bool,
    pub input_revision: u64,
    pub pending_sync: bool,
    pub can_edit: bool,
    pub ready: bool,
    pub save_error: Option<String>,
}

pub(super) struct AcceptedEdit {
    pub mutation_id: String,
    pub request_hash: String,
    pub applied_content_hash: String,
    pub applied_edit_context: String,
    pub replayed: bool,
    pub current: SessionRead,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CommitRef {
    BinaryPage { client_id: String },
    Plan(PlanCommitRef),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum CommitStatus {
    Unchanged,
    Pending(CommitRef),
    Queued(CommitRef),
}

fn plan_error(error: impl std::fmt::Display) -> CoreError {
    invalid(&error.to_string())
}
fn sql_error(error: rusqlite::Error) -> CoreError {
    CoreError::Storage {
        message: error.to_string(),
    }
}

fn verify_receipt(
    engine: &Engine,
    item: &ItemState,
    tag: &PlanOrigin,
    doc: &PlanDocument,
    receipt: &PlanReceiptV1,
) -> R<PlanRevision> {
    if receipt.tag != *tag {
        return Err(invalid("the saved receipt belongs to another Plan origin"));
    }
    // The encrypted applied context is opened under the current actor, space, item,
    // and creation AAD before its receipt can be reused for replay or continuation.
    let revision = doc
        .verify_revision(&decode_bytes(
            &receipt.accepted_revision_base64,
            PlanRevision::MAX_ENCODED_BYTES,
        )?)
        .map_err(plan_error)?;
    if doc
        .at(&revision)
        .map_err(plan_error)?
        .semantic_hash()
        .map_err(plan_error)?
        != receipt.content_hash
    {
        return Err(invalid(
            "the saved receipt does not match its operation witness",
        ));
    }
    let context = decode_context(engine, item, tag, &receipt.applied_edit_context)?;
    if context.generation != receipt.accepted_generation
        || context.revision_base64 != receipt.accepted_revision_base64
    {
        return Err(invalid("the saved receipt context is not exact"));
    }
    Ok(revision)
}

fn stale() -> CoreError {
    CoreError::Stale {
        reason: "this edit is outside the retained generation fence".into(),
    }
}
fn origin(fold: &PlanFold) -> R<PlanOrigin> {
    let (space, item, creation_event) = fold
        .origin_tag()
        .ok_or_else(|| invalid("the Plan has no authenticated creation"))?;
    Ok(PlanOrigin {
        space: space.into(),
        item: item.into(),
        creation_event: creation_event.into(),
    })
}

fn plan_doc(document: &SessionDocument) -> R<&PlanDocument> {
    match document {
        SessionDocument::Plan(doc) => Ok(doc),
        _ => Err(invalid("this cursor is not a Plan")),
    }
}

#[cfg(test)]
mod tests;

mod authority;
mod binary_refresh;
mod editing;
mod input;
mod plan_refresh;
mod publication;
mod reading;

//! One document session owns canonical state, drafts, receipts and publication.
//! Public Page adapters admit BinaryPage only until Plan consumers and clients migrate.

use base64::Engine as _;
use roda_docs::plan::{PlanBlock, PlanBlockEdit, PlanDocument, PlanRevision, PlanUpdate};
use roda_docs::{Block, BlockEdit, Frontiers, Page};
use roda_types::*;
use std::sync::Arc;

use crate::engine::{Engine, ItemState, R};
use crate::i18n::t;
use crate::CoreError;

use super::drafts::{self, tagged::*};
use super::plan::{self, Acceptance, PlanFold};
use super::PageBlockDto;

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
    BinaryPage(drafts::Journal),
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

impl PageSession {
    fn write_author(&self, engine: &Engine) -> R<String> {
        let author = engine.page_write_author(&self.item)?;
        if matches!(self.tag, DocumentTag::PlanV1(_)) {
            let it = engine.item_state(&self.item)?;
            let mut role = None;
            for event in engine
                .logs
                .get(&it.space)
                .map(|log| log.events())
                .unwrap_or(&[])
            {
                match &event.body {
                    EventBody::SpaceCreated { .. } if event.author == author => {
                        role = Some(Role::Owner)
                    }
                    EventBody::MemberAdded {
                        identity,
                        role: next,
                    } if identity == &author => role = Some(*next),
                    EventBody::MemberRemoved { identity } if identity == &author => role = None,
                    _ => {}
                }
            }
            if role.is_none_or(|role| role == Role::Reader) {
                return Err(CoreError::Forbidden {
                    reason: "only an authenticated active writing member can edit a Plan".into(),
                });
            }
        }
        Ok(author)
    }

    pub(super) fn open(engine: &Engine, id: &str) -> R<Self> {
        let it = engine.item_state(id)?;
        let (tag, document, journal) = match it.kind {
            ItemKind::Plan => {
                let fold = engine.plan_timeline(id)?;
                let tag = origin(&fold)?;
                let document = fold
                    .document
                    .ok_or_else(|| invalid("the Plan is still arriving"))?;
                (
                    DocumentTag::PlanV1(tag.clone()),
                    SessionDocument::Plan(document),
                    SessionJournal::Plan(Box::new(PlanJournalV1::new(tag))),
                )
            }
            ItemKind::Page => (
                DocumentTag::BinaryPageV1,
                SessionDocument::BinaryPage(Page::new()),
                SessionJournal::BinaryPage(drafts::Journal::default()),
            ),
            _ => return Err(invalid("this Item is not a Page or Plan")),
        };
        let shadow = match &document {
            SessionDocument::Plan(doc) => SessionDocument::Plan(doc.fork().map_err(plan_error)?),
            SessionDocument::BinaryPage(doc) => {
                SessionDocument::BinaryPage(doc.at(&doc.frontiers()).map_err(super::doc_err)?)
            }
        };
        let mut session = Self {
            item: id.into(),
            tag,
            shadow,
            live: document,
            owner: engine.me.clone(),
            journal,
            blocked: None,
            binary_history: None,
            plan_history: None,
            text: None,
            #[cfg(test)]
            lose_apply_response: false,
            #[cfg(test)]
            lose_commit_response: false,
        };
        session.refresh(engine)?;
        Ok(session)
    }

    pub(super) fn refresh(&mut self, engine: &Engine) -> R<()> {
        if self.owner != engine.me {
            *self = Self::open(engine, &self.item)?;
            return Ok(());
        }
        match self.tag.clone() {
            DocumentTag::PlanV1(tag) => self.refresh_plan(engine, &tag),
            DocumentTag::BinaryPageV1 => self.refresh_binary(engine),
        }
    }

    pub(super) fn plan_snapshot(&mut self, engine: &Engine) -> R<plan::PlanSnapshot> {
        self.refresh(engine)?;
        let fold = match &self.plan_history {
            Some(history) => history.metadata.metadata(),
            None => engine.plan_timeline(&self.item)?,
        };
        if fold.waiting {
            return Err(invalid("the current Plan is still arriving"));
        }
        if fold.frozen_at.is_none() {
            let current = engine.item_state(&self.item)?.current();
            if let ItemContent::Plan(document) = &current.content {
                // Existing legacy clients keep their offline optimistic preview until
                // cutover. A canonical preview never uses an outbox or local draft.
                return Ok(plan::PlanSnapshot::legacy(document, current));
            }
        }
        let version = fold
            .accepted_current
            .ok_or_else(|| invalid("the Plan has no accepted current version"))?;
        let source = &fold
            .slots
            .iter()
            .find(|slot| slot.number == Some(version))
            .ok_or_else(|| invalid("the accepted Plan version has no authenticated source"))?
            .source;
        plan::PlanSnapshot::canonical(plan_doc(&self.shadow)?, version, source)
    }

    fn refresh_plan(&mut self, engine: &Engine, tag: &PlanOrigin) -> R<()> {
        let it = engine.item_state(&self.item)?;
        if it.kind != ItemKind::Plan {
            return Err(invalid("the Plan item kind changed"));
        }
        let inputs = PlanInputs::read(engine, it)?;
        let cached = self.plan_history.as_ref().filter(|history| {
            Arc::ptr_eq(&history.epoch, &engine.pages.epoch)
                && history.space == it.space
                && history.origin == it.origin
                && history.inputs == inputs
        });
        let fold = if let Some(history) = cached {
            let mut fold = history.metadata.metadata();
            fold.document = Some(plan_doc(&self.shadow)?.fork().map_err(plan_error)?);
            fold
        } else {
            engine.plan_timeline(&self.item)?
        };
        if origin(&fold)? != *tag {
            return Err(invalid("the authenticated Plan creation changed"));
        }
        let Some(shadow) = &fold.document else {
            self.plan_history = None;
            self.blocked = Some(SessionBlock::Waiting(
                "the authenticated Plan is still arriving".into(),
            ));
            return Ok(());
        };
        let shadow = shadow.fork().map_err(plan_error)?;
        let history = (!fold.waiting).then(|| PlanHistory {
            epoch: engine.pages.epoch.clone(),
            space: it.space.clone(),
            origin: it.origin.clone(),
            inputs,
            metadata: fold.metadata(),
        });
        let loaded = super::drafts::tagged::load(engine, it, tag);
        let mut journal = match loaded {
            Ok(journal) => journal,
            Err(error) => {
                // Do not replace an unreadable journal or discard an existing live intent.
                self.shadow = SessionDocument::Plan(shadow);
                self.plan_history = history;
                self.blocked = Some(SessionBlock::DraftLocked(error.to_string()));
                return Ok(());
            }
        };
        let mut live = shadow.fork().map_err(plan_error)?;
        if let Some(snapshot) = &journal.snapshot {
            let import = (|| {
                if !fold.accepts_prefix(&snapshot.prefix) {
                    return Err(invalid(
                        "the saved draft is not an exact authenticated prefix",
                    ));
                }
                let update = PlanUpdate::from_json(&decode_bytes(
                    &snapshot.update_json_base64,
                    roda_docs::plan::MAX_PLAN_PACKET_BYTES,
                )?)
                .map_err(plan_error)?;
                live.import(&update).map_err(plan_error)?;
                if let Some(commit) = &journal.commit {
                    let revision = live
                        .verify_revision(&decode_bytes(
                            &commit.published_revision_base64,
                            PlanRevision::MAX_ENCODED_BYTES,
                        )?)
                        .map_err(plan_error)?;
                    if live
                        .at(&revision)
                        .map_err(plan_error)?
                        .semantic_hash()
                        .map_err(plan_error)?
                        != commit.published_content_hash
                    {
                        return Err(invalid("the saved publication hash is not exact"));
                    }
                }
                Ok(())
            })();
            if let Err(error) = import {
                self.shadow = SessionDocument::Plan(shadow);
                self.plan_history = history;
                self.blocked = Some(SessionBlock::DraftLocked(error.to_string()));
                return Ok(());
            }
        } else if !journal.receipts.is_empty() || journal.commit.is_some() || journal.has_draft {
            self.blocked = Some(SessionBlock::DraftLocked(
                "the saved Plan has no operation history".into(),
            ));
            return Ok(());
        }
        let block = if fold.waiting {
            Some(SessionBlock::Waiting(
                "a Plan payload is still arriving".into(),
            ))
        } else if fold.pending_legacy {
            Some(SessionBlock::PendingLegacy)
        } else {
            None
        };
        // Only an exact semantically accepted ordered source releases this publication.
        // Read-only/key-loss paths never rewrite the encrypted journal as a side effect.
        if block.is_none()
            && self.write_author(engine).is_ok()
            && journal
                .commit
                .as_ref()
                .is_some_and(|c| fold.acknowledges(&c.client_id, &c.event_id))
        {
            journal.commit = None;
            journal.has_draft = live.semantic_hash().map_err(plan_error)?
                != shadow.semantic_hash().map_err(plan_error)?;
            super::drafts::tagged::save(engine, it, &journal)?;
        }
        self.shadow = SessionDocument::Plan(shadow);
        self.plan_history = history;
        self.live = SessionDocument::Plan(live);
        self.journal = SessionJournal::Plan(Box::new(journal));
        self.blocked = block;
        Ok(())
    }

    fn refresh_binary(&mut self, engine: &Engine) -> R<()> {
        let it = engine.item_state(&self.item)?;
        if it.kind != ItemKind::Page {
            return Err(invalid("the BinaryPage item kind changed"));
        }
        let reusable = self.binary_history.as_ref().filter(|history| {
            Arc::ptr_eq(&history.epoch, &engine.pages.epoch)
                && history.space == it.space
                && history.origin == it.origin
                && history.applied <= it.versions.len()
        });
        let loaded = engine.load_page_journal(it);
        if self.blocked.is_none()
            && reusable.is_some_and(|history| history.applied == it.versions.len())
        {
            if let (
                Ok(loaded),
                SessionJournal::BinaryPage(current),
                SessionDocument::BinaryPage(live),
                SessionDocument::BinaryPage(shadow),
            ) = (&loaded, &self.journal, &self.live, &self.shadow)
            {
                // Authenticate the current journal before reusing memory. A clean
                // no-op still takes the acknowledgement path below exactly once.
                if loaded == current
                    && (!loaded.has_draft || live.content_hash() != shadow.content_hash())
                {
                    return Ok(());
                }
            }
        }
        let (mut shadow, mut applied, mut frontiers) = match (&self.shadow, reusable) {
            (SessionDocument::BinaryPage(shadow), Some(history)) => (
                shadow.at(&shadow.frontiers()).map_err(super::doc_err)?,
                history.applied,
                history.frontiers.clone(),
            ),
            _ => (Page::new(), 0, Vec::new()),
        };
        let mut blocked = None;
        for (index, version) in it.versions.iter().enumerate().skip(applied) {
            if let ItemContent::Page(page) = &version.content {
                if page.manifest.is_some() {
                    return Err(invalid("BinaryPage does not accept a Plan manifest"));
                }
                let Some(bytes) = engine.page_payload(page) else {
                    blocked = Some(SessionBlock::Waiting("the Page is still arriving".into()));
                    break;
                };
                let imported = shadow.at(&shadow.frontiers()).map_err(super::doc_err)?;
                if let Err(error) = imported.import(&bytes) {
                    blocked = Some(SessionBlock::Waiting(error.to_string()));
                    break;
                }
                shadow = imported;
            }
            frontiers.push(shadow.frontiers());
            applied = index + 1;
        }
        let history = BinaryHistory {
            epoch: engine.pages.epoch.clone(),
            space: it.space.clone(),
            origin: it.origin.clone(),
            applied,
            frontiers,
        };
        let mut journal = match loaded {
            Ok(journal) => journal,
            Err(error) => {
                self.lock_binary(shadow, history, error.to_string())?;
                return Ok(());
            }
        };
        let live = shadow.at(&shadow.frontiers()).map_err(super::doc_err)?;
        if let Some(snapshot) = &journal.snapshot {
            let result = base64::engine::general_purpose::STANDARD
                .decode(snapshot)
                .map_err(|_| invalid("invalid binary draft"))
                .and_then(|bytes| live.import(&bytes).map_err(super::doc_err));
            if let Err(error) = result {
                self.lock_binary(shadow, history, error.to_string())?;
                return Ok(());
            }
        }
        // This branch keeps BinaryPageV1's existing acknowledgement rule and formats.
        // The stronger exact-source semantic fence belongs only to the new Plan tag.
        if blocked.is_none()
            && journal.has_draft
            && live.content_hash() == shadow.content_hash()
            && self.write_author(engine).is_ok()
        {
            let acknowledged = journal.commit.as_ref().is_some_and(|id| {
                engine
                    .logs
                    .get(&it.space)
                    .is_some_and(|log| log.events().iter().any(|event| &event.client_id == id))
            });
            let pending = self.pending(engine)?;
            let queue = engine.store.outbox()?;
            let failed = journal
                .commit
                .as_ref()
                .is_some_and(|id| queue.iter().any(|p| &p.event.client_id == id && p.failed));
            if acknowledged || (journal.commit.is_none() && !pending && !failed) {
                journal.has_draft = false;
                journal.commit = None;
                engine.save_page_journal(it, &journal)?;
            }
        }
        self.shadow = SessionDocument::BinaryPage(shadow);
        self.binary_history = Some(history);
        self.live = SessionDocument::BinaryPage(live);
        self.journal = SessionJournal::BinaryPage(journal);
        self.blocked = blocked;
        Ok(())
    }

    fn lock_binary(&mut self, shadow: Page, history: BinaryHistory, reason: String) -> R<()> {
        // A cold reader still sees the saved page. An existing reader retains its
        // last verified live intent while the encrypted bytes remain untouched.
        if self.binary_history.is_none() {
            self.live = SessionDocument::BinaryPage(
                shadow.at(&shadow.frontiers()).map_err(super::doc_err)?,
            );
        }
        self.shadow = SessionDocument::BinaryPage(shadow);
        self.binary_history = Some(history);
        self.blocked = Some(SessionBlock::DraftLocked(reason));
        Ok(())
    }

    fn binary(&self) -> R<&Page> {
        match &self.live {
            SessionDocument::BinaryPage(document) => Ok(document),
            _ => Err(super::invalid(
                "este Item não é uma página",
                "this Item is not a page",
            )),
        }
    }

    fn binary_frontier(&self, version: u32) -> R<&Frontiers> {
        version
            .checked_sub(1)
            .and_then(|index| self.binary_history.as_ref()?.frontiers.get(index as usize))
            .ok_or_else(|| super::invalid("versão não encontrada", "version not found"))
    }

    pub(super) fn binary_at(&self, version: u32) -> R<Page> {
        let SessionDocument::BinaryPage(shadow) = &self.shadow else {
            return Err(super::invalid(
                "este Item não é uma página",
                "this Item is not a page",
            ));
        };
        shadow
            .at(self.binary_frontier(version)?)
            .map_err(super::doc_err)
    }

    pub(super) fn restore_binary(&mut self, engine: &Engine, version: u32) -> R<()> {
        self.refresh(engine)?;
        self.write_author(engine)?;
        if self.blocked.is_some() {
            return Err(super::invalid(
                "a página ainda está chegando",
                "the page is still arriving",
            ));
        }
        let SessionJournal::BinaryPage(journal) = &self.journal else {
            return Err(super::invalid(
                "este Item não é uma página",
                "this Item is not a page",
            ));
        };
        let live = self
            .binary()?
            .at(&self.binary()?.frontiers())
            .map_err(super::doc_err)?;
        live.revert_to(self.binary_frontier(version)?)
            .map_err(super::doc_err)?;
        let mut journal = journal.clone();
        journal.invalidate_contexts()?;
        journal.snapshot = Some(encode_bytes(&live.snapshot()));
        journal.has_draft = true;
        journal.commit = None;
        engine.save_page_journal(engine.item_state(&self.item)?, &journal)?;
        self.live = SessionDocument::BinaryPage(live);
        self.journal = SessionJournal::BinaryPage(journal);
        Ok(())
    }

    pub(super) fn binary_markdown(&mut self, engine: &Engine) -> R<String> {
        self.refresh(engine)?;
        if matches!(self.blocked, Some(SessionBlock::Waiting(_))) {
            return Err(super::invalid(
                "a página ainda está chegando",
                "the page is still arriving",
            ));
        }
        Ok(self.binary()?.to_markdown())
    }

    pub(super) fn binary_text(&mut self, engine: &Engine) -> R<String> {
        self.refresh(engine)?;
        let document = self.binary()?;
        let hash = document.content_hash();
        if let Some((cached_hash, text)) = &self.text {
            if cached_hash == &hash {
                return Ok(text.clone());
            }
        }
        let text = document
            .blocks()
            .iter()
            .map(|block| block.plain_text().replace('\u{2028}', " "))
            .collect::<Vec<_>>()
            .join("\n");
        if !matches!(self.blocked, Some(SessionBlock::Waiting(_))) {
            self.text = Some((hash, text.clone()));
        }
        Ok(text)
    }

    fn pending(&self, engine: &Engine) -> R<bool> {
        let space = &engine.item_state(&self.item)?.space;
        Ok(engine.store.outbox()?.iter().any(|p| !p.failed && p.event.space == *space
            && matches!(&p.event.body, EventBody::ItemCreated { item, .. } | EventBody::ItemVersioned { item, .. } | EventBody::ItemReverted { item, .. } if item == &self.item)))
    }

    fn save_error(&self, engine: &Engine) -> R<Option<String>> {
        if let Some(block) = &self.blocked {
            return Ok(Some(match block {
                SessionBlock::Waiting(reason) | SessionBlock::DraftLocked(reason) => reason.clone(),
                SessionBlock::PendingLegacy => {
                    "a legacy Plan intent must confirm or refuse before editing".into()
                }
            }));
        }
        if let SessionJournal::BinaryPage(journal) = &self.journal {
            let error = if journal.has_draft {
                engine.store.outbox()?.into_iter().rev().find(|p| {
                    p.failed
                        && matches!(&p.event.body,
                            EventBody::ItemCreated { item, .. }
                                | EventBody::ItemVersioned { item, .. }
                            if item == &self.item)
                        && journal
                            .commit
                            .as_ref()
                            .is_none_or(|id| id == &p.event.client_id)
                })
            } else {
                None
            };
            return Ok(error.map(|p| {
                p.last_error
                    .unwrap_or_else(|| t("não foi possível salvar", "could not save"))
            }));
        }
        if let SessionJournal::Plan(journal) = &self.journal {
            if let Some(commit) = &journal.commit {
                let fold = engine.plan_timeline(&self.item)?;
                if let Some(slot) = fold.slots.iter().find(|slot| {
                    slot.source.client_id == commit.client_id
                        && slot.source.event_id == commit.event_id
                }) {
                    if let Acceptance::Refused(reason)
                    | Acceptance::TransportRefused(reason)
                    | Acceptance::Waiting(reason) = &slot.acceptance
                    {
                        return Ok(Some(reason.clone()));
                    }
                }
                // A removed outbox row is not an ACK; keep the uncertain marker visible.
                if !fold.acknowledges(&commit.client_id, &commit.event_id)
                    && !fold.slots.iter().any(|s| {
                        s.source.client_id == commit.client_id
                            && s.source.event_id == commit.event_id
                    })
                {
                    return Ok(Some(
                        "publication has no complete authenticated semantic acknowledgement".into(),
                    ));
                }
            }
        }
        Ok(None)
    }

    fn read_current(&self, engine: &Engine) -> R<SessionRead> {
        let it = engine.item_state(&self.item)?;
        let can_edit = self.blocked.is_none() && self.write_author(engine).is_ok();
        let save_error = self.save_error(engine)?;
        let (view, content_hash, unsaved) = match (&self.tag, &self.live, &self.journal) {
            (DocumentTag::PlanV1(_), SessionDocument::Plan(doc), SessionJournal::Plan(journal)) => {
                (
                    DocumentView::Plan {
                        blocks: doc.blocks().map_err(plan_error)?,
                    },
                    doc.semantic_hash().map_err(plan_error)?,
                    journal.has_draft,
                )
            }
            (
                DocumentTag::BinaryPageV1,
                SessionDocument::BinaryPage(doc),
                SessionJournal::BinaryPage(journal),
            ) => {
                let SessionDocument::BinaryPage(shadow) = &self.shadow else {
                    return Err(invalid("the binary session has no canonical shadow"));
                };
                (
                    DocumentView::BinaryPage {
                        blocks: doc.blocks(),
                    },
                    doc.content_hash(),
                    doc.content_hash() != shadow.content_hash()
                        || matches!(self.blocked, Some(SessionBlock::DraftLocked(_)))
                        || (journal.has_draft && save_error.is_some()),
                )
            }
            _ => return Err(invalid("session document and journal tags differ")),
        };
        let edit_context = if can_edit {
            match (&self.tag, &self.journal, self.live.cursor()?) {
                (
                    DocumentTag::PlanV1(tag),
                    SessionJournal::Plan(journal),
                    DocumentCursor::Plan(revision),
                ) => super::drafts::tagged::context(
                    engine,
                    it,
                    tag,
                    journal.generation,
                    &revision.encode().map_err(plan_error)?,
                )?,
                (
                    DocumentTag::BinaryPageV1,
                    SessionJournal::BinaryPage(journal),
                    DocumentCursor::BinaryPage(frontiers),
                ) => engine.page_context_at(it, journal.generation, frontiers.encode())?,
                _ => return Err(invalid("session document and cursor tags differ")),
            }
        } else {
            String::new()
        };
        Ok(SessionRead {
            tag: self.tag.clone(),
            view,
            version: self
                .plan_history
                .as_ref()
                .map_or(it.versions.len() as u32, |history| {
                    history.metadata.accepted_current.unwrap_or(0)
                }),
            content_hash,
            edit_context,
            unsaved,
            pending_sync: self.pending(engine)?,
            can_edit,
            ready: !matches!(self.blocked, Some(SessionBlock::Waiting(_))),
            save_error,
        })
    }

    /// Pending legacy intent can use its existing writer; unreadable journals cannot.
    pub(super) fn legacy_read_blocked(&self) -> bool {
        matches!(
            self.blocked,
            Some(SessionBlock::Waiting(_) | SessionBlock::DraftLocked(_))
        )
    }

    pub(super) fn read(&mut self, engine: &Engine) -> R<SessionRead> {
        self.refresh(engine)?;
        self.read_current(engine)
    }

    pub(super) fn apply_from(
        &mut self,
        engine: &Engine,
        mutation_id: &str,
        context: &str,
        edit: DocumentEdit<'_>,
    ) -> R<AcceptedEdit> {
        self.refresh(engine)?;
        self.write_author(engine)?;
        if self.blocked.is_some() {
            return Err(invalid("the document is not ready for editing"));
        }
        if mutation_id.is_empty()
            || mutation_id.len() > 128
            || !mutation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(invalid("invalid edit identifier"));
        }
        let result = match edit {
            DocumentEdit::Plan { order, changed } => {
                self.apply_plan(engine, mutation_id, context, order, changed)
            }
            DocumentEdit::BinaryPage { order, changed } => {
                self.apply_binary(engine, mutation_id, context, order, changed)
            }
        }?;
        #[cfg(test)]
        if self.lose_apply_response {
            self.lose_apply_response = false;
            return Err(CoreError::Storage {
                message: "injected lost response after durable acceptance".into(),
            });
        }
        Ok(result)
    }

    fn apply_plan(
        &mut self,
        engine: &Engine,
        id: &str,
        context: &str,
        order: &[String],
        changed: &[PlanBlockEdit],
    ) -> R<AcceptedEdit> {
        let (DocumentTag::PlanV1(tag), SessionJournal::Plan(journal)) = (&self.tag, &self.journal)
        else {
            return Err(invalid("a Plan edit requires a Plan session"));
        };
        let it = engine.item_state(&self.item)?;
        let decoded = decode_context(engine, it, tag, context)?;
        let doc = plan_doc(&self.live)?;
        let revision = doc
            .verify_revision(&decode_bytes(
                &decoded.revision_base64,
                PlanRevision::MAX_ENCODED_BYTES,
            )?)
            .map_err(plan_error)?;
        let request_hash = super::drafts::tagged::request_hash(tag, id, context, order, changed)?;
        if let Some(receipt) = journal.receipt(id) {
            if receipt.request_hash != request_hash {
                return Err(stale());
            }
            verify_receipt(engine, it, tag, doc, receipt)?;
            return Ok(AcceptedEdit {
                mutation_id: id.into(),
                request_hash,
                applied_content_hash: receipt.content_hash.clone(),
                applied_edit_context: receipt.applied_edit_context.clone(),
                replayed: true,
                current: self.read_current(engine)?,
            });
        }
        if decoded.generation < journal.minimum_generation {
            return Err(stale());
        }
        if decoded.generation > journal.generation {
            return Err(invalid("future editing context"));
        }
        let context_hash = super::drafts::tagged::context_hash(context);
        let mut desired = doc.at(&revision).map_err(plan_error)?;
        let (observed, base) = if let Some(prior) = journal.continuation(&context_hash) {
            desired
                .apply_from(&revision, order, changed)
                .map_err(plan_error)?;
            let previous = verify_receipt(engine, it, tag, doc, prior)?;
            let mut observed = doc.at(&previous).map_err(plan_error)?;
            let base = observed.version();
            let complete: Vec<_> = desired
                .blocks()
                .map_err(plan_error)?
                .into_iter()
                .map(|block| PlanBlockEdit {
                    text: block.plain_text(),
                    spans: roda_docs::spans_of(&block.runs),
                    id: block.id,
                    content: block.content,
                })
                .collect();
            observed
                .apply_from(&previous, order, &complete)
                .map_err(plan_error)?;
            (observed, base)
        } else {
            let base = desired.version();
            desired
                .apply_from(&revision, order, changed)
                .map_err(plan_error)?;
            (desired, base)
        };
        let accepted_hash = observed.semantic_hash().map_err(plan_error)?;
        let accepted_revision = observed
            .revision()
            .map_err(plan_error)?
            .encode()
            .map_err(plan_error)?;
        let mut live = doc.fork().map_err(plan_error)?;
        live.import(&observed.updates_since(&base).map_err(plan_error)?)
            .map_err(plan_error)?;
        live.blocks().map_err(plan_error)?;
        let merged_revision = live
            .revision()
            .map_err(plan_error)?
            .encode()
            .map_err(plan_error)?;
        let mut staged = journal.clone();
        let generation = staged.next_generation()?;
        let accepted_context =
            super::drafts::tagged::context(engine, it, tag, generation, &accepted_revision)?;
        // A response's current view must also have a representable context before the
        // durable acceptance. The receipt retains the separate own-branch context.
        super::drafts::tagged::context(engine, it, tag, generation, &merged_revision)?;
        staged.accept(PlanReceiptV1 {
            tag: tag.clone(),
            mutation_id: id.into(),
            request_hash: request_hash.clone(),
            context_hash,
            input_generation: decoded.generation,
            accepted_generation: generation,
            content_hash: accepted_hash.clone(),
            accepted_revision_base64: encode_bytes(&accepted_revision),
            applied_edit_context: accepted_context.clone(),
        })?;
        staged.snapshot = Some(PlanSnapshotV1 {
            prefix: engine.plan_timeline(&self.item)?.manifest()?,
            update_json_base64: encode_bytes(&live.full_update().to_json().map_err(plan_error)?),
        });
        staged.has_draft = true;
        // Preserve an older in-flight publication while newer local intent is accepted.
        super::drafts::tagged::save(engine, it, &staged)?;
        self.live = SessionDocument::Plan(live);
        self.journal = SessionJournal::Plan(staged);
        Ok(AcceptedEdit {
            mutation_id: id.into(),
            request_hash,
            applied_content_hash: accepted_hash,
            applied_edit_context: accepted_context,
            replayed: false,
            current: self.read_current(engine)?,
        })
    }

    fn apply_binary(
        &mut self,
        engine: &Engine,
        id: &str,
        context: &str,
        order: &[String],
        changed: &[PageBlockDto],
    ) -> R<AcceptedEdit> {
        let (SessionDocument::BinaryPage(doc), SessionJournal::BinaryPage(journal)) =
            (&self.live, &self.journal)
        else {
            return Err(invalid("a binary edit requires a BinaryPage session"));
        };
        if context.len() > 2 * 1024 * 1024 {
            return Err(invalid("invalid binary editing context"));
        }
        let it = engine.item_state(&self.item)?;
        let decoded = engine.decode_page_edit_context(it, context)?;
        let request_hash = drafts::request_hash(id, context, order, changed)?;
        if let Some(receipt) = journal.receipt(id) {
            if receipt.fingerprint != request_hash {
                return Err(stale());
            }
            return Ok(AcceptedEdit {
                mutation_id: id.into(),
                request_hash,
                applied_content_hash: receipt.content_hash.clone(),
                applied_edit_context: engine.page_context_at(
                    it,
                    receipt.accepted_generation,
                    receipt.frontiers.clone(),
                )?,
                replayed: true,
                current: self.read_current(engine)?,
            });
        }
        if decoded.generation < journal.minimum_generation {
            return Err(stale());
        }
        if decoded.generation > journal.generation {
            return Err(invalid("future binary editing context"));
        }
        let revision = Frontiers::decode(&decoded.frontiers).map_err(plan_error)?;
        let desired = doc.at(&revision).map_err(super::doc_err)?;
        let current: std::collections::HashMap<_, _> = desired
            .blocks()
            .into_iter()
            .map(|b| (b.id, b.kind))
            .collect();
        let edits: Vec<_> = changed
            .iter()
            .map(|dto| BlockEdit {
                id: dto.id.clone(),
                kind: super::kind_from_dto(dto, current.get(&dto.id)),
                text: dto.text.clone(),
                spans: dto
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
            .collect();
        let context_hash = drafts::context_hash(context);
        let (observed, base) = if let Some(prior) = journal.continuation(&context_hash) {
            desired.apply(order, &edits).map_err(super::doc_err)?;
            let previous = Frontiers::decode(&prior.frontiers).map_err(plan_error)?;
            let observed = doc.at(&previous).map_err(super::doc_err)?;
            let base = observed.version();
            let complete: Vec<_> = desired
                .blocks()
                .into_iter()
                .map(|b| BlockEdit {
                    text: b.plain_text(),
                    spans: roda_docs::spans_of(&b.runs),
                    id: b.id,
                    kind: b.kind,
                })
                .collect();
            observed.apply(order, &complete).map_err(super::doc_err)?;
            (observed, base)
        } else {
            let base = desired.version();
            desired.apply(order, &edits).map_err(super::doc_err)?;
            (desired, base)
        };
        let live = doc.at(&doc.frontiers()).map_err(super::doc_err)?;
        live.import(&observed.updates_since(&base))
            .map_err(super::doc_err)?;
        let hash = observed.content_hash();
        let fronts = observed.frontiers().encode();
        let mut journal = journal.clone();
        journal.accept(
            id.into(),
            request_hash.clone(),
            context_hash,
            hash.clone(),
            decoded.generation,
            fronts.clone(),
        )?;
        journal.snapshot = Some(encode_bytes(&live.snapshot()));
        journal.has_draft = true;
        let accepted_context = engine.page_context_at(it, journal.generation, fronts)?;
        engine.save_page_journal(it, &journal)?;
        self.live = SessionDocument::BinaryPage(live);
        self.journal = SessionJournal::BinaryPage(journal);
        Ok(AcceptedEdit {
            mutation_id: id.into(),
            request_hash,
            applied_content_hash: hash,
            applied_edit_context: accepted_context,
            replayed: false,
            current: self.read_current(engine)?,
        })
    }

    pub(super) fn commit(&mut self, engine: &mut Engine, note: &str) -> R<CommitStatus> {
        self.refresh(engine)?;
        let author = self.write_author(engine)?;
        if self.blocked.is_some() {
            return Err(invalid("the document is not ready for publication"));
        }
        let it = engine.item_state(&self.item)?.clone();
        if matches!(self.tag, DocumentTag::PlanV1(_))
            && (!engine.net.synced.contains(&it.space)
                || !engine.store.synced_spaces()?.contains(&it.space))
        {
            return Err(invalid(
                "publication requires durable relay-ordered readiness",
            ));
        }
        match &self.journal {
            SessionJournal::Plan(journal) => {
                if let Some(commit) = &journal.commit {
                    let fold = engine.plan_timeline(&self.item)?;
                    let refused = fold.slots.iter().any(|slot| {
                        slot.source.client_id == commit.client_id
                            && slot.source.event_id == commit.event_id
                            && matches!(
                                slot.acceptance,
                                Acceptance::Refused(_) | Acceptance::TransportRefused(_)
                            )
                    });
                    if !refused {
                        return Ok(CommitStatus::Pending(CommitRef::Plan(commit.clone())));
                    }
                }
            }
            SessionJournal::BinaryPage(journal) => {
                if let Some(client_id) = &journal.commit {
                    if engine
                        .store
                        .outbox()?
                        .iter()
                        .any(|p| p.event.client_id == *client_id && !p.failed)
                    {
                        return Ok(CommitStatus::Pending(CommitRef::BinaryPage {
                            client_id: client_id.clone(),
                        }));
                    }
                }
            }
        }
        if self.pending(engine)? {
            if matches!(self.tag, DocumentTag::BinaryPageV1) {
                if let Some(pending) = engine.store.outbox()?.into_iter().find(|pending| {
                    !pending.failed
                        && pending.event.space == it.space
                        && plan::item_event(&pending.event, &self.item)
                }) {
                    return Ok(CommitStatus::Pending(CommitRef::BinaryPage {
                        client_id: pending.event.client_id,
                    }));
                }
            }
            return Err(invalid("an earlier publication is still pending"));
        }
        let unchanged = match (&self.live, &self.shadow) {
            (SessionDocument::Plan(live), SessionDocument::Plan(shadow)) => {
                live.semantic_hash().map_err(plan_error)?
                    == shadow.semantic_hash().map_err(plan_error)?
            }
            (SessionDocument::BinaryPage(live), SessionDocument::BinaryPage(shadow)) => {
                live.content_hash() == shadow.content_hash()
            }
            _ => return Err(invalid("session document kinds differ")),
        };
        if unchanged {
            match &self.journal {
                SessionJournal::Plan(journal) if journal.commit.is_none() && journal.has_draft => {
                    let mut journal = journal.clone();
                    journal.has_draft = false;
                    super::drafts::tagged::save(engine, &it, &journal)?;
                    self.journal = SessionJournal::Plan(journal);
                }
                SessionJournal::BinaryPage(journal)
                    if journal.commit.is_none() && journal.has_draft =>
                {
                    let mut journal = journal.clone();
                    journal.has_draft = false;
                    engine.save_page_journal(&it, &journal)?;
                    self.journal = SessionJournal::BinaryPage(journal);
                }
                _ => {}
            }
            return Ok(CommitStatus::Unchanged);
        }
        let (bytes, title, manifest, revision, hash) = match (&self.live, &self.shadow) {
            (SessionDocument::Plan(live), SessionDocument::Plan(shadow)) => {
                let fold = engine.plan_timeline(&self.item)?;
                let update = if fold.frozen_at.is_none() {
                    live.full_update()
                } else {
                    live.updates_since(&shadow.version()).map_err(plan_error)?
                };
                (
                    update.to_json().map_err(plan_error)?,
                    live.view().map_err(plan_error)?.title,
                    Some(PageManifest::PlanV1(fold.manifest()?)),
                    Some(encode_bytes(
                        &live
                            .revision()
                            .map_err(plan_error)?
                            .encode()
                            .map_err(plan_error)?,
                    )),
                    live.semantic_hash().map_err(plan_error)?,
                )
            }
            (SessionDocument::BinaryPage(live), SessionDocument::BinaryPage(shadow)) => (
                live.updates_since(&shadow.version()),
                live.title(),
                None,
                None,
                live.content_hash(),
            ),
            _ => return Err(invalid("session document kinds differ")),
        };
        let title = if matches!(self.tag, DocumentTag::BinaryPageV1) && title.is_empty() {
            t("Sem título", "Untitled")
        } else {
            title
        };
        if bytes.is_empty() {
            return Err(CoreError::Storage {
                message: t(
                    "não foi possível salvar a página",
                    "could not save the page",
                ),
            });
        }
        // No Transaction handle borrows Engine across append. Exclusive mutable Engine
        // ownership encloses queue + blob/upload + encrypted marker in this savepoint.
        engine
            .store
            .conn()
            .execute_batch("SAVEPOINT page_session_commit")
            .map_err(sql_error)?;
        let staged = (|| {
            let path = match (&self.tag, it.versions.first().map(|v| &v.content)) {
                (DocumentTag::BinaryPageV1, Some(ItemContent::Page(page))) => page.path.as_str(),
                _ => "",
            };
            let mut content = engine.page_content(&it.space, title, path, bytes)?;
            content.manifest = manifest;
            let event = engine.append(
                &it.space,
                &author,
                EventBody::ItemVersioned {
                    item: self.item.clone(),
                    content: ItemContent::Page(content),
                    note: note.into(),
                },
            )?;
            let reference = match &self.journal {
                SessionJournal::Plan(journal) => {
                    let mut journal = journal.clone();
                    let commit = PlanCommitRef {
                        client_id: event.client_id.clone(),
                        event_id: plan::source(&it.space, &self.item, &event, false).event_id,
                        published_revision_base64: revision
                            .ok_or_else(|| invalid("missing Plan publication revision"))?,
                        published_content_hash: hash,
                    };
                    journal.commit = Some(commit.clone());
                    super::drafts::tagged::save(engine, &it, &journal)?;
                    (SessionJournal::Plan(journal), CommitRef::Plan(commit))
                }
                SessionJournal::BinaryPage(journal) => {
                    let mut journal = journal.clone();
                    journal.commit = Some(event.client_id.clone());
                    engine.save_page_journal(&it, &journal)?;
                    (
                        SessionJournal::BinaryPage(journal),
                        CommitRef::BinaryPage {
                            client_id: event.client_id,
                        },
                    )
                }
            };
            engine
                .store
                .conn()
                .execute_batch("RELEASE page_session_commit")
                .map_err(sql_error)?;
            Ok(reference)
        })();
        let (journal, reference) = match staged {
            Ok(value) => value,
            Err(error) => {
                engine
                    .store
                    .conn()
                    .execute_batch("ROLLBACK TO page_session_commit; RELEASE page_session_commit")
                    .map_err(sql_error)?;
                engine.reload()?;
                return Err(error);
            }
        };
        self.journal = journal;
        #[cfg(test)]
        if self.lose_commit_response {
            self.lose_commit_response = false;
            return Err(CoreError::Storage {
                message: "injected lost response after durable publication".into(),
            });
        }
        Ok(CommitStatus::Queued(reference))
    }
}

#[cfg(test)]
mod tests;

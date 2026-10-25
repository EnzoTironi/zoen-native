//! Reading operations on the original `PageSession` state.

use super::super::doc_err;
use super::*;

impl PageSession {
    pub(super) fn pending(&self, engine: &Engine) -> R<bool> {
        let space = &engine.item_state(&self.item)?.space;
        Ok(engine.store.outbox()?.iter().any(|p| !p.failed && p.event.space == *space
            && matches!(&p.event.body, EventBody::ItemCreated { item, .. } | EventBody::ItemVersioned { item, .. } | EventBody::ItemReverted { item, .. } if item == &self.item)))
    }

    pub(super) fn save_error(&self, engine: &Engine) -> R<Option<String>> {
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

    pub(super) fn read_current(&self, engine: &Engine) -> R<SessionRead> {
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
                    doc.content_hash().map_err(doc_err)?,
                    !doc.editor_content_eq(shadow).map_err(doc_err)?
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
                ) => super::super::drafts::tagged::context(
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
            input_pending: self.input_state().capture.is_some(),
            input_revision: self.input_state().revision,
            pending_sync: self.pending(engine)?,
            can_edit,
            ready: !matches!(self.blocked, Some(SessionBlock::Waiting(_))),
            save_error,
        })
    }

    /// Pending legacy intent can use its existing writer; unreadable journals cannot.
    pub(in crate::pages) fn legacy_read_blocked(&self) -> bool {
        matches!(
            self.blocked,
            Some(SessionBlock::Waiting(_) | SessionBlock::DraftLocked(_))
        )
    }

    pub(in crate::pages) fn read(&mut self, engine: &Engine) -> R<SessionRead> {
        self.refresh(engine)?;
        self.read_current(engine)
    }
}

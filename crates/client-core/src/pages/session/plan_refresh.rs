//! Plan refresh operations on the original PageSession state.

use super::*;

impl PageSession {
    pub(super) fn refresh_plan(&mut self, engine: &Engine, tag: &PlanOrigin) -> R<()> {
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
        let loaded = super::super::drafts::tagged::load(engine, it, tag);
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
                    documents::plan::MAX_PLAN_PACKET_BYTES,
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
            super::super::drafts::tagged::save(engine, it, &journal)?;
        }
        self.shadow = SessionDocument::Plan(shadow);
        self.plan_history = history;
        self.live = SessionDocument::Plan(live);
        self.journal = SessionJournal::Plan(Box::new(journal));
        self.blocked = block;
        Ok(())
    }
}

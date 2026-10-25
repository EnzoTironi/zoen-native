//! Publication operations on the original `PageSession` state.

use super::*;

impl PageSession {
    pub(in crate::pages) fn commit(&mut self, engine: &mut Engine, note: &str) -> R<CommitStatus> {
        self.refresh(engine)?;
        let author = self.write_author(engine)?;
        self.input_state().require_empty()?;
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
                live.editor_content_eq(shadow)
            }
            _ => return Err(invalid("session document kinds differ")),
        };
        if unchanged {
            match &self.journal {
                SessionJournal::Plan(journal) if journal.commit.is_none() && journal.has_draft => {
                    let mut journal = journal.clone();
                    journal.has_draft = false;
                    super::super::drafts::tagged::save(engine, &it, &journal)?;
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
            let path = match (&self.tag, &it.versions.initial().content) {
                (DocumentTag::BinaryPageV1, ItemContent::Page(page)) => page.path.as_str(),
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
                    super::super::drafts::tagged::save(engine, &it, &journal)?;
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

//! One native command on the existing PageSession and encrypted journal.
use super::*;
use crate::{
    agent_page_tools::{self as tools, ReplaceBlock},
    agent_pages::{AgentPageDelivery, AgentPageResult, AgentPageSnapshot},
};

fn event_intent(event: &Event) -> R<String> {
    Ok(tools::digest(
        &serde_json::to_vec(&(
            "zoen.native-page.publication.v1",
            &event.space,
            &event.author,
            &event.device,
            &event.client_id,
            event.at_ms,
            &event.body,
        ))
        .map_err(|_| invalid("invalid Page publication"))?,
    ))
}

impl PageSession {
    fn agent_validate_edit(&self, command: &ReplaceBlock, base: &AgentPageSnapshot) -> R<()> {
        let SessionDocument::BinaryPage(shadow) = &self.shadow else {
            return Err(stale());
        };
        let desired = shadow
            .at(&shadow.frontiers())
            .map_err(super::super::doc_err)?;
        let blocks = desired.blocks();
        let kind = blocks
            .iter()
            .find(|b| b.id == command.before.id)
            .ok_or_else(stale)?
            .kind
            .clone();
        let order: Vec<_> = base.blocks.iter().map(|b| b.id.clone()).collect();
        desired
            .apply(
                &order,
                &[BlockEdit {
                    id: command.after.id.clone(),
                    kind,
                    text: command.after.text.clone(),
                    spans: command
                        .after
                        .spans
                        .iter()
                        .map(|span| documents::Span {
                            start: span.start,
                            end: span.end,
                            key: span.key.clone(),
                            value: span.value.clone(),
                        })
                        .collect(),
                }],
            )
            .map_err(super::super::doc_err)?;
        let actual: Vec<_> = desired
            .blocks()
            .iter()
            .map(super::super::block_dto)
            .collect();
        let expected: Vec<_> = base
            .blocks
            .iter()
            .map(|b| {
                if b.id == command.before.id {
                    command.after.clone()
                } else {
                    b.clone()
                }
            })
            .collect();
        if actual != expected {
            return Err(invalid(
                "the native Page cannot preserve this exact reviewed block",
            ));
        }
        Ok(())
    }
    fn agent_shadow(&mut self, engine: &Engine, clean: bool) -> R<AgentPageSnapshot> {
        self.refresh(engine)?;
        if self.tag != DocumentTag::BinaryPageV1 {
            return Err(invalid("only ordinary Pages are supported"));
        }
        let it = engine.item_state(&self.item)?;
        if matches!(self.blocked, Some(SessionBlock::Waiting(_))) {
            return Err(invalid("the published Page is still arriving"));
        }
        // State may include optimistic versions. None is a published read base.
        if engine
            .store
            .outbox()?
            .iter()
            .any(|p| p.event.space == it.space && plan::item_event(&p.event, &it.id))
        {
            return Err(invalid("an earlier Page publication needs attention"));
        }
        if engine.account().is_some_and(|a| a.registered)
            && (!engine.net.synced.contains(&it.space)
                || !engine.store.synced_spaces()?.contains(&it.space))
        {
            return Err(invalid("the Page requires durable relay-ordered readiness"));
        }
        let (
            SessionDocument::BinaryPage(shadow),
            SessionDocument::BinaryPage(live),
            SessionJournal::BinaryPage(journal),
        ) = (&self.shadow, &self.live, &self.journal)
        else {
            return Err(invalid("invalid Page session"));
        };
        if clean
            && (self.blocked.is_some()
                || journal.has_draft
                || journal.commit.is_some()
                || journal.input.binding.is_some()
                || journal.input.capture.is_some()
                || live.content_hash() != shadow.content_hash()
                // A reverted/no-op human draft can have equal semantic content
                // but still contain private Loro operations. Never publish those.
                || live.version() != shadow.version())
        {
            return Err(invalid(
                "private editor input or a Page draft must be resolved first",
            ));
        }
        let version = it.current().number;
        let snapshot = AgentPageSnapshot {
            artifact_id: it.id.clone(),
            space_id: it.space.clone(),
            actor_id: engine.me_id()?,
            version,
            prefix_sha256: tools::prefix(&engine.state, &it.id, version)
                .map_err(|reason| invalid(&reason))?,
            content_sha256: shadow.content_hash(),
            title: match &it.current().content {
                ItemContent::Page(p) => p.title.clone(),
                _ => return Err(stale()),
            },
            blocks: shadow
                .blocks()
                .iter()
                .map(super::super::block_dto)
                .collect(),
        };
        if serde_json::to_vec(&snapshot)
            .map_err(|_| invalid("invalid Page snapshot"))?
            .len()
            > tools::SNAPSHOT_LIMIT
        {
            return Err(invalid(
                "the published Page exceeds the native tool snapshot capacity",
            ));
        }
        Ok(snapshot)
    }

    fn agent_result(&self, engine: &Engine, id: &str, intent: &str) -> R<Option<AgentPageResult>> {
        let SessionJournal::BinaryPage(journal) = &self.journal else {
            return Err(invalid("invalid Page journal"));
        };
        let Some(receipt) = journal.receipt(id) else {
            return Ok(None);
        };
        let publication = receipt
            .publication
            .as_ref()
            .filter(|p| p.intent == intent)
            .ok_or_else(stale)?;
        let me = engine.me_id()?;
        let author = engine.author_for(&me)?;
        let it = engine.item_state(&self.item)?;
        if publication.author != me
            || publication.device != author.device.clone().unwrap_or_default()
            || publication.space != it.space
            || publication.applied_content_sha256 != receipt.content_hash
            || publication.observed_base_version == 0
            || publication.observed_base_version >= it.current().number
            || !tools::valid_digest(&publication.intent)
            || !tools::valid_digest(&publication.event_intent)
            || !tools::valid_digest(&publication.applied_content_sha256)
            || publication.client_id.is_empty()
        {
            return Err(stale());
        }
        let queue = engine.store.outbox()?;
        let pending = queue
            .iter()
            .find(|p| p.event.space == it.space && p.event.client_id == publication.client_id);
        // CID alone is not an authenticated intent (#139). A valid peer alias
        // remains in history and cannot confirm or mask the actual later echo.
        let mut confirmed = false;
        if let Some(log) = engine.logs.get(&it.space) {
            for event in log.events().iter().filter(|event| {
                event.client_id == publication.client_id
                    && event.author == publication.author
                    && event.device.as_deref() == Some(publication.device.as_str())
            }) {
                if event_intent(event)? == publication.event_intent {
                    confirmed = true;
                    break;
                }
            }
        }
        let delivery = if let Some(pending) = pending {
            if event_intent(&pending.event)? != publication.event_intent {
                return Err(stale());
            }
            if pending.failed {
                AgentPageDelivery::Refused
            } else {
                AgentPageDelivery::Queued
            }
        } else if confirmed {
            AgentPageDelivery::Confirmed
        } else {
            AgentPageDelivery::Unknown
        };
        Ok(Some(AgentPageResult {
            artifact_id: self.item.clone(),
            actor_id: me,
            device_id: publication.device.clone(),
            observed_base_version: publication.observed_base_version,
            current_version: it.current().number,
            applied_content_sha256: publication.applied_content_sha256.clone(),
            client_id: publication.client_id.clone(),
            delivery,
            replayed: true,
        }))
    }
}

impl Engine {
    pub(crate) fn agent_page_snapshot(&self, item: &str) -> R<AgentPageSnapshot> {
        self.with_page(item, |_, session| session.agent_shadow(self, false))
    }

    pub(crate) fn agent_page_snapshot_at(&self, item: &str, version: u32) -> R<AgentPageSnapshot> {
        self.with_page(item, |it, session| {
            session.refresh(self)?;
            if session.tag != DocumentTag::BinaryPageV1 {
                return Err(invalid("only ordinary Pages are supported"));
            }
            let page = session.binary_at(version)?;
            let result = AgentPageSnapshot {
                artifact_id: item.into(),
                space_id: it.space.clone(),
                actor_id: self.me_id()?,
                version,
                prefix_sha256: tools::prefix(&self.state, item, version)
                    .map_err(|reason| invalid(&reason))?,
                content_sha256: page.content_hash(),
                title: match &it.versions[version as usize - 1].content {
                    ItemContent::Page(p) => p.title.clone(),
                    _ => return Err(stale()),
                },
                blocks: page.blocks().iter().map(super::super::block_dto).collect(),
            };
            if serde_json::to_vec(&result)
                .map_err(|_| invalid("invalid Page snapshot"))?
                .len()
                > tools::SNAPSHOT_LIMIT
            {
                return Err(invalid("the Page snapshot exceeds its capacity"));
            }
            Ok(result)
        })
    }

    pub(crate) fn agent_page_validate_write(&self, command: &ReplaceBlock) -> R<()> {
        self.with_page(&command.artifact_id, |_, session| {
            session.write_author(self)?;
            let base = session.agent_shadow(self, true)?;
            if base.version != command.version
                || !tools::prefix_matches(
                    &self.state,
                    &command.artifact_id,
                    command.version,
                    &command.prefix_sha256,
                )
                || base.content_sha256 != command.content_sha256
                || base
                    .blocks
                    .iter()
                    .find(|block| block.id == command.before.id)
                    != Some(&command.before)
            {
                return Err(stale());
            }
            session.agent_validate_edit(command, &base)?;
            Ok(())
        })
    }

    pub(crate) fn agent_page_command_result(
        &self,
        item: &str,
        id: &str,
        intent: &str,
    ) -> R<Option<AgentPageResult>> {
        self.with_page(item, |_, session| {
            session.refresh(self)?;
            session.agent_result(self, id, intent)
        })
    }

    pub(crate) fn agent_page_apply_commit(
        &mut self,
        command: &ReplaceBlock,
        id: &str,
        intent: &str,
    ) -> R<AgentPageResult> {
        let item = &command.artifact_id;
        // No pages mutex crosses append/reload. There is still exactly one owner.
        let cached = self
            .pages
            .sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(item);
        let mut session = cached.map_or_else(|| PageSession::open(self, item), Ok)?;
        let result = self.atomic_agent_action(|engine| {
            session.refresh(engine)?;
            session.write_author(engine)?;
            if let Some(result) = session.agent_result(engine, id, intent)? {
                return Ok(result);
            }
            let base = session.agent_shadow(engine, true)?;
            if base.version != command.version
                || !tools::prefix_matches(
                    &engine.state,
                    item,
                    command.version,
                    &command.prefix_sha256,
                )
                || base.content_sha256 != command.content_sha256
                || base.blocks.iter().find(|b| b.id == command.before.id) != Some(&command.before)
            {
                return Err(stale());
            }
            session.agent_validate_edit(command, &base)?;
            let SessionJournal::BinaryPage(journal) = &session.journal else {
                return Err(invalid("invalid Page journal"));
            };
            let SessionDocument::BinaryPage(shadow) = &session.shadow else {
                return Err(invalid("invalid Page shadow"));
            };
            let context = engine.page_context_at(
                engine.item_state(item)?,
                journal.generation,
                shadow.frontiers().encode(),
            )?;
            let order: Vec<_> = base.blocks.iter().map(|b| b.id.clone()).collect();
            let accepted = session.apply_from(
                engine,
                id,
                &context,
                DocumentEdit::BinaryPage {
                    order: &order,
                    changed: std::slice::from_ref(&command.after),
                },
            )?;
            let CommitStatus::Queued(CommitRef::BinaryPage { client_id }) =
                session.commit(engine, "Page block updated by reviewed agent")?
            else {
                return Err(invalid("the Page command did not create one publication"));
            };
            let event = engine
                .store
                .outbox()?
                .into_iter()
                .find(|p| p.event.space == base.space_id && p.event.client_id == client_id)
                .map(|p| p.event)
                .or_else(|| {
                    engine
                        .logs
                        .get(&base.space_id)
                        .and_then(|log| {
                            log.events()
                                .iter()
                                .find(|event| event.client_id == client_id)
                        })
                        .cloned()
                })
                .ok_or_else(stale)?;
            let publication = drafts::PagePublication {
                intent: intent.into(),
                author: event.author.clone(),
                device: event.device.clone().unwrap_or_default(),
                space: event.space.clone(),
                observed_base_version: base.version,
                applied_content_sha256: accepted.applied_content_hash,
                client_id,
                event_intent: event_intent(&event)?,
            };
            let SessionJournal::BinaryPage(journal) = &mut session.journal else {
                return Err(invalid("invalid Page journal"));
            };
            journal.attach_publication(id, publication)?;
            engine.save_page_journal(engine.item_state(item)?, journal)?;
            let mut result = session
                .agent_result(engine, id, intent)?
                .ok_or_else(stale)?;
            result.replayed = false;
            Ok(result)
        });
        // Rollback reload invalidates every cached producer; never reinsert a
        // detached session that still contains the rolled-back command.
        if result.is_ok() {
            self.index_dirty = true;
            self.pages
                .sessions
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(item.clone(), session);
        }
        result
    }
}

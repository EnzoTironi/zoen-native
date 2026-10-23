//! Authority operations on the original PageSession state.

use super::*;

impl PageSession {
    pub(super) fn write_author(&self, engine: &Engine) -> R<String> {
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

    pub(in crate::pages) fn open(engine: &Engine, id: &str) -> R<Self> {
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
                SessionJournal::BinaryPage(Box::default()),
            ),
            _ => return Err(invalid("this Item is not a Page or Plan")),
        };
        let shadow = match &document {
            SessionDocument::Plan(doc) => SessionDocument::Plan(doc.fork().map_err(plan_error)?),
            SessionDocument::BinaryPage(doc) => SessionDocument::BinaryPage(
                doc.at(&doc.frontiers()).map_err(super::super::doc_err)?,
            ),
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

    pub(in crate::pages) fn refresh(&mut self, engine: &Engine) -> R<()> {
        if self.owner != engine.me {
            *self = Self::open(engine, &self.item)?;
            return Ok(());
        }
        match self.tag.clone() {
            DocumentTag::PlanV1(tag) => self.refresh_plan(engine, &tag),
            DocumentTag::BinaryPageV1 => self.refresh_binary(engine),
        }
    }

    pub(in crate::pages) fn plan_snapshot(&mut self, engine: &Engine) -> R<plan::PlanSnapshot> {
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
}

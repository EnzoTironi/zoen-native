//! Existing currentness on the same canonical State authority.

use super::{
    content_hash, ActionClass, AgentRequest, Currentness, ItemContent, ItemKind, PlanDoc, PlanRead,
    Role, State,
};

impl State {
    pub(crate) fn writer(&self, space: &str, identity: &str) -> bool {
        self.spaces.get(space).is_some_and(|s| {
            s.members
                .iter()
                .any(|(id, role)| id == identity && *role != Role::Reader)
        })
    }

    pub(crate) fn request_currentness(
        &self,
        req: &AgentRequest,
        space: &str,
        plan: &PlanRead,
    ) -> Currentness {
        if req.tool_proposal.is_some() {
            return if self.tool_request_matches(req, space) {
                Currentness::Current
            } else {
                Currentness::Changed
            };
        }
        if matches!(plan, PlanRead::Waiting)
            && req.proposal.is_none()
            && self.writer(space, &req.agent)
            && matches!((&req.item, &req.line), (Some(item), Some(_))
                if self.items.get(item).is_some_and(|item| item.space == space))
        {
            return Currentness::Waiting;
        }
        let document = match plan {
            PlanRead::Ready { document, .. } => Some(document),
            _ => None,
        };
        if self.request_matches(req, space, document) {
            Currentness::Current
        } else {
            Currentness::Changed
        }
    }

    pub(super) fn request_matches(
        &self,
        req: &AgentRequest,
        space: &str,
        read_plan: Option<&PlanDoc>,
    ) -> bool {
        if req.tool_proposal.is_some() {
            return self.tool_request_matches(req, space);
        }
        if !self.writer(space, &req.agent) {
            return false;
        }
        if let Some(p) = &req.proposal {
            let valid_content = match (&p.kind, &p.content) {
                (ItemKind::Plan, ItemContent::Plan(doc)) => {
                    doc.sections.iter().any(|s| !s.lines.is_empty())
                }
                (ItemKind::App, ItemContent::App(doc)) => {
                    crate::apps::spec(&doc.app).is_some_and(|spec| {
                        doc.resource_uri == spec.resource_uri
                            && serde_json::from_str::<serde_json::Value>(&doc.state_json).is_ok()
                    })
                }
                _ => false,
            };
            return req.action == ActionClass::Reversible
                && req.item.is_none()
                && req.line.is_none()
                && p.ai_cost_cents >= 0
                && !p.item.is_empty()
                && valid_content
                && !self.items.contains_key(&p.item)
                && req.proposal_hash(space).as_deref() == Some(req.content_hash.as_str());
        }
        match (&req.item, &req.line) {
            (Some(item), Some(line)) => self.items.get(item).is_some_and(|it| {
                it.space == space
                    && read_plan.is_some_and(|plan| {
                        plan.line(line).is_some_and(|line| {
                            content_hash(&(&line.text, line.cost_cents))
                                .is_ok_and(|hash| hash == req.content_hash)
                        })
                    })
            }),
            (None, None) => {
                // Old creation cards did not retain their output. They can be denied,
                // but cannot claim to create a document that was never persisted.
                req.action != ActionClass::Reversible
                    || content_hash(&(&req.title, &req.detail, &req.audience))
                        .is_ok_and(|hash| hash == req.content_hash)
            }
            _ => false,
        }
    }
}

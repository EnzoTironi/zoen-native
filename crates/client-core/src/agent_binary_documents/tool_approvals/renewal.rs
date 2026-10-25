//! An owner requests preparation, never renewal of an execution permission.
use super::*;
use crate::engine::{RequestState, State};
use types::{Event, Identity, IdentityKind, ToolReviewLineage};

#[derive(Clone, Debug)]
pub(crate) struct ReviewIntent {
    pub id: String,
    pub at_ms: i64,
    pub successor: Option<String>,
}

fn root(r: &RequestState) -> &str {
    r.req
        .tool_proposal
        .as_ref()
        .and_then(|p| p.review.as_ref())
        .map_or(r.req.id.as_str(), |lineage| lineage.root.as_str())
}

impl State {
    /// Same canonical order used by normal host continuation discovery. No wall
    /// clock, request ID or caller-supplied generation can create a newer leaf.
    pub(crate) fn tool_approval_order(&self, request: &AgentRequest) -> (u32, u32, usize) {
        let p = request.tool_proposal.as_ref();
        (
            p.map_or(0, |p| p.step),
            p.and_then(|p| p.review.as_ref())
                .map_or(0, |r| r.generation),
            self.request_order
                .iter()
                .position(|id| id == &request.id)
                .unwrap_or(0),
        )
    }

    pub(crate) fn tool_review_active_leaf(&self, request: &RequestState) -> bool {
        let Some(p) = request.req.tool_proposal.as_ref() else {
            return false;
        };
        self.requests
            .values()
            .filter(|r| {
                r.space == request.space
                    && r.req.agent == request.req.agent
                    && r.req.tool_proposal.as_ref().is_some_and(|other| {
                        other.source == p.source && other.task_sha256 == p.task_sha256
                    })
            })
            .max_by_key(|r| self.tool_approval_order(&r.req))
            .is_some_and(|leaf| leaf.req.id == request.req.id)
    }

    /// Legacy roots retain their existing approval/execution semantics. A renewed
    /// request can authorize fresh IO only while it is the same task's active leaf.
    pub(crate) fn tool_review_current(&self, r: &RequestState) -> bool {
        r.req
            .tool_proposal
            .as_ref()
            .is_none_or(|p| p.review.is_none())
            || self.tool_review_active_leaf(r)
    }
    pub(crate) fn tool_review_source_replied(&self, r: &RequestState) -> bool {
        let Some(p) = r.req.tool_proposal.as_ref() else {
            return true;
        };
        self.spaces.get(&r.space).is_none_or(|space| {
            space.entries.iter().any(|entry| {
                entry.author == r.req.agent
                    && matches!(&entry.body, EntryBody::Message { reply: Some(reply), .. }
                        if reply.to == p.source)
            })
        })
    }

    pub(crate) fn tool_review_intent_valid(
        &self,
        event: &Event,
        request: &str,
        observed: &str,
        identities: &std::collections::HashMap<String, Identity>,
    ) -> bool {
        let Some(r) = self.requests.get(request) else {
            return false;
        };
        let Some(p) = r.req.tool_proposal.as_ref() else {
            return false;
        };
        r.space == event.space
            && r.req.content_hash == observed
            && r.status == ReqStatus::Approved
            && r.resolution.as_ref().and_then(|r| r.tool_decision)
                == Some(ToolDecision::ApproveOnce)
            && r.review.is_none()
            && self.tool_review_active_leaf(r)
            && p.expires_at_ms <= event.at_ms
            && event.author == p.owner
            && !event.client_id.is_empty()
            && event.client_id.len() <= 128
            && self.writer(&r.space, &event.author)
            && identities.get(&r.req.agent).is_some_and(|agent| {
                agent.kind == IdentityKind::Agent
                    && agent.owner.as_deref() == Some(p.owner.as_str())
                    && event_log::agent_owner::profile_authorized(agent)
            })
            && self.tool_request_matches(&r.req, &r.space)
            && self.tool_app_act(p, event.at_ms)
            && !self.tool_review_source_replied(r)
    }

    pub(crate) fn tool_review_successor_valid(&self, request: &AgentRequest, at: i64) -> bool {
        let Some(p) = request.tool_proposal.as_ref() else {
            return true;
        };
        let Some(lineage) = p.review.as_ref() else {
            // A fresh legacy root cannot bypass an explicit renewal lineage.
            return !self.requests.values().any(|old| {
                old.req.agent == request.agent
                    && old.req.tool_proposal.as_ref().is_some_and(|prior| {
                        prior.task_sha256 == p.task_sha256
                            && prior.step == p.step
                            && (prior.review.is_some() || old.review.is_some())
                    })
            });
        };
        let Some(prior) = self.requests.get(&lineage.predecessor) else {
            return false;
        };
        let Some(old) = prior.req.tool_proposal.as_ref() else {
            return false;
        };
        let Some(intent) = prior.review.as_ref() else {
            return false;
        };
        let Some(generation) = old
            .review
            .as_ref()
            .map_or(Some(1), |r| r.generation.checked_add(1))
        else {
            return false;
        };
        if !self.tool_review_active_leaf(prior)
            || prior.status != ReqStatus::Approved
            || intent.successor.is_some()
            || intent.id != lineage.intent
            || intent.at_ms > at
            || root(prior) != lineage.root
            || lineage.generation != generation
            || old.expires_at_ms > intent.at_ms
        {
            return false;
        }
        let mut expected = prior.req.clone();
        expected.id = request.id.clone();
        expected.content_hash = request.content_hash.clone();
        let Some(expected_proposal) = expected.tool_proposal.as_mut() else {
            return false;
        };
        expected_proposal.expires_at_ms = p.expires_at_ms;
        expected_proposal.review = Some(lineage.clone());
        expected == *request
    }

    pub(crate) fn tool_review_successor_opened(&mut self, request: &AgentRequest) {
        if let Some(lineage) = request
            .tool_proposal
            .as_ref()
            .and_then(|p| p.review.as_ref())
        {
            if let Some(intent) = self
                .requests
                .get_mut(&lineage.predecessor)
                .and_then(|r| r.review.as_mut())
            {
                intent.successor = Some(request.id.clone());
            }
        }
    }
}

impl Engine {
    /// A renewed approval cannot execute from an optimistic ordering that may
    /// lose to a different first valid owner intent/successor at the relay.
    pub(crate) fn tool_review_confirmed(&self, r: &RequestState) -> bool {
        let Some(lineage) = r.req.tool_proposal.as_ref().and_then(|p| p.review.as_ref()) else {
            return true;
        };
        if self.approval_replay_needed || self.net.optimistic_approvals.contains(&r.space) {
            return false;
        }
        let Some(log) = self.logs.get(&r.space) else {
            return false;
        };
        let Some(p) = r.req.tool_proposal.as_ref() else {
            return false;
        };
        let predecessor_hash = self
            .state
            .requests
            .get(&lineage.predecessor)
            .map(|r| &r.req.content_hash);
        log.events().iter().any(|event| {
            event.author == p.owner && event.client_id == lineage.intent && matches!(&event.body,
                EventBody::ToolReviewRequested { request, content_hash }
                    if request == &lineage.predecessor && Some(content_hash) == predecessor_hash)
        }) && log.events().iter().any(|event| {
            event.author == r.req.agent && event.device.as_deref() == Some(p.device.as_str())
                && event_log::content_hash_of(event) == r.opened_content_hash
                && matches!(&event.body, EventBody::RequestOpened { request } if request == &r.req)
        }) && log.events().iter().any(|event| {
            event.author == p.owner && Some(event_log::content_hash_of(event)) == r.resolved_content_hash
                && matches!(&event.body, EventBody::RequestResolved { request, approved: true, content_hash, resolution }
                if request == &r.req.id && content_hash == &r.req.content_hash && resolution == &r.resolution)
        })
    }

    pub(crate) fn request_tool_review(&mut self, id: &str, observed: &str) -> R<DecideOutcome> {
        let r = self.state.requests.get(id).cloned().ok_or_else(stale)?;
        let p = r.req.tool_proposal.as_ref().ok_or_else(malformed)?;
        let me = self.me_id()?;
        if me != p.owner || observed != r.req.content_hash || !self.state.writer(&r.space, &me) {
            return Err(denied());
        }
        if let Some(intent) = &r.review {
            let current = intent
                .successor
                .as_ref()
                .and_then(|id| self.state.requests.get(id))
                .unwrap_or(&r);
            if !self.state.tool_review_active_leaf(current) {
                return Err(stale());
            }
            return Ok(DecideOutcome {
                request: self.request_dto(current),
                message: crate::i18n::t(
                    "A nova revisão já foi solicitada.",
                    "The new review was already requested.",
                ),
                standing_grant_id: None,
                also_resolved: 0,
            });
        }
        let at = crate::engine::now_ms();
        // Use the exact replay guard before append; it is checked again by projection.
        if self.approval_replay_needed
            || !self.state.tool_review_active_leaf(&r)
            || r.status != ReqStatus::Approved
            || p.expires_at_ms > at
            || !self.state.tool_request_matches(&r.req, &r.space)
            || !self.state.tool_app_act(p, at)
            || !self.tool_non_blocked(&r.req)
            || self.state.tool_review_source_replied(&r)
        {
            return Err(stale());
        }
        self.append(
            &r.space,
            &me,
            EventBody::ToolReviewRequested {
                request: id.into(),
                content_hash: observed.into(),
            },
        )?;
        Ok(DecideOutcome {
            request: self.request_dto(
                self.state
                    .requests
                    .get(id)
                    .filter(|r| r.review.is_some())
                    .ok_or_else(stale)?,
            ),
            message: crate::i18n::t(
                "Nova revisão solicitada. Aguarde o pedido do agente antes de aprovar.",
                "New review requested. Wait for the agent's request before approving.",
            ),
            standing_grant_id: None,
            also_resolved: 0,
        })
    }

    pub(crate) fn tool_review_dto(&self, r: &RequestState, p: &ToolProposal) -> ToolReviewDto {
        let state = if let Some(intent) = &r.review {
            if intent.successor.is_some() {
                ToolReviewState::Superseded
            } else if !self.state.tool_review_active_leaf(r)
                || self.state.tool_review_source_replied(r)
            {
                ToolReviewState::Reconcile
            } else {
                ToolReviewState::Requested
            }
        } else if self.me_id().as_deref().is_ok_and(|me| me == p.owner)
            && r.status == ReqStatus::Approved
            && self.state.tool_review_active_leaf(r)
            && p.expires_at_ms <= crate::engine::now_ms()
            && !self.approval_replay_needed
            && self.state.tool_request_matches(&r.req, &r.space)
            && self.state.tool_app_act(p, crate::engine::now_ms())
            && self.tool_non_blocked(&r.req)
            && !self.state.tool_review_source_replied(r)
        {
            ToolReviewState::Available
        } else {
            ToolReviewState::Unavailable
        };
        ToolReviewDto {
            state,
            root_request_id: root(r).into(),
            predecessor_request_id: p.review.as_ref().map(|r| r.predecessor.clone()),
            successor_request_id: r.review.as_ref().and_then(|r| r.successor.clone()),
        }
    }
}

mod staging;

//! Existing projection on the same canonical State authority.

use super::*;

impl State {
    pub(crate) fn tool_request_matches(&self, req: &AgentRequest, space: &str) -> bool {
        let Some(p) = &req.tool_proposal else {
            return false;
        };
        if p.schema == 2 {
            return crate::agent_pages::approvals::matches(self, req, space);
        }
        let Ok((ask, _)) = parsed(p) else {
            return false;
        };
        let Some((source_sha, inbound_sha)) = source_hash(self, space, &p.source, &p.serving)
        else {
            return false;
        };
        let Some(item) = self.items.get(&p.artifact) else {
            return false;
        };
        let ItemContent::App(app) = &item.current().content else {
            return false;
        };
        req.proposal.is_none()
            && req.item.is_none()
            && req.line.is_none()
            && req.action == ActionClass::Reversible
            && self.writer(space, &req.agent)
            && self.writer(space, &p.owner)
            && self.writer(space, &p.serving)
            && p.serving != req.agent
            && item.space == space
            && item.current().number == p.version
            && app.app == deckcraft::APP
            && app.resource_uri == deckcraft::RESOURCE
            && app.document.as_ref().is_some_and(|file| {
                file.sha256 == p.sha256 && file.bytes <= AGENT_BINARY_DOCUMENT_LIMIT as u64
            })
            && source_sha == p.source_sha256
            && req.tool_proposal_hash(space).as_deref() == Some(req.content_hash.as_str())
            && p.task_sha256
                == sha(
                    &serde_json::to_vec(&("zoen.tool-task.v1", space, &req.agent, &p.source))
                        .unwrap_or_default(),
                )
            && ask["envelope"]
                == json!({"agent":req.agent,"device":p.device,"owner":p.owner,"space":space,"from":p.serving,"inbound_sha256":inbound_sha})
    }

    pub(crate) fn tool_app_act(&self, p: &ToolProposal, at: i64) -> bool {
        if p.schema == 2 {
            return crate::agent_pages::approvals::access(self, p);
        }
        self.grants
            .iter()
            .rev()
            .filter(|g| {
                !g.revoked
                    && g.grant.scope == GrantScope::Item(p.artifact.clone())
                    && g.grant.grantee.as_deref() == Some(format!("app:{}", p.artifact).as_str())
                    && g.grant.expires_at_ms.is_none_or(|expiry| expiry > at)
            })
            .find_map(|g| {
                if let Capability::Trust(level) = g.grant.capability {
                    Some(level)
                } else {
                    None
                }
            })
            .is_some_and(|level| level >= TrustLevel::Act)
    }
}

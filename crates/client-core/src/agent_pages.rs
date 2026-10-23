//! Rust-only Page tools on the existing signed log and document session.
use crate::{
    engine::{Engine, R},
    Client, CoreError, PageBlockDto,
};
use serde::{Deserialize, Serialize};

pub(crate) mod approvals;
mod publication;
mod recovery;
#[cfg(test)]
mod tests;

fn stale() -> CoreError {
    CoreError::Stale {
        reason: "The reviewed Page, source or access changed. Read it again.".into(),
    }
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: "Unsupported or invalid native Page command.".into(),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentPageSnapshot {
    pub artifact_id: String,
    pub space_id: String,
    pub actor_id: String,
    pub version: u32,
    pub prefix_sha256: String,
    pub content_sha256: String,
    pub title: String,
    pub blocks: Vec<PageBlockDto>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AgentPageDelivery {
    Queued,
    Confirmed,
    Refused,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentPageResult {
    pub artifact_id: String,
    pub actor_id: String,
    pub device_id: String,
    /// Published base observed before the local command, not its final relay ordinal.
    pub observed_base_version: u32,
    pub current_version: u32,
    /// Local accepted semantic result; a concurrent peer can change the merged head.
    pub applied_content_sha256: String,
    pub client_id: String,
    pub delivery: AgentPageDelivery,
    pub replayed: bool,
}

/// An ephemeral, non-serializable turn. Every boundary rechecks current authority.
pub struct AgentPageTurn {
    pub(crate) snapshot: AgentPageSnapshot,
    pub(crate) device: String,
    pub(crate) owner: String,
    pub(crate) source: String,
    pub(crate) serving: String,
    pub(crate) source_sha256: String,
    pub(crate) approval: Option<(String, String)>,
}
impl AgentPageTurn {
    pub fn snapshot(&self) -> &AgentPageSnapshot {
        &self.snapshot
    }
}

impl Engine {
    pub(crate) fn recheck_agent_page_turn(&self, turn: &AgentPageTurn, write: bool) -> R<()> {
        let (device, owner, space, source) = self.agent_binary_authority(
            &turn.snapshot.artifact_id,
            &turn.source,
            &turn.serving,
            write,
        )?;
        if self.me_id()? != turn.snapshot.actor_id
            || device != turn.device
            || owner != turn.owner
            || space != turn.snapshot.space_id
            || source != turn.source_sha256
        {
            return Err(stale());
        }
        if write
            && (!self.state.writer(&space, &turn.snapshot.actor_id)
                || !self.state.writer(&space, &turn.owner))
        {
            return Err(stale());
        }
        let action = if write {
            types::ActionClass::Reversible
        } else {
            types::ActionClass::Reply
        };
        if matches!(
            self.decide(&turn.snapshot.actor_id, &space, &action, 0),
            grants::Decision::Block(_)
        ) {
            return Err(stale());
        }
        Ok(())
    }
}

impl Client {
    pub fn agent_page_turn(&self, item: &str, source: &str, serving: &str) -> R<AgentPageTurn> {
        let e = self.lock();
        let (device, owner, _, source_sha256) =
            e.agent_binary_authority(item, source, serving, false)?;
        let snapshot = e.agent_page_snapshot(item)?;
        let turn = AgentPageTurn {
            snapshot,
            device,
            owner,
            source: source.into(),
            serving: serving.into(),
            source_sha256,
            approval: None,
        };
        e.recheck_agent_page_turn(&turn, false)?;
        Ok(turn)
    }

    pub fn agent_page_finish_read(&self, turn: &AgentPageTurn) -> R<AgentPageSnapshot> {
        let e = self.lock();
        e.recheck_agent_page_turn(turn, false)?;
        let current = e.agent_page_snapshot(&turn.snapshot.artifact_id)?;
        if current != turn.snapshot {
            return Err(stale());
        }
        if let Some((id, hash)) = &turn.approval {
            let (r, p) = e.tool_resolution(id, hash)?;
            if p.canonical_tool != "read_page"
                || p.sha256 != current.prefix_sha256
                || p.expires_at_ms <= crate::engine::now_ms()
                || !e.state.tool_review_current(r)
                || !e.state.tool_request_matches(&r.req, &r.space)
            {
                return Err(stale());
            }
        }
        Ok(current)
    }
}

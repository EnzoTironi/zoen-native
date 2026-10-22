use super::*;
use crate::agent_page_tools::{self as tools, ReplaceBlock};

impl Client {
    /// Only an exact current canonical ApproveOnce can publish a Page command.
    pub fn agent_page_replace(
        &self,
        turn: &AgentPageTurn,
        command: &ReplaceBlock,
    ) -> R<AgentPageResult> {
        let args = serde_json::to_value(command).map_err(|_| invalid())?;
        tools::parse("replace_page_block", &args).map_err(|_| invalid())?;
        let mut e = self.lock();
        e.recheck_agent_page_turn(turn, true)?;
        let (request, hash) = turn.approval.as_ref().ok_or_else(stale)?;
        let (_, proposal) = e.tool_resolution(request, hash)?;
        if proposal.schema != 2
            || proposal.canonical_tool != "replace_page_block"
            || proposal.arguments_json
                != miniapps::deckcraft::agent_tools::canonical_json(&args).map_err(|_| invalid())?
            || proposal.artifact != turn.snapshot.artifact_id
            || proposal.device != turn.device
            || proposal.owner != turn.owner
            || proposal.source != turn.source
            || proposal.serving != turn.serving
            || proposal.source_sha256 != turn.source_sha256
        {
            return Err(stale());
        }
        let intent = approvals::intent(proposal)?;
        let mutation = format!("pagecmd_{intent}");
        // Current source/device/permission and exact signed approval were checked
        // even for historical accepted-result recovery. This never mints a write.
        if let Some(result) =
            e.agent_page_command_result(&command.artifact_id, &mutation, &intent)?
        {
            return Ok(result);
        }
        if proposal.expires_at_ms <= crate::engine::now_ms()
            || !e
                .state
                .tool_request_matches(&e.state.requests[request].req, &turn.snapshot.space_id)
        {
            return Err(stale());
        }
        e.agent_page_apply_commit(command, &mutation, &intent)
    }
}

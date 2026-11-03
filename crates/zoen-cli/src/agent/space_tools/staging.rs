//! Stage the existing exact held call against current native source authority.
use super::*;

impl SpaceTools<'_> {
    pub(in crate::agent) fn stage_approval(
        &self,
        ask: &ToolAsk,
    ) -> Result<client_core::AgentRequestDto, String> {
        self.fresh().stage_current_approval(ask)
    }
    fn stage_current_approval(
        &self,
        ask: &ToolAsk,
    ) -> Result<client_core::AgentRequestDto, String> {
        self.check_selection()?;
        let prior = self.e.tool_approval_requests(&self.space, self.source);
        if ask.call.name == history_search::TOOL
            && self
                .names
                .iter()
                .any(|(definition, _, _)| definition.name == ask.call.name)
        {
            let dto = self
                .e
                .stage_conversation_search_approval(
                    &self.space,
                    self.source,
                    self.serving,
                    client_core::AgentToolApprovalInput {
                        canonical_tool: history_search::TOOL.into(),
                        arguments: ask.call.arguments.clone(),
                        worker_binding: client_core::agent_conversation_tools::executor_binding(),
                        continuation_json: serde_json::to_string(ask)
                            .map_err(|_| "Invalid held search")?,
                        gate_rule: ask.rule.clone(),
                    },
                )
                .map_err(|error| error.to_string())?;
            self.check_selection()?;
            return self.register_staged(dto, &prior);
        }
        if let Some((_, tool)) = self.site_binding(&ask.call) {
            let dto = self
                .e
                .stage_site_tool_approval(
                    &self.space,
                    self.source,
                    self.serving,
                    client_core::AgentToolApprovalInput {
                        canonical_tool: tool.into(),
                        arguments: ask.call.arguments.clone(),
                        worker_binding: client_core::agent_sites::executor_binding(),
                        continuation_json: serde_json::to_string(ask)
                            .map_err(|_| "Invalid Site continuation")?,
                        gate_rule: ask.rule.clone(),
                    },
                )
                .map_err(|error| error.to_string())?;
            self.check_selection()?;
            return self.register_staged(dto, &prior);
        }
        if let Some((_, _, creation_tool)) = self.names.iter().find(|(definition, _, tool)| {
            definition.name == ask.call.name && client_core::agent_app_creation::is_model_name(tool)
        }) {
            let dto = self
                .e
                .stage_app_creation_approval(
                    &self.space,
                    self.source,
                    self.serving,
                    client_core::AgentToolApprovalInput {
                        canonical_tool: creation_tool.clone(),
                        arguments: ask.call.arguments.clone(),
                        worker_binding: client_core::agent_app_creation::executor_binding_for(
                            creation_tool,
                        )
                        .ok_or("Unsupported creation tool")?,
                        continuation_json: serde_json::to_string(ask)
                            .map_err(|_| "Invalid held creation")?,
                        gate_rule: ask.rule.clone(),
                    },
                )
                .map_err(|error| error.to_string())?;
            self.check_selection()?;
            return self.register_staged(dto, &prior);
        }
        let (_, item, tool) = self
            .names
            .iter()
            .find(|(definition, _, _)| definition.name == ask.call.name)
            .ok_or("The held tool is no longer offered")?;
        if self.page_items.contains(item) && client_core::agent_page_tools::recognizes(tool) {
            let result = self
                .stage_page(item, tool, ask)
                .and_then(|dto| self.register_staged(dto, &prior));
            self.check_selection()?;
            return result;
        }
        if self.scene_items.contains(item) && client_core::agent_scenes::tools::recognizes(tool) {
            let result = self
                .stage_scene(item, tool, ask)
                .and_then(|dto| self.register_staged(dto, &prior));
            self.check_selection()?;
            return result;
        }
        if !self.deckcraft_items.contains(item) || !crate::deckcraft::recognizes(tool) {
            return Err("This tool has no supported canonical approval shape".into());
        }
        let worker = self
            .deckcraft
            .ok_or("The presentation executor is unavailable")?;
        let (version, hash, _) =
            client_core::binary_agent_tools::command(tool, &ask.call.arguments)?;
        let turn = self
            .e
            .agent_binary_turn(item, self.source, self.serving, version, hash.as_deref())
            .map_err(|e| e.to_string())?;
        let staged = self
            .e
            .stage_tool_approval(
                &turn,
                client_core::AgentToolApprovalInput {
                    canonical_tool: tool.clone(),
                    arguments: ask.call.arguments.clone(),
                    worker_binding: worker.binding_for(&turn.document().kind)?.into(),
                    continuation_json: serde_json::to_string(ask)
                        .map_err(|_| "Invalid held call")?,
                    gate_rule: ask.rule.clone(),
                },
            )
            .map_err(|e| e.to_string());
        self.check_selection()?;
        staged.and_then(|dto| self.register_staged(dto, &prior))
    }
}

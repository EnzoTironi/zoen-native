//! Native Pages share the normal host, Gate, signed review and continuation.
use super::trace::admission::{Phase, Refusal};
use super::*;
use client_core::agent_page_tools as page;

pub(super) enum CanonicalPermit {
    Conversation(client_core::agent_conversation_tools::ApprovedConversationSearch),
    Deck(client_core::ApprovedToolTurn),
    Page(client_core::ApprovedPageTurn),
    Create(client_core::agent_app_creation::ApprovedAppCreation),
    Site(client_core::agent_sites::ApprovedSiteTool),
    Scene(client_core::agent_scenes::ApprovedSceneTurn),
}
impl CanonicalPermit {
    pub(super) fn new(tools: &SpaceTools<'_>, request: &str, schema: u8) -> Result<Self, Refusal> {
        tools
            .check_selection()
            .map_err(|_| Refusal::selection(Phase::SelectionBefore))?;
        let result = match schema {
            client_core::agent_conversation_tools::SCHEMA => tools
                .e
                .approved_conversation_search(request)
                .map(Self::Conversation)
                .map_err(|error| Refusal::core(Phase::ConversationPermit, error)),
            1 => {
                let binding = tools
                    .binary_binding(request)
                    .map_err(|_| Refusal::executor())?;
                tools
                    .e
                    .approved_tool_turn(request, binding)
                    .map(Self::Deck)
                    .map_err(|error| Refusal::core(Phase::DeckPermit, error))
            }
            2 => tools
                .e
                .approved_page_tool_turn(request)
                .map(Self::Page)
                .map_err(|error| Refusal::core(Phase::PagePermit, error)),
            3 => tools
                .e
                .approved_app_creation(request)
                .map(Self::Create)
                .map_err(|error| Refusal::core(Phase::DeckPermit, error)),
            5 => tools
                .e
                .approved_site_tool(request)
                .map(Self::Site)
                .map_err(|error| Refusal::core(Phase::PagePermit, error)),
            6 => tools
                .e
                .approved_scene_tool_turn(request)
                .map(Self::Scene)
                .map_err(|error| Refusal::core(Phase::ScenePermit, error)),
            _ => return Err(Refusal::family()),
        };
        tools
            .check_selection()
            .map_err(|_| Refusal::selection(Phase::SelectionAfter))?;
        result
    }
    pub(super) fn proposal(&self) -> (&str, &str) {
        let p = match self {
            Self::Conversation(p) => p.proposal(),
            Self::Deck(p) => p.proposal(),
            Self::Page(p) => p.proposal(),
            Self::Create(p) => p.proposal(),
            Self::Site(p) => p.proposal(),
            Self::Scene(p) => p.proposal(),
        };
        (&p.canonical_tool, &p.continuation_json)
    }
    pub(super) fn execute(&self, tools: &SpaceTools<'_>) -> Result<String, String> {
        match self {
            Self::Conversation(permit) => tools
                .e
                .execute_conversation_search(permit)
                .map_err(|error| error.to_string()),
            Self::Scene(permit) => scene_tools::execute(tools.e, permit),
            Self::Site(permit) => tools
                .e
                .execute_site_tool(permit)
                .map_err(|error| error.to_string()),
            Self::Create(permit) => tools
                .e
                .execute_app_creation(permit)
                .map_err(|error| error.to_string()),
            Self::Deck(permit) => {
                let worker = tools
                    .deckcraft
                    .ok_or("The presentation executor is unavailable")?;
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(crate::deckcraft::call_approved(tools.e, worker, permit))
                })
            }
            Self::Page(permit) => {
                let p = permit.proposal();
                let args =
                    serde_json::from_str(&p.arguments_json).map_err(|_| "Invalid Page command")?;
                match page::parse(&p.canonical_tool, &args)? {
                    page::Command::Read { .. } => serde_json::to_string(
                        &tools
                            .e
                            .agent_page_finish_read(permit.turn())
                            .map_err(|e| e.to_string())?,
                    ),
                    page::Command::Replace(command) => serde_json::to_string(
                        &tools
                            .e
                            .agent_page_replace(permit.turn(), &command)
                            .map_err(|e| e.to_string())?,
                    ),
                }
                .map_err(|_| "Invalid Page result".into())
            }
        }
    }
}

pub(super) fn definitions() -> Vec<ToolDef> {
    page::tools()
        .into_iter()
        .map(|v| serde_json::from_value(v).expect("native Page tool schema"))
        .collect()
}
pub(super) fn is_model_name(name: &str) -> bool {
    ["read_page_", "replace_page_block_"].iter().any(|prefix| {
        name.strip_prefix(prefix)
            .is_some_and(|id| id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit()))
    })
}
impl SpaceTools<'_> {
    /// This only selects a configured fixed component; approved_tool_turn and
    /// recover_tool_results independently revalidate the signed canonical request.
    fn binary_binding(&self, request: &str) -> Result<&str, String> {
        let requests = self.e.tool_approval_requests(&self.space, self.source);
        let proposal = requests
            .iter()
            .find(|r| r.id == request)
            .and_then(|r| r.tool_proposal.as_ref())
            .ok_or("Canonical request unavailable")?;
        let format = client_core::binary_agent_tools::format(&proposal.canonical_tool)
            .ok_or("Unsupported binary App tool")?;
        self.deckcraft
            .ok_or("Binary App executor unavailable")?
            .binding_for(format.id())
    }

    pub(super) fn page_binding(&self, call: &ToolCall) -> Option<(&str, &str)> {
        self.names
            .iter()
            .find(|(def, item, tool)| {
                def.name == call.name && self.page_items.contains(item) && page::recognizes(tool)
            })
            .map(|(_, item, tool)| (item.as_str(), tool.as_str()))
    }
    pub(super) fn stage_page(
        &self,
        item: &str,
        tool: &str,
        ask: &ToolAsk,
    ) -> Result<client_core::AgentRequestDto, String> {
        let turn = self
            .e
            .agent_page_turn(item, self.source, self.serving)
            .map_err(|e| e.to_string())?;
        self.e
            .stage_page_tool_approval(
                &turn,
                client_core::AgentToolApprovalInput {
                    canonical_tool: tool.into(),
                    arguments: ask.call.arguments.clone(),
                    worker_binding: page::executor_binding(),
                    continuation_json: serde_json::to_string(ask)
                        .map_err(|_| "Invalid held Page call")?,
                    gate_rule: ask.rule.clone(),
                },
            )
            .map_err(|e| e.to_string())
    }
    pub(super) fn page_call(
        &self,
        item: &str,
        tool: &str,
        args: &serde_json::Value,
    ) -> Result<String, String> {
        let command = page::parse(tool, args)?;
        if command.artifact() != item {
            return Err("The offered tool belongs to another Page".into());
        }
        let turn = self
            .e
            .agent_page_turn(item, self.source, self.serving)
            .map_err(|e| e.to_string())?;
        match command {
            page::Command::Read { .. } => serde_json::to_string(
                &self
                    .e
                    .agent_page_finish_read(&turn)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|_| "Invalid Page result".into()),
            // Runtime forces review after Gate Allow; this boundary also refuses
            // direct ToolHost/model calls that have no signed native admission.
            page::Command::Replace(_) => {
                Err("This Page write requires an exact owner approval".into())
            }
        }
    }
    pub(super) fn recover_canonical(
        &self,
        request: &str,
        schema: u8,
        steps: &str,
    ) -> Result<String, String> {
        self.check_selection()?;
        let result = match schema {
            client_core::agent_conversation_tools::SCHEMA => {
                self.e.recover_conversation_search_results(request, steps)
            }
            1 => self
                .e
                .recover_tool_results(request, self.binary_binding(request)?, steps),
            2 => self.e.recover_page_tool_results(request, steps),
            3 => self.e.recover_app_creation_results(request, steps),
            5 => self.e.recover_site_tool_results(request, steps),
            6 => self.e.recover_scene_tool_results(request, steps),
            _ => return Err("Unsupported canonical tool family".into()),
        }
        .map_err(|e| e.to_string());
        self.check_selection()?;
        result
    }
}

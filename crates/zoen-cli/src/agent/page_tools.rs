//! Native Pages share the normal host, Gate, signed review and continuation.
use super::*;
use client_core::agent_page_tools as page;

pub(super) enum CanonicalPermit {
    Deck(client_core::ApprovedToolTurn),
    Page(client_core::ApprovedPageTurn),
}
impl CanonicalPermit {
    pub(super) fn new(tools: &SpaceTools<'_>, request: &str, schema: u8) -> Result<Self, String> {
        match schema {
            1 => tools
                .e
                .approved_tool_turn(
                    request,
                    tools
                        .deckcraft
                        .ok_or("The presentation executor is unavailable")?
                        .binding_sha256(),
                )
                .map(Self::Deck),
            2 => tools.e.approved_page_tool_turn(request).map(Self::Page),
            _ => return Err("Unsupported canonical tool family".into()),
        }
        .map_err(|e| e.to_string())
    }
    pub(super) fn proposal(&self) -> (&str, &str) {
        let p = match self {
            Self::Deck(p) => p.proposal(),
            Self::Page(p) => p.proposal(),
        };
        (&p.canonical_tool, &p.continuation_json)
    }
    pub(super) fn execute(&self, tools: &SpaceTools<'_>) -> Result<String, String> {
        match self {
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
        match schema {
            1 => self.e.recover_tool_results(
                request,
                self.deckcraft
                    .ok_or("The presentation executor is unavailable")?
                    .binding_sha256(),
                steps,
            ),
            2 => self.e.recover_page_tool_results(request, steps),
            _ => return Err("Unsupported canonical tool family".into()),
        }
        .map_err(|e| e.to_string())
    }
}

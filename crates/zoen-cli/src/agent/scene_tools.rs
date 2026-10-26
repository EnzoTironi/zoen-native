//! Owned scenes use the normal Gate, signed review, receipt and continuation.
use super::*;
use client_core::agent_scenes::{tools as scene, ApprovedSceneTurn};

pub(super) fn is_owned_scene(item: &client_core::ItemDetail) -> bool {
    item.app.as_ref().is_some_and(|app| {
        app.app_id == "excalidraw" && app.resource_uri.starts_with("ui://zoen/package/")
    })
}

pub(super) fn discover(
    e: &Arc<Client>,
    space: &str,
    source: &str,
    serving: &str,
    unrestricted: bool,
    artifact: Option<&str>,
    names: &mut Vec<(ToolDef, String, String)>,
) -> Vec<String> {
    let mut items = Vec::new();
    for item in e.items().into_iter().filter(|item| {
        item.space_id == space
            && (unrestricted || artifact == Some(item.id.as_str()))
            && is_owned_scene(item)
    }) {
        // The URI is only a discovery hint. Native authority mints the turn.
        if e.agent_scene_turn(&item.id, source, serving).is_err() {
            continue;
        }
        let mut offered = Vec::new();
        for value in scene::tools() {
            let Ok(mut definition) = serde_json::from_value::<ToolDef>(value) else {
                offered.clear();
                break;
            };
            let tool = definition.name.clone();
            let Some(name) = scene::model_name(&tool, &item.id) else {
                offered.clear();
                break;
            };
            if names
                .iter()
                .any(|(definition, _, _)| definition.name == name)
            {
                offered.clear();
                break;
            }
            definition.name = name;
            definition.parameters["properties"]["artifactId"]["enum"] = json!([item.id]);
            definition.description = format!("{} (App {:?})", definition.description, item.title);
            offered.push((definition, item.id.clone(), tool));
        }
        if offered.is_empty() || names.len() + offered.len() > 64 {
            continue;
        }
        items.push(item.id);
        names.extend(offered);
    }
    items
}

pub(super) fn is_model_name(name: &str) -> bool {
    ["read_scene_", "replace_scene_"].iter().any(|prefix| {
        name.strip_prefix(prefix)
            .is_some_and(|id| id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit()))
    })
}

pub(super) fn execute(e: &Client, permit: &ApprovedSceneTurn) -> Result<String, String> {
    let proposal = permit.proposal();
    let arguments = serde_json::from_str(&proposal.arguments_json)
        .map_err(|_| "Invalid reviewed scene command")?;
    match scene::parse(&proposal.canonical_tool, &arguments).map_err(|e| e.to_string())? {
        scene::Command::Read { .. } => serde_json::to_string(
            &e.agent_scene_finish_read(permit.turn())
                .map_err(|e| e.to_string())?,
        ),
        scene::Command::Replace(command) => serde_json::to_string(
            &e.agent_scene_replace(permit.turn(), &command)
                .map_err(|e| e.to_string())?,
        ),
    }
    .map_err(|_| "Invalid scene result".into())
}

impl SpaceTools<'_> {
    pub(super) fn scene_binding(&self, call: &ToolCall) -> Option<(&str, &str)> {
        self.names
            .iter()
            .find(|(definition, item, tool)| {
                definition.name == call.name
                    && self.scene_items.contains(item)
                    && scene::recognizes(tool)
                    && scene::model_name(tool, item).as_deref() == Some(call.name.as_str())
            })
            .map(|(_, item, tool)| (item.as_str(), tool.as_str()))
    }

    pub(super) fn stage_scene(
        &self,
        item: &str,
        tool: &str,
        ask: &ToolAsk,
    ) -> Result<client_core::AgentRequestDto, String> {
        let turn = self
            .e
            .agent_scene_turn(item, self.source, self.serving)
            .map_err(|e| e.to_string())?;
        self.e
            .stage_scene_tool_approval(
                &turn,
                client_core::AgentToolApprovalInput {
                    canonical_tool: tool.into(),
                    arguments: ask.call.arguments.clone(),
                    worker_binding: scene::executor_binding(),
                    continuation_json: serde_json::to_string(ask)
                        .map_err(|_| "Invalid held scene call")?,
                    gate_rule: ask.rule.clone(),
                },
            )
            .map_err(|e| e.to_string())
    }

    pub(super) fn scene_call(
        &self,
        item: &str,
        tool: &str,
        args: &serde_json::Value,
    ) -> Result<String, String> {
        let command = scene::parse(tool, args).map_err(|e| e.to_string())?;
        if command.artifact() != item {
            return Err("The offered tool belongs to another App".into());
        }
        let turn = self
            .e
            .agent_scene_turn(item, self.source, self.serving)
            .map_err(|e| e.to_string())?;
        match command {
            scene::Command::Read { .. } => serde_json::to_string(
                &self
                    .e
                    .agent_scene_finish_read(&turn)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|_| "Invalid scene result".into()),
            scene::Command::Replace(_) => {
                Err("This App write requires an exact owner approval".into())
            }
        }
    }

    pub(super) fn scene_public_references(
        &self,
        item: &str,
        tool: &str,
        args: &serde_json::Value,
    ) -> Option<zoen_agentd::PublicArtifactReferences> {
        let command = scene::parse(tool, args).ok()?;
        if command.artifact() != item {
            return None;
        }
        let turn = self
            .e
            .agent_scene_turn(item, self.source, self.serving)
            .ok()?;
        let snapshot = self.e.agent_scene_finish_read(&turn).ok()?;
        if let scene::Command::Replace(command) = command {
            if command.version != snapshot.version
                || command.sha256 != snapshot.sha256
                || command.resource_uri != snapshot.resource_uri
            {
                return None;
            }
            return zoen_agentd::PublicArtifactReferences::scene_head(
                item.into(),
                snapshot.sha256,
                snapshot.resource_uri,
            );
        }
        // The replacement scene remains ordinary semantic Gate input. Only this
        // freshly verified locator/hash is classified as an artifact reference.
        zoen_agentd::PublicArtifactReferences::new(tool.into(), item.into(), None)
    }
}

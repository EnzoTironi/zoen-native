//! Site tools join the existing host/Gate/continuation; no browser or hosting tool.
use super::*;
use client_core::agent_sites as sites;

pub(super) fn discover(
    e: &Arc<Client>,
    space: &str,
    source: &str,
    serving: &str,
    names: &mut Vec<(ToolDef, String, String)>,
) {
    if names.len() < 64 && e.can_create_agent_site(space, source, serving) {
        if let Ok(value) = sites::definition("create_site", space) {
            let definition: ToolDef =
                serde_json::from_value(value).expect("native Site definition");
            if !names.iter().any(|(d, _, _)| d.name == definition.name) {
                names.push((definition, space.into(), "create_site".into()));
            }
        }
    }
    for (item, version, hash) in e.agent_site_sources(space, source, serving) {
        let mut offered = Vec::new();
        for tool in ["read_site", "replace_site_source"] {
            let Ok(value) = sites::selected_definition(tool, &item, version, &hash) else {
                offered.clear();
                break;
            };
            let definition: ToolDef =
                serde_json::from_value(value).expect("native Site definition");
            if names.iter().any(|(d, _, _)| d.name == definition.name) {
                offered.clear();
                break;
            }
            offered.push((definition, item.clone(), tool.into()));
        }
        if names.len() + offered.len() <= 64 {
            names.extend(offered);
        }
    }
}
impl SpaceTools<'_> {
    pub(super) fn site_binding(&self, call: &ToolCall) -> Option<(&str, &str)> {
        self.names
            .iter()
            .find(|(d, item, tool)| {
                d.name == call.name && sites::model_name(tool, item).as_deref() == Some(&call.name)
            })
            .map(|(_, item, tool)| (item.as_str(), tool.as_str()))
    }
    pub(super) fn site_call(&self, call: &ToolCall) -> Result<String, String> {
        self.check_selection()?;
        let (item, tool) = self
            .site_binding(call)
            .ok_or("This Site tool is no longer offered")?;
        match sites::parse(tool, &call.arguments).map_err(|error| error.to_string())? {
            sites::Command::Read(selected) if selected.artifact_id == item => {
                let snapshot = self
                    .e
                    .agent_site_read(
                        item,
                        selected.version,
                        &selected.sha256,
                        self.source,
                        self.serving,
                    )
                    .map_err(|error| error.to_string())?;
                self.check_selection()?;
                serde_json::to_string(&snapshot).map_err(|_| "Invalid Site snapshot".into())
            }
            _ => Err("This Site write requires its exact owner approval".into()),
        }
    }
}

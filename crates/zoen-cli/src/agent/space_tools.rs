//! Existing per-conversation tool discovery, source binding and journal state.

use super::*;

impl<'a> SpaceTools<'a> {
    pub(super) fn new(
        e: &'a Arc<Client>,
        cx: &'a Ctx,
        space: &str,
        serving: &'a str,
        source: &'a str,
    ) -> Self {
        Self::discover(
            e,
            &cx.journal,
            cx.deckcraft.as_ref(),
            space,
            serving,
            source,
        )
    }

    pub(super) fn fresh(&self) -> Self {
        let mut current = Self::discover(
            self.e,
            self.journal,
            self.deckcraft,
            &self.space,
            self.serving,
            self.source,
        );
        current.selection = self.selection;
        current
    }

    fn discover(
        e: &'a Arc<Client>,
        journal: &'a Path,
        deckcraft: Option<&'a Arc<zoen_agentd::deckcraft::DeckcraftWorker>>,
        space: &str,
        serving: &'a str,
        source: &'a str,
    ) -> Self {
        let mut names: Vec<(ToolDef, String, String)> = Vec::new();
        if deckcraft.is_some() && e.can_create_agent_app(space, source, serving) {
            if let Some(definition) = client_core::agent_app_creation::definition(space) {
                let definition: ToolDef =
                    serde_json::from_value(definition).expect("native creation schema");
                names.push((
                    definition,
                    space.into(),
                    client_core::agent_app_creation::TOOL.into(),
                ));
            }
        }
        if let Err(reason) = site_tools::discover(e, space, source, serving, &mut names) {
            say(reason);
        }
        let binary_items: std::collections::BTreeSet<_> = e
            .items()
            .into_iter()
            .filter(|item| {
                item.space_id == space
                    && item
                        .app
                        .as_ref()
                        .is_some_and(|app| app.app_id == "deckcraft")
            })
            .map(|item| item.id)
            .collect();
        for t in e.agent_tools(space) {
            // Artifact transport is for its managed App UI, never a model tool.
            if binary_items.contains(&t.item_id) {
                continue;
            }
            if names.len() >= 64 {
                break;
            }
            // Two mini-apps with the same tool: the second is `name_2`, and so on.
            let mut name = t.name.clone();
            let mut n = 1;
            while names.iter().any(|(d, _, _)| d.name == name) {
                n += 1;
                name = format!("{}_{n}", t.name);
            }
            let def = ToolDef {
                name,
                description: format!("{} (mini-app \"{}\")", t.description, t.item_title),
                parameters: t.schema,
            };
            names.push((def, t.item_id, t.name));
        }
        let mut deckcraft_items = Vec::new();
        if deckcraft.is_some() {
            for item in e.items().into_iter().filter(|item| {
                item.space_id == space
                    && item
                        .app
                        .as_ref()
                        .is_some_and(|app| app.app_id == "deckcraft")
            }) {
                let mut offered = Vec::new();
                for mut definition in crate::deckcraft::tools() {
                    let tool = definition.name.clone();
                    let Some(name) = crate::deckcraft::model_name(&tool, &item.id) else {
                        offered.clear();
                        break;
                    };
                    if names.iter().any(|(existing, _, _)| existing.name == name) {
                        offered.clear();
                        break;
                    }
                    definition.name = name;
                    definition.parameters["properties"]["artifactId"]["enum"] = json!([item.id]);
                    definition.description =
                        format!("{} (presentation {:?})", definition.description, item.title);
                    offered.push((definition, item.id.clone(), tool));
                }
                // Keep the gateway's existing 64-tool limit. Offer an entire
                // artifact's set or none; never advertise an unusable partial set.
                if offered.is_empty() || names.len() + offered.len() > 64 {
                    continue;
                }
                deckcraft_items.push(item.id.clone());
                names.extend(offered);
            }
        }
        let mut page_items = Vec::new();
        for item in e
            .items()
            .into_iter()
            .filter(|item| item.space_id == space && item.kind_id == "page")
        {
            if e.agent_page_turn(&item.id, source, serving).is_err() {
                continue;
            }
            let mut offered = Vec::new();
            for mut definition in page_tools::definitions() {
                let tool = definition.name.clone();
                let Some(name) = client_core::agent_page_tools::model_name(&tool, &item.id) else {
                    offered.clear();
                    break;
                };
                if names.iter().any(|(existing, _, _)| existing.name == name) {
                    offered.clear();
                    break;
                }
                definition.name = name;
                definition.parameters["properties"]["artifactId"]["enum"] = json!([item.id]);
                definition.description =
                    format!("{} (Page {:?})", definition.description, item.title);
                offered.push((definition, item.id.clone(), tool));
            }
            if offered.is_empty() || names.len() + offered.len() > 64 {
                continue;
            }
            page_items.push(item.id);
            names.extend(offered);
        }
        Self {
            e,
            journal,
            source,
            space: space.into(),
            serving,
            names,
            deckcraft,
            deckcraft_items,
            selection: None,
            page_items,
        }
    }

    pub(super) fn check_selection(&self) -> Result<(), String> {
        if let Some(selection) = self.selection {
            selection.check(self.e)?;
        }
        Ok(())
    }

    pub(super) fn stage_approval(
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
        if client_core::agent_app_creation::model_name(&self.space).as_deref()
            == Some(&ask.call.name)
            && self.names.iter().any(|(definition, _, tool)| {
                definition.name == ask.call.name && tool == client_core::agent_app_creation::TOOL
            })
        {
            let dto = self
                .e
                .stage_app_creation_approval(
                    &self.space,
                    self.source,
                    self.serving,
                    client_core::AgentToolApprovalInput {
                        canonical_tool: client_core::agent_app_creation::TOOL.into(),
                        arguments: ask.call.arguments.clone(),
                        worker_binding: client_core::agent_app_creation::executor_binding(),
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
        if !self.deckcraft_items.contains(item) || !crate::deckcraft::recognizes(tool) {
            return Err("This tool has no supported canonical approval shape".into());
        }
        let worker = self
            .deckcraft
            .ok_or("The presentation executor is unavailable")?;
        let (version, hash, _) =
            client_core::deckcraft_agent_tools::command(tool, &ask.call.arguments)?;
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
                    worker_binding: worker.binding_sha256().into(),
                    continuation_json: serde_json::to_string(ask)
                        .map_err(|_| "Invalid held call")?,
                    gate_rule: ask.rule.clone(),
                },
            )
            .map_err(|e| e.to_string());
        self.check_selection()?;
        staged.and_then(|dto| self.register_staged(dto, &prior))
    }

    pub(super) fn load(&self) -> Vec<(String, Vec<ToolStep>)> {
        JournalState::load(self.journal)
            .map(|state| state.steps)
            .unwrap_or_default()
    }

    /// What an earlier run of this turn already did.
    pub(super) fn done(&self) -> Vec<ToolStep> {
        self.load()
            .into_iter()
            .find(|(s, _)| s == self.source)
            .map(|(_, steps)| steps)
            .unwrap_or_default()
    }

    pub(super) fn begin_resume(&self, request: &str, content_hash: &str) -> Result<bool, String> {
        self.check_selection()?;
        let (request, content_hash) = self.journal_binding(request, content_hash)?;
        JournalState::begin_resume(self.journal, &request, &content_hash, Some(self.source))
    }

    pub(super) fn canonical_continuation(
        &self,
    ) -> Result<Option<(String, String, ToolAsk)>, String> {
        self.check_selection()?;
        let requests = self.e.tool_approval_requests(&self.space, self.source);
        let Some(request) = requests.last() else {
            return Ok(None);
        };
        let proposal = request
            .tool_proposal
            .as_ref()
            .ok_or("The canonical tool proposal is missing")?;
        let original: ToolAsk = serde_json::from_str(&proposal.continuation_json)
            .map_err(|_| "The canonical tool continuation is invalid")?;
        if proposal.source != self.source
            || original.envelope.space != self.space
            || original.envelope.from != self.serving
        {
            return Err("The canonical tool source changed".into());
        }
        let (request, hash) = self.journal_binding(&request.id, &request.content_hash)?;
        Ok(Some((request, hash, original)))
    }

    pub(super) fn recovery_checkpoint(
        &self,
        request: &str,
        hash: &str,
    ) -> Result<Option<(ToolProgress, Option<ToolAsk>)>, String> {
        self.check_selection()?;
        let (request, hash) = self.journal_binding(request, hash)?;
        let state = JournalState::load(self.journal)?;
        let Some(budget) = state.continuations.get(&request) else {
            return Ok(None);
        };
        if budget.content_hash != hash || budget.source.as_deref() != Some(self.source) {
            return Err("The continuation checkpoint binding changed".into());
        }
        match (budget.progress.clone(), budget.prepared.clone()) {
            (Some(progress), prepared) => Ok(Some((progress, prepared))),
            (None, None) => Ok(None),
            (None, Some(_)) => Err("The prepared call has no completed checkpoint".into()),
        }
    }

    pub(super) fn same_calls(prefix: &[ToolStep], steps: &[ToolStep]) -> bool {
        prefix.len() <= steps.len()
            && prefix
                .iter()
                .zip(steps)
                .all(|(old, new)| old.call == new.call)
    }

    pub(super) fn prepare_continuation(&self, prepared: &ToolAsk) -> Result<(), String> {
        if prepared.version != 1
            || prepared.round >= 4
            || prepared.steps.len() >= 16
            || prepared.remaining.len() > 3
            || prepared.steps.len() + 1 + prepared.remaining.len() > 16
            || serde_json::to_vec(prepared).map_or(true, |bytes| {
                bytes.len() > zoen_agentd::MAX_TOOL_CONTINUATION_BYTES
            })
        {
            return Err("The prepared continuation exceeds its capacity".into());
        }
        let Some((request, hash, original)) = self.canonical_continuation()? else {
            return Ok(());
        };
        if prepared.envelope != original.envelope
            || prepared.policy_sha256 != original.policy_sha256
            || prepared.round < original.round
            || prepared.steps.len() <= original.steps.len()
            || !Self::same_calls(&original.steps, &prepared.steps)
            || prepared.steps[original.steps.len()].call != original.call
            || self
                .fresh()
                .names
                .iter()
                .find(|(definition, _, _)| definition.name == prepared.call.name)
                .map(|(definition, _, _)| definition)
                != Some(&prepared.definition)
        {
            return Err("The prepared continuation changed its canonical context".into());
        }
        JournalState::update(self.journal, |state| {
            let budget = state
                .continuations
                .get_mut(&request)
                .ok_or("The continuation attempt was not reserved")?;
            if budget.content_hash != hash
                || budget.source.as_deref() != Some(self.source)
                || budget
                    .prepared
                    .as_ref()
                    .is_some_and(|prior| prior != prepared)
                || budget.progress.as_ref().is_none_or(|prior| {
                    prior.round > prepared.round || !Self::same_calls(&prior.steps, &prepared.steps)
                })
            {
                return Err("The prepared continuation conflicts with retained progress".into());
            }
            budget.prepared = Some(prepared.clone());
            Ok(true)
        })
        .map(|_| ())
    }

    pub(super) fn checkpoint_continuation(&self, progress: &ToolProgress) -> Result<(), String> {
        if serde_json::to_vec(progress).map_or(true, |bytes| {
            bytes.len() > zoen_agentd::MAX_TOOL_CONTINUATION_BYTES
        }) {
            return Err("The completed continuation exceeds its capacity".into());
        }
        let Some((request, hash, original)) = self.canonical_continuation()? else {
            return Ok(());
        };
        if progress.round < original.round
            || progress.round >= 4
            || progress.steps.len() <= original.steps.len()
            || progress.steps.len() > 16
            || progress.remaining.len() > 3
            || progress.steps.len() + progress.remaining.len() > 16
            || !Self::same_calls(&original.steps, &progress.steps)
            || progress.steps[original.steps.len()].call != original.call
        {
            return Err("The completed continuation changed its canonical context".into());
        }
        JournalState::update(self.journal, |state| {
            let budget = state
                .continuations
                .get_mut(&request)
                .ok_or("The continuation attempt was not reserved")?;
            if budget.content_hash != hash
                || budget.source.as_deref() != Some(self.source)
                || budget.progress.as_ref().is_some_and(|prior| {
                    prior.round > progress.round || !Self::same_calls(&prior.steps, &progress.steps)
                })
            {
                return Err("The completed continuation conflicts with retained progress".into());
            }
            if let Some(prepared) = &budget.prepared {
                let matches = prepared.round == progress.round
                    && prepared.remaining == progress.remaining
                    && progress.steps.len() == prepared.steps.len() + 1
                    && Self::same_calls(&prepared.steps, &progress.steps)
                    && progress
                        .steps
                        .last()
                        .is_some_and(|step| step.call == prepared.call);
                if !matches {
                    return Err("The completed call does not match the prepared effect".into());
                }
                budget.prepared = None;
            }
            budget.progress = Some(progress.clone());
            match state
                .steps
                .iter_mut()
                .find(|(source, _)| source == self.source)
            {
                Some((_, steps)) => *steps = progress.steps.clone(),
                None => state
                    .steps
                    .push((self.source.into(), progress.steps.clone())),
            }
            let drop = state.steps.len().saturating_sub(JOURNAL_KEEP);
            state.steps.drain(..drop);
            Ok(true)
        })
        .map(|_| ())
    }

    pub(super) fn record(&self, step: ToolStep) {
        let result = JournalState::update(self.journal, |state| {
            match state
                .steps
                .iter_mut()
                .find(|(source, _)| source == self.source)
            {
                Some((_, steps)) => steps.push(step),
                None => state.steps.push((self.source.to_string(), vec![step])),
            }
            let drop = state.steps.len().saturating_sub(JOURNAL_KEEP);
            state.steps.drain(..drop);
            Ok(true)
        });
        if result.is_err() {
            say("tool journal write failed");
        }
    }
}

pub(super) mod renewal;

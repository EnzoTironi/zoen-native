//! Canonical approved execution and the existing ToolHost implementation.

use super::*;

impl ApprovedToolAdmission for CanonicalToolAdmission<'_, '_> {
    fn execute(&self, ask: &ToolAsk) -> Result<ToolResult, String> {
        self.tools.check_selection()?;
        let (tool, continuation) = self.permit.proposal();
        if serde_json::to_string(ask).map_err(|_| "Invalid held call")? != continuation {
            return Err("The approved intent changed".into());
        }
        self.tools.entering_canonical_executor()?;
        // Native authorization is rechecked even for a cached read result. Writes
        // always recover through their canonical binary receipt instead.
        if let page_tools::CanonicalPermit::Deck(permit) = &self.permit {
            self.tools
                .e
                .agent_binary_finish_read(permit.turn())
                .map_err(|e| e.to_string())?;
        } else if let page_tools::CanonicalPermit::Page(permit) = &self.permit {
            if tool == "read_page" {
                self.tools
                    .e
                    .agent_page_finish_read(permit.turn())
                    .map_err(|e| e.to_string())?;
            }
        }
        let result = if matches!(
            tool,
            "read_deckcraft" | "read_gridcraft" | "read_wordcraft" | "read_page"
        ) {
            self.tools
                .done()
                .into_iter()
                .find(|step| step.call == ask.call)
                .map(|step| step.result)
        } else {
            None
        };
        let result = match result {
            Some(result) => result,
            None => self.permit.execute(self.tools)?,
        };
        self.tools.check_selection()?;
        if !self.tools.done().iter().any(|step| step.call == ask.call) {
            fault("before-journal");
            self.tools.record(ToolStep {
                call: ask.call.clone(),
                result: result.clone(),
                is_error: false,
            });
            fault("after-tool");
        }
        Ok(ToolResult {
            result,
            is_error: false,
        })
    }
}

impl ToolHost for SpaceTools<'_> {
    fn observe_model_input(&self, tools: &[ToolDef], steps: &[ToolStep], round: usize) {
        trace::tool_observation::model_input(self, tools, steps, round);
    }
    fn observe_tool_outcome(&self, call: &ToolCall, outcome: zoen_agentd::ToolObservation) {
        trace::tool_observation::tool_outcome(self, call, outcome);
    }
    fn requires_native_review(&self, call: &ToolCall) -> bool {
        let current = self.fresh();
        current
            .site_binding(call)
            .is_some_and(|(_, tool)| tool != "read_site")
            || current
                .page_binding(call)
                .is_some_and(|(_, tool)| tool == "replace_page_block")
            || current.names.iter().any(|(definition, _, tool)| {
                definition.name == call.name && client_core::agent_app_creation::is_model_name(tool)
            })
    }
    fn tools(&self) -> Vec<ToolDef> {
        self.fresh()
            .names
            .into_iter()
            .map(|(definition, _, _)| definition)
            .collect()
    }
    fn refreshed_tools(&self) -> Option<Vec<ToolDef>> {
        Some(self.tools())
    }

    fn prepare(&self, prepared: &ToolAsk) -> Result<(), String> {
        self.check_selection()?;
        self.prepare_continuation(prepared)
    }
    fn checkpoint(&self, progress: &ToolProgress) -> Result<(), String> {
        self.check_selection()?;
        self.checkpoint_continuation(progress)
    }

    fn gate_public_references(
        &self,
        call: &ToolCall,
    ) -> Option<zoen_agentd::PublicArtifactReferences> {
        self.fresh().current_gate_public_references(call)
    }
    fn call(&self, call: &ToolCall) -> Result<String, String> {
        self.fresh().current_call(call)
    }
}

impl SpaceTools<'_> {
    fn current_gate_public_references(
        &self,
        call: &ToolCall,
    ) -> Option<zoen_agentd::PublicArtifactReferences> {
        self.check_selection().ok()?;
        if let Some((item, tool)) = self.site_binding(call) {
            let command = client_core::agent_sites::parse(tool, &call.arguments).ok()?;
            let (artifact, version, hash) = match command {
                client_core::agent_sites::Command::Read(c) => (c.artifact_id, c.version, c.sha256),
                client_core::agent_sites::Command::Replace(c) => {
                    (c.artifact_id, c.version, c.sha256)
                }
                client_core::agent_sites::Command::Create(_) => return None,
            };
            if artifact != item {
                return None;
            }
            self.e
                .agent_site_read(item, version, &hash, self.source, self.serving)
                .ok()?;
            self.check_selection().ok()?;
            // Only the native-verified locator/hash are public. Replacement HTML
            // remains semantic Gate input and retains credential/content checks.
            return zoen_agentd::PublicArtifactReferences::new(
                tool.into(),
                item.into(),
                Some(hash),
            );
        }
        if let Some((item, tool)) = self.page_binding(call) {
            let references = (|| {
                let parsed = client_core::agent_page_tools::parse(tool, &call.arguments).ok()?;
                if parsed.artifact() != item {
                    return None;
                }
                let turn = self
                    .e
                    .agent_page_turn(item, self.source, self.serving)
                    .ok()?;
                let snapshot = self.e.agent_page_finish_read(&turn).ok()?;
                if let client_core::agent_page_tools::Command::Replace(command) = parsed {
                    if command.version != snapshot.version
                        || command.prefix_sha256 != snapshot.prefix_sha256
                        || command.content_sha256 != snapshot.content_sha256
                        || snapshot.blocks.iter().find(|b| b.id == command.before.id)
                            != Some(&command.before)
                    {
                        return None;
                    }
                    return zoen_agentd::PublicArtifactReferences::page_head(
                        item.into(),
                        command.prefix_sha256,
                        command.content_sha256,
                    );
                }
                // Only the native-validated public locator is classified. Reviewed
                // text/formatting and every other argument still reach ordinary Gate.
                zoen_agentd::PublicArtifactReferences::new(tool.into(), item.into(), None)
            })();
            self.check_selection().ok()?;
            return references;
        }
        let (_, item, tool) = self
            .names
            .iter()
            .find(|(definition, _, _)| definition.name == call.name)?;
        if !self.deckcraft_items.contains(item) || !crate::deckcraft::recognizes(tool) {
            return None;
        }
        let references = crate::deckcraft::gate_public_references(
            self.e,
            item,
            self.source,
            self.serving,
            tool,
            &call.arguments,
        );
        self.check_selection().ok()?;
        references
    }
    fn current_call(&self, call: &ToolCall) -> Result<String, String> {
        self.check_selection()?;
        if call.name == image_tools::TOOL {
            return self.image_stage(call);
        }
        if client_core::agent_sites::is_model_name(&call.name) {
            return self.site_call(call);
        }
        if client_core::agent_app_creation::is_model_name(&call.name) {
            return Err("Presentation creation requires its exact native approval".into());
        }
        if call.name.starts_with("app_binary_") {
            return Err("Binary App transport is not a model tool".into());
        }
        if let Some((item, tool)) = self.page_binding(call) {
            let result = self.page_call(item, tool, &call.arguments);
            self.check_selection()?;
            if let Ok(text) = &result {
                if !self.done().iter().any(|step| step.call == *call) {
                    self.record(ToolStep {
                        call: call.clone(),
                        result: text.clone(),
                        is_error: false,
                    });
                }
            }
            return result;
        }
        // Binary intents reconcile through the signed native store, never a journal
        // cache that could bypass current access or an ambiguous accepted result.
        if let Some((_, item, tool)) = self
            .names
            .iter()
            .find(|(definition, _, _)| definition.name == call.name)
        {
            if self.deckcraft_items.contains(item) && crate::deckcraft::recognizes(tool) {
                let worker = self
                    .deckcraft
                    .ok_or("The isolated presentation worker is unavailable")?;
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(crate::deckcraft::call(
                        self.e,
                        worker,
                        item,
                        self.source,
                        self.serving,
                        tool,
                        &call.arguments,
                    ))
                });
                self.check_selection()?;
                if let Ok(text) = &result {
                    if !self.done().iter().any(|step| {
                        step.call.name == call.name && step.call.arguments == call.arguments
                    }) {
                        fault("before-journal");
                        self.record(ToolStep {
                            call: call.clone(),
                            result: text.clone(),
                            is_error: false,
                        });
                        fault("after-tool");
                    }
                }
                return result;
            }
        }
        // Missing/disabled binary bindings must not fall through to the generic
        // journal cache and report success without fresh native authority.
        if crate::deckcraft::is_model_name(&call.name) || page_tools::is_model_name(&call.name) {
            return Err("This presentation tool is no longer offered".into());
        }
        // The same call already ran for this message (before a restart): its result, again.
        if let Some(s) = self
            .done()
            .into_iter()
            .find(|s| s.call.name == call.name && s.call.arguments == call.arguments)
        {
            self.check_selection()?;
            return if s.is_error {
                Err(s.result)
            } else {
                Ok(s.result)
            };
        }
        let Some((_, item, tool)) = self.names.iter().find(|(d, _, _)| d.name == call.name) else {
            return Err(format!("unknown tool: {}", call.name));
        };
        // Stamped with the message it answers, so a call that landed just before a crash is
        // recognised on the rerun instead of being refused as a clash.
        let mut args = call.arguments.clone();
        if !args.is_object() {
            args = json!({});
        }
        args["ref"] = self.source.into();
        let args = args.to_string();
        let res = match self.e.agent_call_tool(item, tool, &args, self.serving) {
            Ok(o) => match o.status {
                AppCallStatus::Done => {
                    let v: serde_json::Value =
                        serde_json::from_str(&o.result_json).unwrap_or_default();
                    if v["isError"].as_bool() == Some(true) {
                        Err(o.result_json)
                    } else {
                        Ok(o.result_json)
                    }
                }
                AppCallStatus::NeedsConfirmation => {
                    Err(format!("needs the owner's confirmation: {}", o.message))
                }
                AppCallStatus::Denied => Err(format!("denied: {}", o.message)),
            },
            Err(err) => Err(err.to_string()),
        };
        self.check_selection()?;
        // Only calls that changed something need to survive a restart; a failed or
        // read-only call is cheap and safe to run again.
        if res.is_ok() {
            fault("before-journal");
            self.record(ToolStep {
                call: call.clone(),
                result: res.clone().unwrap_or_default(),
                is_error: false,
            });
            fault("after-tool");
        }
        turn_note(
            self.selection,
            if res.is_ok() {
                "agent_host_source_tool_done"
            } else {
                "agent_host_source_tool_refused"
            },
            format_args!(
                "tool {} -> {}",
                call.name,
                if res.is_ok() { "ok" } else { "refused" }
            ),
        );
        res
    }
}

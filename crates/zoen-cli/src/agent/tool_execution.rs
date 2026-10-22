//! Canonical approved execution and the existing ToolHost implementation.

use super::*;

impl ApprovedToolAdmission for CanonicalToolAdmission<'_, '_> {
    fn execute(&self, ask: &ToolAsk) -> Result<ToolResult, String> {
        self.tools.check_selection()?;
        let p = self.permit.proposal();
        if serde_json::to_string(ask).map_err(|_| "Invalid held call")? != p.continuation_json {
            return Err("The approved intent changed".into());
        }
        let worker = self
            .tools
            .deckcraft
            .ok_or("The presentation executor is unavailable")?;
        // Native authorization is rechecked even for a cached read result. Writes
        // always recover through their canonical binary receipt instead.
        self.tools
            .e
            .agent_binary_finish_read(self.permit.turn())
            .map_err(|e| e.to_string())?;
        let result = if p.canonical_tool == "read_deckcraft" {
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
            None => tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(crate::deckcraft::call_approved(
                    self.tools.e,
                    worker,
                    &self.permit,
                ))
            })?,
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
    fn tools(&self) -> Vec<ToolDef> {
        self.names.iter().map(|(d, _, _)| d.clone()).collect()
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
        self.check_selection().ok()?;
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
    fn call(&self, call: &ToolCall) -> Result<String, String> {
        self.check_selection()?;
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
        if crate::deckcraft::is_model_name(&call.name) {
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

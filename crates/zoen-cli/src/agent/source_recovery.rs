//! A returned tool plan is known; its executor result still requires native evidence.
use super::*;
use journal::turn_budget::{binding, current_policy_hash, key, ModelOutcome};
use std::sync::atomic::{AtomicBool, Ordering};
mod state;

pub(super) struct SourceTools<'a, 'b> {
    tools: &'a SpaceTools<'b>,
    cx: &'a Ctx,
    definition: &'a Definition,
    source: &'a TimelineEntry,
    first: AtomicBool,
}
impl SourceTools<'_, '_> {
    pub(super) fn new<'a, 'b>(
        tools: &'a SpaceTools<'b>,
        cx: &'a Ctx,
        definition: &'a Definition,
        source: &'a TimelineEntry,
    ) -> SourceTools<'a, 'b> {
        SourceTools {
            tools,
            cx,
            definition,
            source,
            first: AtomicBool::new(true),
        }
    }

    pub(super) fn recover(&self) -> Result<Option<(ToolAsk, ToolProgress)>, String> {
        let Some(budget) = self.record()? else {
            return Ok(None);
        };
        let Some(ModelOutcome::Tools {
            original,
            policy_hash,
        }) = budget.model_outcome
        else {
            return Ok(None);
        };
        if policy_hash != current_policy_hash(self.cx, self.definition)? {
            return Err("The source tool policy changed".into());
        }
        let original = original.ok_or("The known model batch has no exact prepared effect")?;
        let mut progress = if let Some(prepared) = budget.prepared {
            let mut steps = prepared.steps;
            steps.push(ToolStep {
                call: prepared.call,
                result: String::new(),
                is_error: false,
            });
            ToolProgress {
                round: prepared.round,
                steps,
                remaining: prepared.remaining,
            }
        } else {
            budget
                .progress
                .ok_or("The source tool progress is missing")?
        };
        for step in &mut progress.steps {
            if step.is_error {
                return Err("An error is not an accepted effect receipt".into());
            }
            step.result = self.recover_step(&step.call)?;
        }
        let mut original = *original;
        let prefix = progress
            .steps
            .get(..original.steps.len())
            .ok_or("The source tool prefix changed")?;
        if !SpaceTools::same_calls(&original.steps, prefix) {
            return Err("The source tool prefix changed".into());
        }
        original.steps = prefix.to_vec();
        self.first.store(false, Ordering::SeqCst);
        Ok(Some((original, progress)))
    }

    fn recover_step(&self, call: &ToolCall) -> Result<String, String> {
        self.tools.check_selection()?;
        let current = self.tools.fresh();
        if call.name == history_search::TOOL {
            // This is a read, not an App effect receipt. Repeat its authorized
            // index query without repeating the known model proposal.
            return current.search_history(call);
        }
        let (_, item, tool) = current
            .names
            .iter()
            .find(|(definition, _, _)| definition.name == call.name)
            .ok_or("The source tool is no longer offered")?;
        let mut args = call.arguments.clone();
        let object = args.as_object_mut().ok_or("Invalid tool arguments")?;
        object.insert("ref".into(), self.source.id.clone().into());
        let result = self
            .tools
            .e
            .agent_recover_tool(item, tool, &args.to_string(), self.tools.serving)
            .map_err(|error| error.to_string())?;
        self.tools.check_selection()?;
        if result.status != AppCallStatus::Done {
            return Err("The accepted effect is no longer authorized".into());
        }
        let body: serde_json::Value = serde_json::from_str(&result.result_json)
            .map_err(|_| "Invalid accepted tool result")?;
        if body["isError"].as_bool() != Some(false) {
            return Err("The tool result is not an accepted effect".into());
        }
        Ok(result.result_json)
    }
}

impl ToolHost for SourceTools<'_, '_> {
    fn tools(&self) -> Vec<ToolDef> {
        self.tools.tools()
    }
    fn refreshed_tools(&self) -> Option<Vec<ToolDef>> {
        self.tools.refreshed_tools()
    }
    fn observe_model_input(&self, tools: &[ToolDef], steps: &[ToolStep], round: usize) {
        self.tools.observe_model_input(tools, steps, round);
    }
    fn observe_tool_outcome(&self, call: &ToolCall, outcome: zoen_agentd::ToolObservation) {
        self.tools.observe_tool_outcome(call, outcome);
    }
    fn requires_native_review(&self, call: &ToolCall) -> bool {
        self.tools.requires_native_review(call)
    }
    fn gate_public_references(
        &self,
        call: &ToolCall,
    ) -> Option<zoen_agentd::PublicArtifactReferences> {
        self.tools.gate_public_references(call)
    }
    fn call(&self, call: &ToolCall) -> Result<String, String> {
        self.tools.call(call)
    }
    fn begin_model(&self) -> Result<(), String> {
        if self.first.swap(false, Ordering::SeqCst) {
            let reserved = self
                .record()?
                .is_some_and(|budget| budget.model_outcome == Some(ModelOutcome::Unknown));
            return if reserved {
                Ok(())
            } else {
                Err("The initial inference is not reserved".into())
            };
        }
        self.update(|budget| {
            if !matches!(budget.model_outcome, Some(ModelOutcome::Tools { .. }))
                || budget.prepared.is_some()
            {
                return Err("The next inference lacks known completed tools".into());
            }
            budget.model_outcome = Some(ModelOutcome::Unknown);
            Ok(())
        })
    }
    fn model_calls(
        &self,
        _calls: &[ToolCall],
        _steps: &[ToolStep],
        _round: usize,
    ) -> Result<(), String> {
        let policy_hash = current_policy_hash(self.cx, self.definition)?;
        self.update(|budget| {
            if budget.model_outcome != Some(ModelOutcome::Unknown) || budget.prepared.is_some() {
                return Err("The model batch was not preceded by an inference fence".into());
            }
            budget.model_outcome = Some(ModelOutcome::Tools {
                original: None,
                policy_hash,
            });
            Ok(())
        })
    }
    fn prepare(&self, prepared: &ToolAsk) -> Result<(), String> {
        self.prepare_source(prepared)
    }
    fn checkpoint(&self, progress: &ToolProgress) -> Result<(), String> {
        self.checkpoint_source(progress)
    }
}

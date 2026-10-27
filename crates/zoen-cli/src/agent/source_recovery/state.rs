use super::*;
impl SourceTools<'_, '_> {
    pub(super) fn record(&self) -> Result<Option<ResumeAttempts>, String> {
        self.tools.check_selection()?;
        let mut state = JournalState::load(self.tools.journal)?;
        let prior = state
            .continuations
            .remove(&key(self.cx, &self.tools.space, &self.source.id));
        if let Some(prior) = &prior {
            if prior.content_hash != binding(self.source)?
                || prior.source.as_deref() != Some(&self.source.id)
            {
                return Err("The source tool binding changed".into());
            }
        }
        Ok(prior)
    }
    pub(super) fn update(
        &self,
        apply: impl FnOnce(&mut ResumeAttempts) -> Result<(), String>,
    ) -> Result<(), String> {
        self.tools.check_selection()?;
        let key = key(self.cx, &self.tools.space, &self.source.id);
        let binding = binding(self.source)?;
        JournalState::update(self.tools.journal, |state| {
            let prior = state
                .continuations
                .get_mut(&key)
                .ok_or("The source attempt is missing")?;
            if prior.content_hash != binding
                || prior.source.as_deref() != Some(&self.source.id)
                || prior.attempts == 0
            {
                return Err("The source tool binding changed".into());
            }
            apply(prior)?;
            Ok(true)
        })
        .map(|_| ())
    }
    pub(super) fn prepare_source(&self, prepared: &ToolAsk) -> Result<(), String> {
        let policy_hash = current_policy_hash(self.cx, self.definition)?;
        if serde_json::to_vec(prepared).map_or(true, |bytes| {
            bytes.len() > zoen_agentd::MAX_TOOL_CONTINUATION_BYTES
        }) {
            return Err("The source tool plan exceeds capacity".into());
        }
        self.update(|budget| {
            if budget.prepared.is_some() {
                return Err("A source effect still needs reconciliation".into());
            }
            match &mut budget.model_outcome {
                Some(ModelOutcome::Tools {
                    original,
                    policy_hash: retained,
                }) if retained == &policy_hash => {
                    if original.is_none() {
                        *original = Some(Box::new(prepared.clone()));
                    } else {
                        let progress = budget
                            .progress
                            .as_ref()
                            .ok_or("The source progress is missing")?;
                        if progress.round != prepared.round
                            || progress.steps != prepared.steps
                            || progress.remaining.first() != Some(&prepared.call)
                            || progress.remaining[1..] != prepared.remaining
                        {
                            return Err("The next prepared call changed the known batch".into());
                        }
                    }
                }
                _ => return Err("The source tool plan is not a known model response".into()),
            }
            budget.prepared = Some(prepared.clone());
            Ok(())
        })
    }
    pub(super) fn checkpoint_source(&self, progress: &ToolProgress) -> Result<(), String> {
        if serde_json::to_vec(progress).map_or(true, |bytes| {
            bytes.len() > zoen_agentd::MAX_TOOL_CONTINUATION_BYTES
        }) {
            return Err("The source progress exceeds capacity".into());
        }
        self.update(|budget| {
            if !matches!(budget.model_outcome, Some(ModelOutcome::Tools { .. })) {
                return Err("The source model response is unknown".into());
            }
            if let Some(prepared) = &budget.prepared {
                if prepared.round != progress.round
                    || prepared.remaining != progress.remaining
                    || progress.steps.len() != prepared.steps.len() + 1
                    || !SpaceTools::same_calls(&prepared.steps, &progress.steps)
                    || progress
                        .steps
                        .last()
                        .is_none_or(|step| step.call != prepared.call)
                {
                    return Err("The source progress changed its prepared call".into());
                }
            } else if let Some(prior) = &budget.progress {
                if prior.round > progress.round
                    || !SpaceTools::same_calls(&prior.steps, &progress.steps)
                    || (prior.steps.len() == progress.steps.len()
                        && prior.remaining != progress.remaining)
                    || progress.steps[prior.steps.len()..]
                        .iter()
                        .any(|step| !step.is_error)
                {
                    return Err("The source progress has no prepared call".into());
                }
            } else if progress.steps.iter().any(|step| !step.is_error) {
                return Err("The source progress has no prepared call".into());
            }
            budget.prepared = None;
            budget.progress = Some(progress.clone());
            Ok(())
        })
    }
}

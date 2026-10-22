//! The existing citizen turn runtime; root contracts and policy inputs stay unchanged.

use super::*;

impl Runtime {
    pub fn new(gate: Gate, brain: Box<dyn Brain>) -> Self {
        Self { gate, brain }
    }

    pub(super) fn tool_policy_sha256(&self) -> Result<String, HostError> {
        let thresholds = serde_json::to_value(&self.gate.thresholds)
            .map_err(|_| HostError::ToolContinuation("policy encoding".into()))?;
        if thresholds
            .as_object()
            .is_none_or(|fields| fields.values().any(|value| !value.is_number()))
        {
            return Err(HostError::ToolContinuation(
                "invalid policy thresholds".into(),
            ));
        }
        let encoded = serde_json::to_vec(&(
            "zoen.tool-policy.v1",
            thresholds,
            self.gate.remote.as_ref().map(|provider| provider.name()),
            self.gate.on_device.as_ref().map(|provider| provider.name()),
            gate::DecisionProvider::name(&self.gate.fallback),
            Gate::questions(Point::ToolCall),
        ))
        .map_err(|_| HostError::ToolContinuation("policy encoding".into()))?;
        Ok(hex::encode(Sha256::digest(encoded)))
    }

    pub(super) fn tool_check(
        inbound: &Turn,
        host: &dyn ToolHost,
        call: &ToolCall,
        definition: &ToolDef,
    ) -> Check {
        let gate_text = host
            .gate_public_references(call)
            .and_then(|references| references.gate_text(call, definition))
            .unwrap_or_else(|| {
                format!(
                    "mini-app tool {} ({}) {}",
                    call.name, definition.description, call.arguments
                )
            });
        let mut check = Check::new(Point::ToolCall, gate_text);
        check.user_intent = Some(inbound.text.clone());
        check
    }

    /// `Err` only when the brain failed: the caller retries rather than moving on.
    pub async fn turn(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
    ) -> Result<TurnOutcome, HostError> {
        if let Some(held) = self.inbound(inbound) {
            return Ok(held);
        }
        let out = self
            .brain
            .think(cx, history, inbound)
            .await
            .map_err(HostError::Brain)?;
        Ok(self.settle(cx, inbound, out))
    }

    /// A turn that may call `host`'s tools: up to [`MAX_TOOL_ROUNDS`] rounds, each call
    /// through the gate (as a tool call, against what the person just said) and then the
    /// host. `done` are steps an earlier, interrupted run of this same turn already took:
    /// the model reads them instead of running them again.
    pub async fn turn_with_tools(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
        host: &dyn ToolHost,
        done: Vec<ToolStep>,
    ) -> Result<TurnOutcome, HostError> {
        if let Some(held) = self.inbound(inbound) {
            return Ok(held);
        }
        if done.len() > MAX_TOOL_STEPS {
            return Err(HostError::ToolContinuation("completed-step budget".into()));
        }
        let tools = host.tools();
        self.continue_tool_turn(cx, history, inbound, host, &tools, done, 0)
            .await
    }

    /// Resumes the exact held call with a host-owned admission. It does not ask the model
    /// for a replacement proposal or repeat classification of the approved call. New and
    /// remaining calls still pass through the ordinary gate. Current authorization and
    /// exactly-once effects remain the canonical host's responsibility.
    #[allow(clippy::too_many_arguments)]
    pub async fn resume_tool_ask(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
        host: &dyn ToolHost,
        ask: ToolAsk,
        admission: &dyn ApprovedToolAdmission,
    ) -> Result<TurnOutcome, HostError> {
        let tools = self.resume_context(cx, inbound, host, &ask)?;
        if let Some(blocked) = Self::resume_block(inbound, host, &ask) {
            return Ok(blocked);
        }
        let result = admission.execute(&ask).map_err(HostError::ToolAdmission)?;
        let ToolAsk {
            call,
            mut steps,
            remaining,
            round,
            ..
        } = ask;
        steps.push(ToolStep {
            call,
            result: result.result,
            is_error: result.is_error,
        });
        Self::save_progress(
            host,
            ToolProgress {
                round,
                steps: steps.clone(),
                remaining: remaining.clone(),
            },
        )?;
        if let Some(ask) =
            self.run_tool_calls(cx, inbound, host, &tools, &mut steps, remaining, round)?
        {
            return Ok(TurnOutcome::ToolAsk { ask });
        }
        self.continue_tool_turn(cx, history, inbound, host, &tools, steps, round + 1)
            .await
    }

    pub(super) fn save_progress(
        host: &dyn ToolHost,
        progress: ToolProgress,
    ) -> Result<(), HostError> {
        progress.bounds()?;
        host.checkpoint(&progress)
            .map_err(HostError::ToolContinuation)
    }

    pub(super) fn resume_context(
        &self,
        cx: TurnContext<'_>,
        inbound: &Turn,
        host: &dyn ToolHost,
        ask: &ToolAsk,
    ) -> Result<Vec<ToolDef>, HostError> {
        ask.validate(cx, inbound)?;
        if ask.policy_sha256 != self.tool_policy_sha256()? {
            return Err(HostError::ToolContinuation("gate policy changed".into()));
        }
        let tools = host.tools();
        if tools.iter().find(|tool| tool.name == ask.call.name) != Some(&ask.definition) {
            return Err(HostError::ToolContinuation(
                "tool definition changed".into(),
            ));
        }
        Ok(tools)
    }

    pub(super) fn resume_block(
        inbound: &Turn,
        host: &dyn ToolHost,
        ask: &ToolAsk,
    ) -> Option<TurnOutcome> {
        if let Some((Outcome::Block, rule)) =
            gate::hard_rules(&Check::new(Point::ConversationMessage, &inbound.text))
        {
            return Some(TurnOutcome::Blocked { reason: rule });
        }
        gate::hard_rules(&Self::tool_check(inbound, host, &ask.call, &ask.definition)).and_then(
            |(outcome, reason)| {
                (outcome == Outcome::Block).then_some(TurnOutcome::Blocked { reason })
            },
        )
    }

    /// Resume only completed effects already verified by the trusted host/core. This
    /// supplies past results to the model, never an execution permit for a prepared call.
    #[allow(clippy::too_many_arguments)]
    pub async fn resume_tool_progress(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
        host: &dyn ToolHost,
        original: ToolAsk,
        progress: ToolProgress,
    ) -> Result<TurnOutcome, HostError> {
        let tools = self.resume_context(cx, inbound, host, &original)?;
        progress.validate(&original)?;
        if progress
            .steps
            .iter()
            .chain(original.steps.iter())
            .any(|step| !tools.iter().any(|tool| tool.name == step.call.name))
        {
            return Err(HostError::ToolContinuation(
                "completed tool is no longer registered".into(),
            ));
        }
        if let Some(blocked) = Self::resume_block(inbound, host, &original) {
            return Ok(blocked);
        }
        Self::save_progress(host, progress.clone())?;
        let ToolProgress {
            round,
            mut steps,
            remaining,
        } = progress;
        if let Some(ask) =
            self.run_tool_calls(cx, inbound, host, &tools, &mut steps, remaining, round)?
        {
            return Ok(TurnOutcome::ToolAsk { ask });
        }
        self.continue_tool_turn(cx, history, inbound, host, &tools, steps, round + 1)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn continue_tool_turn(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
        host: &dyn ToolHost,
        tools: &[ToolDef],
        mut steps: Vec<ToolStep>,
        first_round: usize,
    ) -> Result<TurnOutcome, HostError> {
        for round in first_round..=MAX_TOOL_ROUNDS {
            let offer = if round < MAX_TOOL_ROUNDS && steps.len() < MAX_TOOL_STEPS {
                tools
            } else {
                &[]
            };
            let out = self
                .brain
                .think_with_tools(cx, history, inbound, offer, &steps)
                .await
                .map_err(HostError::Brain)?;
            let BrainOut::CallTools(calls) = out else {
                return Ok(self.settle(cx, inbound, out));
            };
            if offer.is_empty() {
                break;
            }
            if let Some(ask) =
                self.run_tool_calls(cx, inbound, host, tools, &mut steps, calls, round)?
            {
                return Ok(TurnOutcome::ToolAsk { ask });
            }
        }
        // Still calling tools with none left on offer: stop rather than loop.
        Ok(TurnOutcome::Blocked {
            reason: "tools:round-limit".into(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_tool_calls(
        &self,
        cx: TurnContext<'_>,
        inbound: &Turn,
        host: &dyn ToolHost,
        tools: &[ToolDef],
        steps: &mut Vec<ToolStep>,
        calls: Vec<ToolCall>,
        round: usize,
    ) -> Result<Option<ToolAsk>, HostError> {
        let capacity = MAX_CALLS_PER_ROUND.min(MAX_TOOL_STEPS - steps.len());
        let mut calls = calls.into_iter().take(capacity);
        while let Some(call) = calls.next() {
            let (result, is_error) = match tools.iter().find(|t| t.name == call.name) {
                None => (format!("unknown tool: {}", call.name), true),
                Some(def) => {
                    let check = Self::tool_check(inbound, host, &call, def);
                    let d = self.gate.check(&check);
                    match d.outcome {
                        Outcome::Block => (format!("blocked: {}", d.rule), true),
                        Outcome::Ask => {
                            let ask = ToolAsk {
                                version: 1,
                                policy_sha256: self.tool_policy_sha256()?,
                                envelope: ToolAskEnvelope::for_turn(cx, inbound),
                                call,
                                definition: def.clone(),
                                steps: steps.clone(),
                                remaining: calls.collect(),
                                round,
                                rule: d.rule,
                            };
                            ask.validate(cx, inbound)?;
                            return Ok(Some(ask));
                        }
                        Outcome::Allow => {
                            let prepared = ToolAsk {
                                version: 1,
                                policy_sha256: self.tool_policy_sha256()?,
                                envelope: ToolAskEnvelope::for_turn(cx, inbound),
                                call: call.clone(),
                                definition: def.clone(),
                                steps: steps.clone(),
                                remaining: calls.clone().collect(),
                                round,
                                rule: d.rule,
                            };
                            prepared.validate(cx, inbound)?;
                            host.prepare(&prepared)
                                .map_err(HostError::ToolContinuation)?;
                            match host.call(&call) {
                                Ok(result) => (result, false),
                                Err(result) => (result, true),
                            }
                        }
                    }
                }
            };
            steps.push(ToolStep {
                call,
                result,
                is_error,
            });
            Self::save_progress(
                host,
                ToolProgress {
                    round,
                    steps: steps.clone(),
                    remaining: calls.clone().collect(),
                },
            )?;
        }
        Ok(None)
    }

    /// The inbound gate: `Some` when the message never reaches the brain.
    pub(super) fn inbound(&self, inbound: &Turn) -> Option<TurnOutcome> {
        let d = self
            .gate
            .check(&Check::new(Point::ConversationMessage, &inbound.text));
        match d.outcome {
            Outcome::Block => Some(TurnOutcome::Blocked { reason: d.rule }),
            Outcome::Ask => Some(TurnOutcome::AskOwner {
                draft: inbound.text.clone(),
                rule: d.rule,
            }),
            Outcome::Allow => None,
        }
    }

    pub(super) fn settle(&self, cx: TurnContext<'_>, inbound: &Turn, out: BrainOut) -> TurnOutcome {
        match out {
            BrainOut::Silence => TurnOutcome::Silence,
            BrainOut::Say(text) => self.outbound(text),
            BrainOut::CreateAgent { spec, confirmation } => {
                self.propose(cx, inbound, spec, confirmation)
            }
            // Nobody offered tools: a call is nothing to say.
            BrainOut::CallTools(_) => TurnOutcome::Silence,
        }
    }

    pub(super) fn outbound(&self, text: String) -> TurnOutcome {
        let d = self.gate.check(&Check::new(Point::Outbound, &text));
        match d.outcome {
            Outcome::Allow => TurnOutcome::Say { text },
            Outcome::Ask => TurnOutcome::AskOwner {
                draft: text,
                rule: d.rule,
            },
            Outcome::Block => TurnOutcome::Blocked { reason: d.rule },
        }
    }

    /// `create_agent` is an action: only a Creator, only for its owner, through the gate.
    /// Allow and Ask both end at the owner's approval; the owner signs or nothing exists.
    pub(super) fn propose(
        &self,
        cx: TurnContext<'_>,
        inbound: &Turn,
        spec: AgentSpec,
        confirmation: Option<String>,
    ) -> TurnOutcome {
        if cx.definition.kind != AgentKind::Creator {
            return TurnOutcome::Blocked {
                reason: "create-agent:not-a-creator".into(),
            };
        }
        if inbound.from != cx.owner {
            return TurnOutcome::Blocked {
                reason: "create-agent:not-the-owner".into(),
            };
        }
        let mut check = Check::new(
            Point::ToolCall,
            format!(
                "{CREATE_AGENT_CAPABILITY} @{}: {}",
                spec.handle,
                serde_json::to_string(&spec.definition).unwrap_or_default()
            ),
        );
        check.capability = Some(CREATE_AGENT_CAPABILITY.into());
        check.user_intent = Some(inbound.text.clone());
        let d = self.gate.check(&check);
        if d.outcome == Outcome::Block {
            return TurnOutcome::Blocked { reason: d.rule };
        }
        let confirmation = confirmation.and_then(|c| match self.outbound(c) {
            TurnOutcome::Say { text } => Some(text),
            _ => None,
        });
        TurnOutcome::ProposeAgent { spec, confirmation }
    }
}

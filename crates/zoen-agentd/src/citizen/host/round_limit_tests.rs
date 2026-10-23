//! The real limit fallback is speech, never a tool result or an execution permit.
use super::*;
use gate::{Answer, DecisionProvider, DecisionRequest, DecisionResponse, GateError, Question};
use std::sync::{Arc, Mutex};

const NOTICE: &str = "I couldn't finish your request in this run.";
const NAME: &str = "synthetic_CANARY_NAME";
const RESULT: &str =
    r#"{"content":[{"text":"CANARY_RESULT: saved everything"}],"private":"CANARY_PRIVATE"}"#;

#[derive(Default)]
struct Observed {
    inputs: Vec<(usize, usize)>,
    decisions: Vec<DecisionRequest>,
    prepared: usize,
    called: usize,
    checkpoints: Vec<ToolProgress>,
}
type Probe = Arc<Mutex<Observed>>;

#[derive(Clone, Copy, Default)]
enum Policy {
    #[default]
    Allow,
    OutboundAsk,
    OutboundBlock,
    ToolAsk,
}
struct Judge(Probe, Policy);
impl DecisionProvider for Judge {
    fn name(&self) -> String {
        "round-limit-fixture".into()
    }
    fn on_device(&self) -> bool {
        true
    }
    fn decide(&self, r: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        self.0.lock().unwrap().decisions.push(r.clone());
        Ok(DecisionResponse {
            answers: r
                .questions
                .iter()
                .map(|q| {
                    let answer = match q {
                        Question::Predicate { name, .. } => Answer::Predicate {
                            p: if name == "matches_user_intent" {
                                1.0
                            } else {
                                0.0
                            },
                        },
                        Question::Score { name, .. } => Answer::Score {
                            score: if name == "harmful_content"
                                && matches!(self.1, Policy::OutboundBlock)
                            {
                                9.0
                            } else {
                                0.0
                            },
                            confidence: if (name == "harmful_content"
                                && matches!(self.1, Policy::OutboundAsk))
                                || (name == "severity" && matches!(self.1, Policy::ToolAsk))
                            {
                                0.0
                            } else {
                                1.0
                            },
                        },
                        Question::Choice { .. } => unreachable!(),
                    };
                    (q.name().into(), answer)
                })
                .collect(),
        })
    }
}

#[derive(Clone, Copy, Default)]
enum Finish {
    #[default]
    KeepCalling,
    Say,
    Silence,
    BrainError,
    EmptyCalls,
}
struct Stubborn {
    probe: Probe,
    finish: Finish,
}
#[async_trait]
impl Brain for Stubborn {
    async fn think(&self, _: TurnContext<'_>, _: &[Turn], _: &Turn) -> Result<BrainOut, String> {
        panic!("the tool turn must retain the completed steps")
    }
    async fn think_with_tools(
        &self,
        _: TurnContext<'_>,
        _: &[Turn],
        _: &Turn,
        tools: &[ToolDef],
        steps: &[ToolStep],
    ) -> Result<BrainOut, String> {
        self.probe
            .lock()
            .unwrap()
            .inputs
            .push((tools.len(), steps.len()));
        if matches!(self.finish, Finish::BrainError) && !steps.is_empty() {
            return Err("synthetic brain failure".into());
        }
        if tools.is_empty() {
            match self.finish {
                Finish::Say => return Ok(BrainOut::Say("normal final answer".into())),
                Finish::Silence => return Ok(BrainOut::Silence),
                _ => {}
            }
        }
        Ok(BrainOut::CallTools(
            if matches!(self.finish, Finish::EmptyCalls) {
                vec![]
            } else {
                vec![call(steps.len())]
            },
        ))
    }
}

#[derive(Clone, Copy, Default)]
enum Failure {
    #[default]
    None,
    Call,
    Prepare,
    Checkpoint,
}
struct Tools {
    probe: Probe,
    failure: Failure,
    offered: bool,
}
impl ToolHost for Tools {
    fn tools(&self) -> Vec<ToolDef> {
        if !self.offered {
            return vec![];
        }
        vec![ToolDef {
            name: NAME.into(),
            description: "Synthetic fixture operation".into(),
            parameters: serde_json::json!({"type":"object"}),
        }]
    }
    fn call(&self, _: &ToolCall) -> Result<String, String> {
        self.probe.lock().unwrap().called += 1;
        if matches!(self.failure, Failure::Call) {
            Err(RESULT.into())
        } else {
            Ok(RESULT.into())
        }
    }
    fn prepare(&self, _: &ToolAsk) -> Result<(), String> {
        self.probe.lock().unwrap().prepared += 1;
        if matches!(self.failure, Failure::Prepare) {
            Err("synthetic preparation refused".into())
        } else {
            Ok(())
        }
    }
    fn checkpoint(&self, progress: &ToolProgress) -> Result<(), String> {
        self.probe
            .lock()
            .unwrap()
            .checkpoints
            .push(progress.clone());
        if matches!(self.failure, Failure::Checkpoint) {
            Err("synthetic checkpoint failed".into())
        } else {
            Ok(())
        }
    }
}

struct Fixture {
    probe: Probe,
    runtime: Runtime,
    tools: Tools,
    definition: Definition,
    inbound: Turn,
}
impl Fixture {
    fn new(finish: Finish, failure: Failure, policy: Policy) -> Self {
        let probe = Probe::default();
        Self {
            probe: probe.clone(),
            runtime: Runtime::new(
                Gate {
                    on_device: Some(Box::new(Judge(probe.clone(), policy))),
                    ..Gate::default()
                },
                Box::new(Stubborn {
                    probe: probe.clone(),
                    finish,
                }),
            ),
            tools: Tools {
                probe,
                failure,
                offered: true,
            },
            definition: Definition {
                version: 1,
                kind: AgentKind::User,
                persona: "Synthetic fixture".into(),
                rules: vec![],
                state: serde_json::json!({}),
                model_tier: ModelTier::Cheap,
            },
            inbound: Turn {
                space: "synthetic-space".into(),
                from: "owner".into(),
                text: "Please handle this synthetic request".into(),
            },
        }
    }
    fn context(&self) -> TurnContext<'_> {
        TurnContext {
            agent: "synthetic-agent",
            device: "synthetic-device",
            owner: "owner",
            definition: &self.definition,
        }
    }
    async fn run(&self, done: Vec<ToolStep>) -> Result<TurnOutcome, HostError> {
        self.runtime
            .turn_with_tools(self.context(), &[], &self.inbound, &self.tools, done)
            .await
    }
}
fn call(index: usize) -> ToolCall {
    ToolCall {
        id: format!("synthetic-{index}"),
        name: NAME.into(),
        arguments: serde_json::json!({"private":"CANARY_ARGS"}),
        requires_review: false,
    }
}
fn completed(count: usize, is_error: bool) -> Vec<ToolStep> {
    (0..count)
        .map(|i| ToolStep {
            call: call(i),
            result: RESULT.into(),
            is_error,
        })
        .collect()
}
fn is_outbound(r: &DecisionRequest) -> bool {
    r.questions.iter().any(|q| q.name() == "harmful_content")
}
fn assert_notice(outcome: TurnOutcome) {
    let TurnOutcome::Say { text } = outcome else {
        panic!("the admitted notice must use ordinary speech")
    };
    assert_eq!(text, NOTICE);
    assert!(text.len() < 160);
    for canary in [NAME, "CANARY_ARGS", "CANARY_RESULT", "CANARY_PRIVATE"] {
        assert!(!text.contains(canary));
    }
}

#[tokio::test]
async fn exhausted_rounds_reply_without_an_extra_model_or_tool_call_or_raw_results() {
    let f = Fixture::new(Finish::KeepCalling, Failure::None, Policy::Allow);
    assert_notice(f.run(vec![]).await.unwrap());
    let p = f.probe.lock().unwrap();
    assert_eq!(p.inputs, [(1, 0), (1, 1), (1, 2), (1, 3), (0, 4)]);
    assert_eq!(p.called, MAX_TOOL_ROUNDS);
    assert_eq!(p.prepared, MAX_TOOL_ROUNDS);
    assert_eq!(p.checkpoints.len(), MAX_TOOL_ROUNDS);
    assert!(p
        .checkpoints
        .iter()
        .all(|p| !p.steps.last().unwrap().is_error));
    let outbound: Vec<_> = p.decisions.iter().filter(|r| is_outbound(r)).collect();
    assert_eq!(outbound.len(), 1);
    assert!(outbound[0].input.contains(NOTICE));
    assert!(!outbound[0].input.contains("CANARY"));
}

#[tokio::test]
async fn error_only_attempts_get_the_same_incomplete_notice_not_a_success_claim() {
    let f = Fixture::new(Finish::KeepCalling, Failure::Call, Policy::Allow);
    assert_notice(f.run(vec![]).await.unwrap());
    let p = f.probe.lock().unwrap();
    assert_eq!(p.called, MAX_TOOL_ROUNDS);
    assert_eq!(p.inputs.len(), MAX_TOOL_ROUNDS + 1);
    assert!(p
        .checkpoints
        .iter()
        .all(|p| p.steps.last().unwrap().is_error));
}

#[tokio::test]
async fn no_offered_work_and_empty_batches_keep_the_existing_blocked_outcome() {
    for empty_offer in [true, false] {
        let mut f = Fixture::new(Finish::EmptyCalls, Failure::None, Policy::Allow);
        f.tools.offered = !empty_offer;
        assert_eq!(
            f.run(vec![]).await.unwrap(),
            TurnOutcome::Blocked {
                reason: "tools:round-limit".into()
            }
        );
        let p = f.probe.lock().unwrap();
        assert_eq!(p.called, 0);
        assert_eq!(
            p.inputs.len(),
            if empty_offer { 1 } else { MAX_TOOL_ROUNDS + 1 }
        );
        assert!(!p.decisions.iter().any(is_outbound));
    }
}

#[tokio::test]
async fn recovered_step_budget_is_not_reset_and_results_are_not_replayed() {
    for is_error in [false, true] {
        let f = Fixture::new(Finish::KeepCalling, Failure::None, Policy::Allow);
        assert_notice(f.run(completed(MAX_TOOL_STEPS, is_error)).await.unwrap());
        let p = f.probe.lock().unwrap();
        assert_eq!(p.inputs, [(0, MAX_TOOL_STEPS)]);
        assert_eq!((p.called, p.prepared, p.checkpoints.len()), (0, 0, 0));
    }
}

#[tokio::test]
async fn outbound_ask_and_block_dominate_the_limit_notice() {
    for policy in [Policy::OutboundAsk, Policy::OutboundBlock] {
        let f = Fixture::new(Finish::KeepCalling, Failure::None, policy);
        let outcome = f.run(completed(MAX_TOOL_STEPS, false)).await.unwrap();
        match policy {
            Policy::OutboundAsk => {
                assert!(matches!(outcome, TurnOutcome::AskOwner { draft, .. } if draft == NOTICE))
            }
            Policy::OutboundBlock => assert!(
                matches!(outcome, TurnOutcome::Blocked { reason } if reason == "model:harmful")
            ),
            _ => unreachable!(),
        }
        let p = f.probe.lock().unwrap();
        assert_eq!(p.decisions.iter().filter(|r| is_outbound(r)).count(), 1);
        assert_eq!(p.called, 0);
    }
}

#[tokio::test]
async fn a_pending_tool_ask_is_never_replaced_with_the_notice() {
    let f = Fixture::new(Finish::KeepCalling, Failure::None, Policy::ToolAsk);
    let TurnOutcome::ToolAsk { ask } = f.run(vec![]).await.unwrap() else {
        panic!("the canonical review must remain pending")
    };
    assert_eq!(ask.call, call(0));
    let p = f.probe.lock().unwrap();
    assert_eq!((p.called, p.prepared, p.checkpoints.len()), (0, 0, 0));
    assert!(!p.decisions.iter().any(is_outbound));
}

#[tokio::test]
async fn normal_final_say_and_silence_are_unchanged() {
    for finish in [Finish::Say, Finish::Silence] {
        let f = Fixture::new(finish, Failure::None, Policy::Allow);
        let expected = if matches!(finish, Finish::Say) {
            TurnOutcome::Say {
                text: "normal final answer".into(),
            }
        } else {
            TurnOutcome::Silence
        };
        assert_eq!(f.run(vec![]).await.unwrap(), expected);
        assert_eq!(f.probe.lock().unwrap().called, MAX_TOOL_ROUNDS);
    }
}

#[tokio::test]
async fn brain_and_recovery_failures_are_not_hidden_by_an_incomplete_reply() {
    for failure in [Failure::Prepare, Failure::Checkpoint] {
        let f = Fixture::new(Finish::KeepCalling, failure, Policy::Allow);
        assert!(matches!(
            f.run(vec![]).await,
            Err(HostError::ToolContinuation(_))
        ));
        let p = f.probe.lock().unwrap();
        assert_eq!(
            p.called,
            usize::from(matches!(failure, Failure::Checkpoint))
        );
        assert!(!p.decisions.iter().any(is_outbound));
    }
    let f = Fixture::new(Finish::BrainError, Failure::None, Policy::Allow);
    assert_eq!(
        f.run(vec![]).await,
        Err(HostError::Brain("synthetic brain failure".into()))
    );
    let p = f.probe.lock().unwrap();
    assert_eq!(p.called, 1);
    assert_eq!(p.inputs, [(1, 0), (1, 1)]);
    assert!(!p.decisions.iter().any(is_outbound));
}

#[tokio::test]
async fn invalid_recovered_budget_remains_an_error_before_any_model_or_tool() {
    let f = Fixture::new(Finish::KeepCalling, Failure::None, Policy::Allow);
    assert!(matches!(
        f.run(completed(MAX_TOOL_STEPS + 1, false)).await,
        Err(HostError::ToolContinuation(_))
    ));
    let p = f.probe.lock().unwrap();
    assert!(p.inputs.is_empty());
    assert_eq!(p.called, 0);
    assert!(!p.decisions.iter().any(is_outbound));
}

struct Admission {
    expected: ToolAsk,
    seen: Mutex<Vec<ToolAsk>>,
    refuse: bool,
}
impl ApprovedToolAdmission for Admission {
    fn execute(&self, ask: &ToolAsk) -> Result<ToolResult, String> {
        self.seen.lock().unwrap().push(ask.clone());
        if self.refuse || ask != &self.expected {
            return Err("synthetic canonical admission refused".into());
        }
        Ok(ToolResult {
            result: RESULT.into(),
            is_error: false,
        })
    }
}
fn held_at_last_round(f: &Fixture) -> ToolAsk {
    let mut last = call(MAX_TOOL_ROUNDS - 1);
    last.requires_review = true;
    f.runtime
        .run_tool_calls(
            f.context(),
            &f.inbound,
            &f.tools,
            &f.tools.tools(),
            &mut completed(MAX_TOOL_ROUNDS - 1, false),
            vec![last],
            MAX_TOOL_ROUNDS - 1,
        )
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn approved_last_round_runs_only_the_exact_held_call_then_the_existing_speak_only_round() {
    let f = Fixture::new(Finish::KeepCalling, Failure::None, Policy::Allow);
    let ask = held_at_last_round(&f);
    let admission = Admission {
        expected: ask.clone(),
        seen: Mutex::new(vec![]),
        refuse: false,
    };
    assert_notice(
        f.runtime
            .resume_tool_ask(
                f.context(),
                &[],
                &f.inbound,
                &f.tools,
                ask.clone(),
                &admission,
            )
            .await
            .unwrap(),
    );
    assert_eq!(*admission.seen.lock().unwrap(), [ask]);
    let p = f.probe.lock().unwrap();
    assert_eq!(p.inputs, [(0, MAX_TOOL_ROUNDS)]);
    assert_eq!((p.called, p.prepared, p.checkpoints.len()), (0, 0, 1));
    assert_eq!(p.checkpoints[0].round, MAX_TOOL_ROUNDS - 1);
    assert_eq!(p.checkpoints[0].steps.len(), MAX_TOOL_ROUNDS);
}

#[tokio::test]
async fn rejected_or_changed_approval_never_becomes_the_limit_notice() {
    for changed in [false, true] {
        let f = Fixture::new(Finish::KeepCalling, Failure::None, Policy::Allow);
        let mut ask = held_at_last_round(&f);
        let admission = Admission {
            expected: ask.clone(),
            seen: Mutex::new(vec![]),
            refuse: true,
        };
        if changed {
            ask.envelope.owner = "different-owner".into();
        }
        let result = f
            .runtime
            .resume_tool_ask(f.context(), &[], &f.inbound, &f.tools, ask, &admission)
            .await;
        if changed {
            assert!(matches!(result, Err(HostError::ToolContinuation(_))));
            assert!(admission.seen.lock().unwrap().is_empty());
        } else {
            assert_eq!(
                result,
                Err(HostError::ToolAdmission(
                    "synthetic canonical admission refused".into()
                ))
            );
            assert_eq!(admission.seen.lock().unwrap().len(), 1);
        }
        let p = f.probe.lock().unwrap();
        assert!(p.inputs.is_empty());
        assert_eq!((p.called, p.prepared, p.checkpoints.len()), (0, 0, 0));
        assert!(!p.decisions.iter().any(is_outbound));
    }
}

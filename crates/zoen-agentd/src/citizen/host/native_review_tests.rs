use super::*;
use gate::{Answer, DecisionProvider, DecisionRequest, DecisionResponse, GateError, Question};
use std::sync::Mutex;

struct Judge;
impl DecisionProvider for Judge {
    fn name(&self) -> String {
        "native-review-fixture".into()
    }
    fn on_device(&self) -> bool {
        true
    }
    fn decide(&self, r: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        Ok(DecisionResponse {
            answers: r
                .questions
                .iter()
                .map(|q| {
                    (
                        q.name().into(),
                        match q {
                            Question::Predicate { name, .. } => Answer::Predicate {
                                p: if name == "matches_user_intent" {
                                    1.0
                                } else {
                                    0.0
                                },
                            },
                            Question::Score { name, .. } => Answer::Score {
                                score: 0.0,
                                confidence: if name == "severity" && r.input.contains("gate_ask") {
                                    0.1
                                } else {
                                    1.0
                                },
                            },
                            Question::Choice { .. } => unreachable!(),
                        },
                    )
                })
                .collect(),
        })
    }
}
struct Tools(Mutex<Vec<String>>);
impl ToolHost for Tools {
    fn tools(&self) -> Vec<ToolDef> {
        [
            "page_write",
            "gate_ask_page_write",
            "ordinary_deck",
            "generic",
        ]
        .into_iter()
        .map(|n| ToolDef {
            name: n.into(),
            description: "Synthetic operation".into(),
            parameters: serde_json::json!({"type":"object"}),
        })
        .collect()
    }
    fn requires_native_review(&self, c: &ToolCall) -> bool {
        c.name.ends_with("page_write")
    }
    fn call(&self, c: &ToolCall) -> Result<String, String> {
        self.0.lock().unwrap().push(c.name.clone());
        Ok("done".into())
    }
}
struct NoBrain;
#[async_trait]
impl Brain for NoBrain {
    async fn think(&self, _: TurnContext<'_>, _: &[Turn], _: &Turn) -> Result<BrainOut, String> {
        panic!("not used")
    }
}
fn run(name: &str, args: Value) -> (Option<ToolAsk>, Vec<ToolStep>, Vec<String>) {
    let runtime = Runtime::new(
        Gate {
            on_device: Some(Box::new(Judge)),
            ..Gate::default()
        },
        Box::new(NoBrain),
    );
    let definition = Definition {
        version: 1,
        kind: AgentKind::User,
        persona: "synthetic".into(),
        rules: vec![],
        state: serde_json::json!({}),
        model_tier: ModelTier::Cheap,
    };
    let cx = TurnContext {
        agent: "agent",
        device: "device",
        owner: "owner",
        definition: &definition,
    };
    let inbound = Turn {
        space: "space".into(),
        from: "owner".into(),
        text: "Update my document".into(),
    };
    let tools = Tools(Mutex::new(vec![]));
    let mut steps = vec![];
    let ask = runtime
        .run_tool_calls(
            cx,
            &inbound,
            &tools,
            &tools.tools(),
            &mut steps,
            vec![ToolCall {
                requires_review: false,
                id: "call".into(),
                name: name.into(),
                arguments: args,
            }],
            0,
        )
        .unwrap();
    (ask, steps, tools.0.into_inner().unwrap())
}
#[test]
fn native_page_write_act_is_held_while_deck_and_generic_allow_are_unchanged() {
    let (ask, steps, ran) = run(
        "page_write",
        serde_json::json!({"text":"review this exact text"}),
    );
    assert_eq!(ask.unwrap().rule, "native:review-required");
    assert!(steps.is_empty() && ran.is_empty());
    for name in ["ordinary_deck", "generic"] {
        let (ask, steps, ran) = run(name, serde_json::json!({}));
        assert!(ask.is_none());
        assert_eq!(steps.len(), 1);
        assert_eq!(ran, [name]);
    }
}
#[test]
fn native_review_cannot_override_block_or_rewrite_gate_ask() {
    let (ask, steps, ran) = run(
        "page_write",
        serde_json::json!({"text":"rm -rf synthetic-directory"}),
    );
    assert!(ask.is_none() && ran.is_empty());
    assert_eq!(steps.len(), 1);
    assert!(steps[0].is_error);
    let (ask, steps, ran) = run("gate_ask_page_write", serde_json::json!({}));
    let ask = ask.unwrap();
    assert_ne!(ask.rule, "native:review-required");
    assert!(steps.is_empty() && ran.is_empty());
}

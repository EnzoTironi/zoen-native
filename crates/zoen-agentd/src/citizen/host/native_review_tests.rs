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

#[derive(Default)]
struct DynamicTools(Mutex<Vec<String>>);
impl ToolHost for DynamicTools {
    fn tools(&self) -> Vec<ToolDef> {
        let mut names = vec!["create_document"];
        if !self.0.lock().unwrap().is_empty() {
            names.push("edit_created_document");
        }
        names
            .into_iter()
            .map(|name| ToolDef {
                name: name.into(),
                description: "Synthetic native document operation".into(),
                parameters: serde_json::json!({"type":"object"}),
            })
            .collect()
    }
    fn refreshed_tools(&self) -> Option<Vec<ToolDef>> {
        Some(self.tools())
    }
    fn call(&self, call: &ToolCall) -> Result<String, String> {
        self.0.lock().unwrap().push(call.name.clone());
        Ok("accepted".into())
    }
}
struct DiscoveryBrain;
#[async_trait]
impl Brain for DiscoveryBrain {
    async fn think(&self, _: TurnContext<'_>, _: &[Turn], _: &Turn) -> Result<BrainOut, String> {
        panic!("tool boundary required")
    }
    async fn think_with_tools(
        &self,
        _: TurnContext<'_>,
        _: &[Turn],
        _: &Turn,
        tools: &[ToolDef],
        steps: &[ToolStep],
    ) -> Result<BrainOut, String> {
        let name = match steps.len() {
            0 => {
                assert_eq!(tools.len(), 1);
                "create_document"
            }
            1 => {
                assert!(tools
                    .iter()
                    .any(|tool| tool.name == "edit_created_document"));
                "edit_created_document"
            }
            2 => return Ok(BrainOut::Say("Created and edited".into())),
            _ => panic!("unexpected progress"),
        };
        Ok(BrainOut::CallTools(vec![ToolCall {
            id: format!("call-{}", steps.len()),
            name: name.into(),
            arguments: serde_json::json!({}),
            requires_review: false,
        }]))
    }
}
#[tokio::test]
async fn native_discovery_refresh_offers_created_artifact_only_to_the_next_model_round() {
    let runtime = Runtime::new(
        Gate {
            on_device: Some(Box::new(Judge)),
            ..Gate::default()
        },
        Box::new(DiscoveryBrain),
    );
    let definition:Definition=serde_json::from_value(serde_json::json!({"version":1,"kind":"user","persona":"Create and edit","rules":[],"state":{}})).unwrap();
    let context = TurnContext {
        agent: "agent",
        device: "device",
        owner: "owner",
        definition: &definition,
    };
    let inbound = Turn {
        space: "space".into(),
        from: "owner".into(),
        text: "Create and edit a document".into(),
    };
    let host = DynamicTools::default();
    assert!(matches!(
        runtime
            .turn_with_tools(context, &[], &inbound, &host, vec![])
            .await
            .unwrap(),
        TurnOutcome::Say { .. }
    ));
    assert_eq!(
        *host.0.lock().unwrap(),
        ["create_document", "edit_created_document"]
    );
}

#[tokio::test]
async fn a_future_tool_cannot_execute_in_the_original_frozen_batch() {
    let runtime = Runtime::new(
        Gate {
            on_device: Some(Box::new(Judge)),
            ..Gate::default()
        },
        Box::new(NoBrain),
    );
    let definition:Definition=serde_json::from_value(serde_json::json!({"version":1,"kind":"user","persona":"Create and edit","rules":[],"state":{}})).unwrap();
    let context = TurnContext {
        agent: "agent",
        device: "device",
        owner: "owner",
        definition: &definition,
    };
    let inbound = Turn {
        space: "space".into(),
        from: "owner".into(),
        text: "Create and edit a document".into(),
    };
    let host = DynamicTools::default();
    let original = host.tools();
    let mut steps = vec![];
    let calls = ["create_document", "edit_created_document"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| ToolCall {
            id: index.to_string(),
            name: name.into(),
            arguments: serde_json::json!({}),
            requires_review: false,
        })
        .collect();
    assert!(runtime
        .run_tool_calls(context, &inbound, &host, &original, &mut steps, calls, 0)
        .unwrap()
        .is_none());
    assert_eq!(*host.0.lock().unwrap(), ["create_document"]);
    assert!(!steps[0].is_error && steps[1].is_error);
}

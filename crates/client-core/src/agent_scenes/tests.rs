//! Golden intent is independent of serde_json map features and insertion order.
use super::*;
fn proposal() -> ToolProposal {
    let args = json!({"artifactId":"it_owned","version":1,"sha256":"11".repeat(32),
        "resourceUri":format!("ui://zoen/package/it_package/1/{}", "22".repeat(32)),
        "sceneJson":miniapps::excalidraw::EMPTY});
    let alias = tools::model_name("replace_scene", "it_owned").unwrap();
    let envelope: Value = serde_json::from_str(&format!(
        r#"{{"space":"space","owner":"owner","from":"owner","agent":"agent","device":"device","inbound_sha256":"{}"}}"#, "dd".repeat(32)
    )).unwrap();
    let ask = json!({"version":1,"policy_sha256":"aa".repeat(32),"envelope":envelope,
        "call":{"id":"call","name":alias,"arguments":args},"definition":{"name":alias},
        "steps":[],"remaining":[],"round":0,"rule":"native:review-required"});
    ToolProposal {
        schema: SCHEMA,
        source: "source".into(),
        serving: "owner".into(),
        source_sha256: "bb".repeat(32),
        device: "device".into(),
        owner: "owner".into(),
        task_sha256: "cc".repeat(32),
        step: 0,
        artifact: "it_owned".into(),
        version: 1,
        sha256: "11".repeat(32),
        canonical_tool: "replace_scene".into(),
        arguments_json: miniapps::deckcraft::agent_tools::canonical_json(&args).unwrap(),
        worker_binding: tools::executor_binding(),
        engine_revision: tools::REVISION.into(),
        continuation_json: ask.to_string(),
        expires_at_ms: 1_700_000_000_000,
        gate_rule: "native:review-required".into(),
        review: None,
    }
}
#[test]
fn scene_intent_keeps_exact_golden_bytes_across_map_orders() {
    let mut p = proposal();
    // Independently encoded as sorted UTF-8 JSON in Python hashlib, not another
    // call to the implementation's canonical serializer.
    let expected = "3e330b3c063c435ac416cc95599527867f91f3e97e91e3cf7432e35061aa01fb";
    assert_eq!(review::intent(&p).unwrap(), expected);
    let mut ask: Value = serde_json::from_str(&p.continuation_json).unwrap();
    ask["envelope"] = serde_json::from_str(&format!(
        r#"{{"inbound_sha256":"{}","device":"device","agent":"agent","from":"owner","owner":"owner","space":"space"}}"#, "dd".repeat(32)
    )).unwrap();
    p.continuation_json = ask.to_string();
    assert_eq!(review::intent(&p).unwrap(), expected);
    ask["envelope"]["owner"] = "different_owner".into();
    p.continuation_json = ask.to_string();
    assert_ne!(review::intent(&p).unwrap(), expected);
}
#[test]
fn scene_contract_refuses_undeclared_fields_capacity_and_changed_executor() {
    let p = proposal();
    let mut args: Value = serde_json::from_str(&p.arguments_json).unwrap();
    args["owner"] = "model_selected_owner".into();
    assert!(tools::parse("replace_scene", &args).is_err());
    assert!(tools::parse(
        "read_scene",
        &json!({"artifactId":"it_owned","sha256":"11".repeat(32)})
    )
    .is_err());
    assert!(tools::parse("read_scene", &json!({"artifactId":"../outside"})).is_err());
    let mut args: Value = serde_json::from_str(&p.arguments_json).unwrap();
    args["sceneJson"] = " ".repeat(tools::SCENE_LIMIT + 1).into();
    assert!(tools::parse("replace_scene", &args).is_err());
    let mut changed = p;
    changed.worker_binding = "00".repeat(32);
    assert!(parsed(&changed).is_err());
}

#[test]
fn renewed_approval_is_selected_even_when_expired_predecessor_is_first() {
    use crate::engine::{ReqStatus, RequestState};
    fn row(id: &str, proposal: ToolProposal) -> RequestState {
        RequestState {
            req: AgentRequest {
                id: id.into(),
                agent: "agent".into(),
                title: "Drawing".into(),
                detail: String::new(),
                audience: String::new(),
                action: ActionClass::Reversible,
                content_hash: String::new(),
                item: None,
                line: None,
                proposal: None,
                tool_proposal: Some(Box::new(proposal)),
            },
            space: "space".into(),
            opened_ms: 0,
            opened_content_hash: String::new(),
            resolved_content_hash: None,
            status: ReqStatus::Approved,
            resolved_ms: Some(0),
            review: None,
            resolution: Some(types::RequestResolution {
                message: String::new(),
                created_item: None,
                ai_cost_cents: 0,
                standing_grant: None,
                tool_decision: Some(types::ToolDecision::ApproveOnce),
            }),
        }
    }
    let at = 1000;
    let mut old = proposal();
    old.expires_at_ms = at;
    let mut current = old.clone();
    current.expires_at_ms = at + 100;
    current.review = Some(types::ToolReviewLineage {
        root: "old".into(),
        predecessor: "old".into(),
        intent: "intent".into(),
        generation: 1,
    });
    let mut state = State::default();
    state.requests.insert("old".into(), row("old", old));
    state
        .requests
        .insert("current".into(), row("current", current));
    state.request_order = vec!["old".into(), "current".into()];
    // Deliberately put the stale candidate first; HashMap iteration is irrelevant.
    let selected: Vec<_> = ["old", "current"]
        .into_iter()
        .filter(|id| effects::live_approval(&state, &state.requests[*id], at))
        .collect();
    assert_eq!(selected, vec!["current"]);
    state.requests.get_mut("current").unwrap().status = ReqStatus::Pending;
    assert!(!effects::live_approval(&state, &state.requests["old"], at));
    assert!(!effects::live_approval(
        &state,
        &state.requests["current"],
        at
    ));
}

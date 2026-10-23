use super::*;

fn held(call: ToolCall) -> (Runtime, ToolAsk, TestTools) {
    let runtime = runtime(
        vec![],
        Arc::new(Mutex::new(vec![])),
        Arc::new(Mutex::new(vec![])),
    );
    let def = definition();
    let tools = TestTools::default();
    let ask = runtime
        .run_tool_calls(
            context(&def),
            &inbound(),
            &tools,
            &tools.tools(),
            &mut vec![],
            vec![call],
            0,
        )
        .unwrap()
        .unwrap();
    assert!(tools.executed.lock().unwrap().is_empty());
    (runtime, ask, tools)
}

#[test]
fn legacy_call_json_defaults_false_and_keeps_exact_false_bytes() {
    const LEGACY: &str = r#"{"id":"legacy","name":"read_current","arguments":{"version":1}}"#;
    let legacy: ToolCall = serde_json::from_str(LEGACY).unwrap();
    assert!(!legacy.requires_review);
    assert_eq!(serde_json::to_string(&legacy).unwrap(), LEGACY);
    let mut explicit = legacy.clone();
    explicit.requires_review = true;
    assert_eq!(
        serde_json::to_string(&explicit).unwrap(),
        r#"{"id":"legacy","name":"read_current","arguments":{"version":1},"requires_review":true}"#
    );
    assert!(serde_json::from_str::<ToolCall>(
        r#"{"id":"legacy","name":"read_current","arguments":{},"requires_review":"true"}"#
    )
    .is_err());
    let (_, ask, _) = held(call("hold_first"));
    let bytes = serde_json::to_string(&ask).unwrap();
    assert!(!bytes.contains("requires_review"));
    let restored: ToolAsk = serde_json::from_str(&bytes).unwrap();
    assert_eq!(serde_json::to_string(&restored).unwrap(), bytes);
    assert_eq!(restored, ask);
}

#[test]
fn explicit_review_tightens_allow_without_preparing_or_executing() {
    let mut selected = call("read_current");
    selected.requires_review = true;
    let (_runtime, ask, tools) = held(selected.clone());
    assert_eq!(ask.rule, NATIVE_REVIEW_REQUIRED);
    assert_eq!(ask.call, selected);
    assert!(ask.steps.is_empty() && ask.remaining.is_empty());
    assert!(tools.executed.lock().unwrap().is_empty());
}

#[test]
fn block_dominates_review_and_existing_ask_keeps_its_original_rule() {
    let def = definition();
    let runtime = runtime(
        vec![],
        Arc::new(Mutex::new(vec![])),
        Arc::new(Mutex::new(vec![])),
    );
    let tools = TestTools::default();
    let mut blocked = call("read_current");
    blocked.requires_review = true;
    blocked.arguments = serde_json::json!({"text":"rm -rf synthetic-directory"});
    let mut steps = vec![];
    assert!(runtime
        .run_tool_calls(
            context(&def),
            &inbound(),
            &tools,
            &tools.tools(),
            &mut steps,
            vec![blocked],
            0
        )
        .unwrap()
        .is_none());
    assert_eq!(steps.len(), 1);
    assert!(steps[0].is_error && tools.executed.lock().unwrap().is_empty());
    let (_, ordinary, _) = held(call("hold_first"));
    let mut reviewed = call("hold_first");
    reviewed.requires_review = true;
    let (_, tightened, _) = held(reviewed);
    assert_eq!(tightened.rule, ordinary.rule);
    assert_ne!(tightened.rule, NATIVE_REVIEW_REQUIRED);
    assert!(tightened.steps.is_empty());
}

#[tokio::test]
async fn approved_review_resumes_exactly_and_flag_or_argument_changes_are_refused() {
    let mut selected = call("read_current");
    selected.requires_review = true;
    let (runtime, ask, tools) = held(selected);
    let admission = TestAdmission {
        expected: ask.clone(),
        executed: Mutex::new(vec![]),
        refuse: false,
    };
    let def = definition();
    let mut changed = ask.clone();
    changed.call.requires_review = false;
    assert!(runtime
        .resume_tool_ask(context(&def), &[], &inbound(), &tools, changed, &admission)
        .await
        .is_err());
    let mut changed = ask.clone();
    changed.call.arguments["version"] = 2.into();
    assert!(runtime
        .resume_tool_ask(context(&def), &[], &inbound(), &tools, changed, &admission)
        .await
        .is_err());
    assert!(admission.executed.lock().unwrap().is_empty());
    assert!(matches!(
        runtime
            .resume_tool_ask(
                context(&def),
                &[],
                &inbound(),
                &tools,
                ask.clone(),
                &admission
            )
            .await
            .unwrap(),
        TurnOutcome::Say { .. }
    ));
    assert_eq!(*admission.executed.lock().unwrap(), vec![ask.call]);
    assert!(
        tools.executed.lock().unwrap().is_empty(),
        "only the canonical admission executed"
    );
}

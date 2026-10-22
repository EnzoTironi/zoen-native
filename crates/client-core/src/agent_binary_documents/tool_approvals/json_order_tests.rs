//! Literal wire vectors run both with and without serde_json/preserve_order.
use super::*;

fn reverse_ordered_input(fixture: &Fixture) -> AgentToolApprovalInput {
    let turn = turn(fixture);
    let mut input = input(fixture, &turn, "deckcraft_slide_notes");
    input.arguments = serde_json::from_str(&format!(
        r#"{{"version":1,"text":"Synthetic reviewed notes","slideId":1,"sha256":"{}","artifactId":"{}"}}"#,
        fixture.base.sha256, fixture.base.item_id,
    )).unwrap();
    let mut ask: Value = serde_json::from_str(&input.continuation_json).unwrap();
    ask["call"]["arguments"] = input.arguments.clone();
    input.continuation_json = serde_json::to_string(&ask).unwrap();
    input
}

fn sorted_arguments(fixture: &Fixture) -> String {
    format!(
        r#"{{"artifactId":"{}","sha256":"{}","slideId":1,"text":"Synthetic reviewed notes","version":1}}"#,
        fixture.base.item_id, fixture.base.sha256
    )
}

#[test]
fn producer_freezes_the_same_exact_sorted_arguments_in_both_map_feature_modes() {
    let fixture = fixture();
    let request = fixture
        .agent
        .stage_tool_approval(&turn(&fixture), reverse_ordered_input(&fixture))
        .unwrap();
    assert_eq!(
        proposal(&fixture, &request.id).arguments_json,
        sorted_arguments(&fixture)
    );
    assert_v1(&fixture);
}

#[test]
fn canonical_wire_request_projects_and_approves_after_reload_in_both_map_feature_modes() {
    let fixture = fixture();
    let staged = fixture
        .agent
        .stage_tool_approval(&turn(&fixture), reverse_ordered_input(&fixture))
        .unwrap();
    let mut request = fixture.agent.lock().state.requests[&staged.id].req.clone();
    request.id = types::new_id("synthetic_cross_feature_ask");
    request.tool_proposal.as_mut().unwrap().arguments_json = sorted_arguments(&fixture);
    request.content_hash = request.tool_proposal_hash(&fixture.space).unwrap();
    let id = request.id.clone();
    let hash = request.content_hash.clone();
    let frozen = request.tool_proposal.clone();
    append_at(&fixture, request.clone(), crate::engine::now_ms());
    fixture.agent.lock().reload().unwrap();
    let retained = fixture.agent.lock().state.requests[&id].req.clone();
    assert_eq!(retained, request);
    assert_eq!(
        retained.tool_proposal_hash(&fixture.space).as_deref(),
        Some(hash.as_str())
    );
    let dto = fixture
        .agent
        .requests()
        .into_iter()
        .find(|r| r.id == id)
        .unwrap();
    assert!(dto.tool_approval.is_some());
    assert_eq!(dto.status, RequestStatus::Pending);
    owner_decide(&fixture, &id, &hash, ToolRequestDecision::ApproveOnce).unwrap();
    fixture.agent.lock().reload().unwrap();
    let admission = fixture.agent.approved_tool_turn(&id, WORKER).unwrap();
    assert_eq!(Some(admission.proposal()), frozen.as_deref());
    assert_v1(&fixture);
}

#[test]
fn noncanonical_or_altered_signed_arguments_do_not_gain_approval() {
    let fixture = fixture();
    let request = stage(&fixture, "deckcraft_slide_notes");
    let original = fixture.agent.lock().state.requests[&request.id].req.clone();
    for args in [
        sorted_arguments(&fixture).replace("Synthetic reviewed notes", "Changed without review"),
        format!(" {}", sorted_arguments(&fixture)),
        format!(
            r#"{{"artifactId":"{}","artifactId":"{}","sha256":"{}","slideId":1,"text":"Synthetic reviewed notes","version":1}}"#,
            fixture.base.item_id, fixture.base.item_id, fixture.base.sha256
        ),
        format!(
            r#"{{"version":1,"text":"Synthetic reviewed notes","slideId":1,"sha256":"{}","artifactId":"{}"}}"#,
            fixture.base.sha256, fixture.base.item_id
        ),
    ] {
        let mut changed = original.clone();
        changed.tool_proposal.as_mut().unwrap().arguments_json = args;
        changed.content_hash = changed.tool_proposal_hash(&fixture.space).unwrap();
        assert!(!fixture
            .agent
            .lock()
            .state
            .tool_request_matches(&changed, &fixture.space));
    }
    assert_eq!(
        fixture.agent.lock().state.requests[&request.id].req,
        original
    );
    assert_v1(&fixture);
}

#[test]
fn command_intent_matches_a_fixed_cross_feature_vector() {
    let fixture = fixture();
    let request = stage(&fixture, "deckcraft_slide_notes");
    let mut p = proposal(&fixture, &request.id);
    p.artifact = "it_deck_cross_feature".into();
    p.source = "01M5HKT1PY2HBV13H2XRXGFE11".into();
    p.serving = "owner-cross-feature".into();
    p.source_sha256 = "11".repeat(32);
    p.version = 1;
    p.sha256 = "22".repeat(32);
    let args: Value = serde_json::from_str(r#"{"version":1,"text":"Reviewed notes","slideId":257,"sha256":"2222222222222222222222222222222222222222222222222222222222222222","artifactId":"it_deck_cross_feature"}"#).unwrap();
    let (_, _, action) = agent_tools::command("deckcraft_slide_notes", &args).unwrap();
    assert_eq!(
        serde_json::to_string(action.as_ref().unwrap()).unwrap(),
        r#"{"command":"slide.notes","params":{"slideId":257,"text":"Reviewed notes"}}"#
    );
    let expected_wire = format!(
        r#"["it_deck_cross_feature","01M5HKT1PY2HBV13H2XRXGFE11","owner-cross-feature","{}",1,"{}","{}","deckcraft_slide_notes",{{"command":"slide.notes","params":{{"slideId":257,"text":"Reviewed notes"}}}}]"#,
        "11".repeat(32),
        "22".repeat(32),
        deckcraft::ENGINE_REVISION
    );
    assert_eq!(
        command_intent(&p, action.as_ref()).unwrap(),
        sha(expected_wire.as_bytes())
    );
}

#[test]
fn geometry_unicode_and_nested_action_bytes_match_a_fixed_cross_feature_vector() {
    let arguments: Value = serde_json::from_str(r#"{"artifactId":"it_deck_vector","version":1,"sha256":"2222222222222222222222222222222222222222222222222222222222222222","slideId":257,"preset":"rect","rect":[0,2.5,-3,120],"text":"Café 🌙\n"}"#).unwrap();
    let original = arguments.clone();
    let (_, _, action) = agent_tools::command("deckcraft_shape_insert", &arguments).unwrap();
    let expected = r#"{"command":"shape.insert","params":{"preset":"rect","rect":[0.0,2.5,-3.0,120.0],"slideId":257,"text":"Café 🌙\n"}}"#;
    assert_eq!(serde_json::to_string(&action.unwrap()).unwrap(), expected);
    assert_eq!(
        arguments, original,
        "normalization must not mutate the frozen input"
    );
}

#[test]
fn recursive_canonical_json_preserves_nested_arrays_numbers_unicode_and_input() {
    let input: Value = serde_json::from_str(
        r#"{"z":[{"z":null,"a":"Café 🌙"},[3,2,1]],"a":{"z":1.0,"b":true,"a":-2}}"#,
    )
    .unwrap();
    let original = input.clone();
    assert_eq!(
        agent_tools::canonical_json(&input).unwrap(),
        r#"{"a":{"a":-2,"b":true,"z":1.0},"z":[{"a":"Café 🌙","z":null},[3,2,1]]}"#
    );
    assert_eq!(input, original);
}

#[test]
fn signed_noncanonical_wire_is_retained_without_projection_or_approval() {
    let fixture = fixture();
    let staged = stage(&fixture, "deckcraft_slide_notes");
    let mut request = fixture.agent.lock().state.requests[&staged.id].req.clone();
    request.id = types::new_id("synthetic_noncanonical_wire");
    request.tool_proposal.as_mut().unwrap().arguments_json = format!(
        r#"{{"version":1,"text":"Synthetic reviewed notes","slideId":1,"sha256":"{}","artifactId":"{}"}}"#,
        fixture.base.sha256, fixture.base.item_id,
    );
    // This is a valid content hash over the exact noncanonical signed bytes,
    // not a stale-hash test. No migration may rewrite those bytes into authority.
    request.content_hash = request.tool_proposal_hash(&fixture.space).unwrap();
    let id = request.id.clone();
    let hash = request.content_hash.clone();
    append_at(&fixture, request.clone(), crate::engine::now_ms());
    for reload in [false, true] {
        if reload {
            fixture.agent.lock().reload().unwrap();
        }
        let engine = fixture.agent.lock();
        let event = engine.logs[&fixture.space].events().last().unwrap();
        let EventBody::RequestOpened { request: retained } = &event.body else {
            panic!("the original signed event must remain retained");
        };
        assert_eq!(retained, &request);
        assert_eq!(
            retained.tool_proposal_hash(&fixture.space).as_deref(),
            Some(hash.as_str())
        );
        assert!(!engine.state.requests.contains_key(&id));
        drop(engine);
        assert!(!fixture
            .agent
            .requests()
            .iter()
            .any(|request| request.id == id));
        assert!(owner_decide(&fixture, &id, &hash, ToolRequestDecision::ApproveOnce).is_err());
        assert!(fixture.agent.approved_tool_turn(&id, WORKER).is_err());
        assert_v1(&fixture);
    }
}

#[test]
fn precise_geometry_wire_keeps_exact_arguments_action_and_intent_after_native_parse() {
    let fixture = fixture();
    let turn = turn(&fixture);
    let mut input = input(&fixture, &turn, "deckcraft_shape_insert");
    // Real f64 values whose JSON is parsed differently without float_roundtrip.
    // The producer starts from exact typed values, as the CLI already does.
    input.arguments = json!({"artifactId":fixture.base.item_id,"version":1,"sha256":fixture.base.sha256,
        "slideId":1,"preset":"rect","rect":[51.248178375505404_f64,-93.31137037688033_f64,2.5,120.0],"text":"Café 🌙"});
    let mut ask: Value = serde_json::from_str(&input.continuation_json).unwrap();
    ask["call"]["arguments"] = input.arguments.clone();
    input.continuation_json = serde_json::to_string(&ask).unwrap();
    let request = fixture
        .agent
        .stage_tool_approval(&turn, input)
        .expect("native parsing must retain the exact canonical f64 wire");
    let p = proposal(&fixture, &request.id);
    let expected_args = format!(
        r#"{{"artifactId":"{}","preset":"rect","rect":[51.248178375505404,-93.31137037688033,2.5,120.0],"sha256":"{}","slideId":1,"text":"Café 🌙","version":1}}"#,
        fixture.base.item_id, fixture.base.sha256
    );
    assert_eq!(p.arguments_json, expected_args);
    let (_, action) = parsed(&p).unwrap();
    let expected_action = r#"{"command":"shape.insert","params":{"preset":"rect","rect":[51.248178375505404,-93.31137037688033,2.5,120.0],"slideId":1,"text":"Café 🌙"}}"#;
    assert_eq!(
        serde_json::to_string(action.as_ref().unwrap()).unwrap(),
        expected_action
    );
    let expected_intent = format!(
        r#"["{}","{}","{}","{}",1,"{}","{}","deckcraft_shape_insert",{}]"#,
        p.artifact,
        p.source,
        p.serving,
        p.source_sha256,
        p.sha256,
        p.engine_revision,
        expected_action
    );
    assert_eq!(
        command_intent(&p, action.as_ref()).unwrap(),
        sha(expected_intent.as_bytes())
    );
    fixture.agent.lock().reload().unwrap();
    owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    fixture.agent.lock().reload().unwrap();
    assert_eq!(
        fixture
            .agent
            .approved_tool_turn(&request.id, WORKER)
            .unwrap()
            .proposal(),
        &p
    );
    assert_v1(&fixture);
}

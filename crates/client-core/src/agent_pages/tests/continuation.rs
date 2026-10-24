use super::*;

#[test]
fn completed_prefix_result_bytes_are_retained_when_the_next_write_is_recovered() {
    let f = fixture().unwrap();
    let t = turn(&f);
    let c = command(&t);
    let (request, permit) = approved(&f, &t, &c);
    let first = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
    let first_call: Value = serde_json::from_str::<Value>(
        &f.actor.agent.lock().state.requests[&request.id]
            .req
            .tool_proposal
            .as_ref()
            .unwrap()
            .continuation_json,
    )
    .unwrap()["call"]
        .clone();
    let prefix =
        json!({"call":first_call,"result":serde_json::to_string(&first).unwrap(),"is_error":false});
    let t = turn(&f);
    let mut second = c.clone();
    second.version = t.snapshot.version;
    second.prefix_sha256 = t.snapshot.prefix_sha256.clone();
    second.content_sha256 = t.snapshot.content_sha256.clone();
    second.before = c.after.clone();
    second.after.text = "Second reviewed replacement".into();
    let mut input = input(
        &f,
        &t,
        "replace_page_block",
        serde_json::to_value(&second).unwrap(),
    );
    let mut ask: Value = serde_json::from_str(&input.continuation_json).unwrap();
    ask["steps"] = json!([prefix]);
    input.continuation_json = ask.to_string();
    let request = f.actor.agent.stage_page_tool_approval(&t, input).unwrap();
    decide(&f, &request, ToolRequestDecision::ApproveOnce).unwrap();
    let permit = f.actor.agent.approved_page_tool_turn(&request.id).unwrap();
    f.actor
        .agent
        .agent_page_replace(permit.turn(), &second)
        .unwrap();
    let steps = json!([prefix,{"call":ask["call"],"result":"","is_error":false}]);
    let recovered: Value = serde_json::from_str(
        &f.actor
            .agent
            .recover_page_tool_results(&request.id, &steps.to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        recovered[0], prefix,
        "Runtime requires its original completed prefix byte-for-byte"
    );
    assert_eq!(
        serde_json::from_str::<AgentPageResult>(recovered[1]["result"].as_str().unwrap())
            .unwrap()
            .observed_base_version,
        2
    );
}

#[test]
fn completed_read_and_write_are_recovered_from_exact_shadow_and_publication_not_claimed_results() {
    let f = fixture().unwrap();
    let t = turn(&f);
    let c = command(&t);
    let read = json!({"call":{"id":"read-before-write","name":tools::model_name("read_page",&f.page).unwrap(),"arguments":{"artifactId":f.page}},"result":serde_json::to_string(&t.snapshot).unwrap(),"is_error":false});
    let mut input = input(
        &f,
        &t,
        "replace_page_block",
        serde_json::to_value(&c).unwrap(),
    );
    let mut ask: Value = serde_json::from_str(&input.continuation_json).unwrap();
    ask["steps"] = json!([read]);
    input.continuation_json = ask.to_string();
    let request = f.actor.agent.stage_page_tool_approval(&t, input).unwrap();
    decide(&f, &request, ToolRequestDecision::ApproveOnce).unwrap();
    let permit = f.actor.agent.approved_page_tool_turn(&request.id).unwrap();
    let result = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
    let steps = json!([read,{"call":ask["call"],"result":"","is_error":false}]);
    let before = counts(&f);
    let recovered: Value = serde_json::from_str(
        &f.actor
            .agent
            .recover_page_tool_results(&request.id, &steps.to_string())
            .unwrap(),
    )
    .unwrap();
    let actual: AgentPageResult =
        serde_json::from_str(recovered[1]["result"].as_str().unwrap()).unwrap();
    assert_eq!(actual.client_id, result.client_id);
    assert_eq!(actual.applied_content_sha256, result.applied_content_sha256);
    assert!(actual.replayed);
    let mut altered = steps.clone();
    let mut snapshot = t.snapshot.clone();
    snapshot.blocks[0].text = "Forged context".into();
    altered[0]["result"] = serde_json::to_string(&snapshot).unwrap().into();
    assert!(f
        .actor
        .agent
        .recover_page_tool_results(&request.id, &altered.to_string())
        .is_err());
    let mut altered = steps.clone();
    altered[1]["call"]["arguments"]["after"]["text"] = "Unapproved replacement".into();
    assert!(f
        .actor
        .agent
        .recover_page_tool_results(&request.id, &altered.to_string())
        .is_err());
    let mut altered = steps;
    let mut claimed = result;
    claimed.client_id = "different-publication".into();
    altered[1]["result"] = serde_json::to_string(&claimed).unwrap().into();
    assert!(f
        .actor
        .agent
        .recover_page_tool_results(&request.id, &altered.to_string())
        .is_err());
    assert_eq!(counts(&f), before);
    block(&f).unwrap();
    assert!(f
        .actor
        .agent
        .recover_page_tool_results(&request.id, &recovered.to_string())
        .is_err());
}

#[test]
fn missing_bounded_receipt_after_publication_is_stale_not_permission_to_reexecute() {
    let f = fixture().unwrap();
    let t = turn(&f);
    let c = command(&t);
    let (request, permit) = approved(&f, &t, &c);
    let accepted = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
    assert_eq!(accepted.observed_base_version, 1);
    // Equivalent to lost/evicted protected receipt, with signed history retained.
    f.actor
        .agent
        .lock()
        .store
        .conn()
        .execute("DELETE FROM meta WHERE key LIKE 'page.draft.%'", [])
        .unwrap();
    let permit = f.actor.agent.approved_page_tool_turn(&request.id).unwrap();
    let before = counts(&f);
    assert!(f.actor.agent.agent_page_replace(permit.turn(), &c).is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(page(&f).version, 2);
}

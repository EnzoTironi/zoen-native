use super::*;
use crate::{
    agent_binary_documents::tests::{fixture_with_privacy, Fixture},
    AgentToolApprovalInput, ToolRequestDecision,
};
use types::{Capability, EventBody, Grant, GrantScope, TrustLevel};

fn setup() -> Fixture {
    let f = fixture_with_privacy(Privacy::EndToEnd);
    let mut e = f.agent.lock();
    let actor = e.me_id().unwrap();
    e.append(
        &f.space,
        &f.owner,
        EventBody::GrantIssued {
            grant: Grant {
                id: types::new_id("site_trust"),
                grantor: f.owner.clone(),
                grantee: Some(actor),
                scope: GrantScope::Space(f.space.clone()),
                capability: Capability::Trust(TrustLevel::Act),
                expires_at_ms: None,
            },
        },
    )
    .unwrap();
    drop(e);
    f
}
fn input(f: &Fixture, tool: &str, args: Value, steps: Value) -> AgentToolApprovalInput {
    let e = f.agent.lock();
    let actor = e.me_id().unwrap();
    let (device, owner, _, _) = e
        .agent_site_authority(&f.space, &f.source, &f.owner, true)
        .unwrap();
    let (_, inbound) = shared::source_hash(&e.state, &f.space, &f.source, &f.owner).unwrap();
    let locator = args["artifactId"].as_str().unwrap_or(&f.space);
    let definition = if tool == "create_site" {
        definition(tool, locator).unwrap()
    } else {
        selected_definition(
            tool,
            locator,
            args["version"].as_u64().unwrap() as u32,
            args["sha256"].as_str().unwrap(),
        )
        .unwrap()
    };
    let ask = json!({"version":1,"policy_sha256":"77".repeat(32),"envelope":{"agent":actor,"device":device,"owner":owner,"space":f.space,"from":f.owner,"inbound_sha256":inbound},"call":{"requires_review":true,"id":format!("site-{}",steps.as_array().unwrap().len()),"name":definition["name"],"arguments":args},"definition":definition,"steps":steps,"remaining":[],"round":0,"rule":"native:review-required"});
    AgentToolApprovalInput {
        canonical_tool: tool.into(),
        arguments: args,
        worker_binding: executor_binding(),
        continuation_json: ask.to_string(),
        gate_rule: "native:review-required".into(),
    }
}
fn stage(f: &Fixture, tool: &str, args: Value, steps: Value) -> crate::AgentRequestDto {
    f.agent
        .stage_site_tool_approval(&f.space, &f.source, &f.owner, input(f, tool, args, steps))
        .unwrap()
}
fn decide(
    f: &Fixture,
    r: &crate::AgentRequestDto,
    decision: ToolRequestDecision,
) -> R<crate::DecideOutcome> {
    let mut e = f.agent.lock();
    let actor = e.me.replace(f.owner.clone());
    let synced = e.net.synced.remove(&f.space);
    let result = e.decide_tool_request(
        &r.id,
        &r.tool_approval.as_ref().unwrap().content_hash,
        decision,
    );
    if synced {
        e.net.synced.insert(f.space.clone());
    }
    e.me = actor;
    result
}
fn execute(f: &Fixture, r: &crate::AgentRequestDto) -> Value {
    decide(f, r, ToolRequestDecision::ApproveOnce).unwrap();
    let p = f.agent.approved_site_tool(&r.id).unwrap();
    serde_json::from_str(&f.agent.execute_site_tool(&p).unwrap()).unwrap()
}
fn create_args() -> Value {
    json!({"title":"Shared Site","html":"<!doctype html><h1>Private α</h1>"})
}

#[test]
fn contracts_refuse_authority_hosting_fields_and_bound_exact_unicode() {
    assert!(parse("create_site", &create_args()).is_ok());
    for field in ["ownerId", "deviceId", "publicOrigin", "url", "publish"] {
        let mut args = create_args();
        args[field] = json!("foreign");
        assert!(parse("create_site", &args).is_err());
    }
    let mut args = create_args();
    args["html"] = json!("x".repeat(usize::try_from(HTML_LIMIT).unwrap() + 1));
    assert!(parse("create_site", &args).is_err());
    args["html"] = json!("a\0b");
    assert!(parse("create_site", &args).is_err());
    assert!(parse("publish_site", &json!({})).is_err());
}
#[test]
fn proposal_is_inert_then_one_approved_file_survives_projection_reload() {
    let f = setup();
    let before = f.agent.items().len();
    let r = stage(&f, "create_site", create_args(), json!([]));
    assert_eq!(f.agent.items().len(), before);
    assert!(f.agent.approved_site_tool(&r.id).is_err());
    assert_eq!(stage(&f, "create_site", create_args(), json!([])).id, r.id);
    let accepted = execute(&f, &r);
    assert_eq!(accepted["version"], 1);
    assert_eq!(f.agent.items().len(), before + 1);
    f.agent.lock().reload().unwrap();
    let p = f.agent.approved_site_tool(&r.id).unwrap();
    let again: Value = serde_json::from_str(&f.agent.execute_site_tool(&p).unwrap()).unwrap();
    assert_eq!(again["itemId"], accepted["itemId"]);
    assert_eq!(again["replayed"], true);
    assert_eq!(f.agent.items().len(), before + 1);
    let source = f
        .agent
        .agent_site_read(
            accepted["itemId"].as_str().unwrap(),
            1,
            accepted["sha256"].as_str().unwrap(),
            &f.source,
            &f.owner,
        )
        .unwrap();
    assert_eq!(source.html, create_args()["html"].as_str().unwrap());
}
#[test]
fn reviewed_edit_recovers_receipt_without_a_third_version_and_keeps_old_bytes() {
    let f = setup();
    let r = stage(&f, "create_site", create_args(), json!([]));
    let created = execute(&f, &r);
    let p = f.agent.approved_site_tool(&r.id).unwrap();
    let ask: Value = serde_json::from_str(&p.proposal().continuation_json).unwrap();
    let steps = json!([{"call":ask["call"],"result":created.to_string(),"is_error":false}]);
    let args = json!({"artifactId":created["itemId"],"version":1,"sha256":created["sha256"],"html":"<!doctype html><h1>Reviewed v2</h1>"});
    let edit = stage(&f, "replace_site_source", args, steps);
    let changed = execute(&f, &edit);
    assert_eq!(changed["version"], 2);
    let p = f.agent.approved_site_tool(&edit.id).unwrap();
    let ask: Value = serde_json::from_str(&p.proposal().continuation_json).unwrap();
    let mut lost = ask["steps"].as_array().unwrap().clone();
    lost.push(json!({"call":ask["call"],"result":"","is_error":false}));
    f.agent.lock().reload().unwrap();
    let recovered = f
        .agent
        .recover_site_tool_results(&edit.id, &serde_json::to_string(&lost).unwrap())
        .unwrap();
    assert!(recovered.contains("replayed"));
    let again: Value = serde_json::from_str(&f.agent.execute_site_tool(&p).unwrap()).unwrap();
    assert_eq!(again["version"], 2);
    let old = f
        .agent
        .agent_site_read(
            created["itemId"].as_str().unwrap(),
            1,
            created["sha256"].as_str().unwrap(),
            &f.source,
            &f.owner,
        )
        .unwrap();
    assert_eq!(old.html, create_args()["html"].as_str().unwrap());
    assert!(f.agent.verify_all().iter().all(|v| v.valid));
}
#[test]
fn denied_modified_source_and_revoked_writer_never_create() {
    for mode in 0..3 {
        let f = setup();
        let before = f.agent.items().len();
        let r = stage(&f, "create_site", create_args(), json!([]));
        if mode == 0 {
            decide(&f, &r, ToolRequestDecision::Deny).unwrap();
            assert!(f.agent.approved_site_tool(&r.id).is_err());
        } else {
            decide(&f, &r, ToolRequestDecision::ApproveOnce).unwrap();
            let p = f.agent.approved_site_tool(&r.id).unwrap();
            let mut e = f.agent.lock();
            if mode == 1 {
                e.append(
                    &f.space,
                    &f.owner,
                    EventBody::MessageAnnotated {
                        target: f.source.clone(),
                        note: types::Annotation::Edit {
                            text: "Changed request".into(),
                        },
                    },
                )
                .unwrap();
            } else {
                e.append(
                    &f.space,
                    &f.owner,
                    EventBody::MemberAdded {
                        identity: f.owner.clone(),
                        role: types::Role::Reader,
                    },
                )
                .unwrap();
            }
            drop(e);
            assert!(f.agent.execute_site_tool(&p).is_err());
        }
        assert_eq!(f.agent.items().len(), before);
    }
}
#[test]
fn no_receipt_cannot_be_recovered_and_atomic_failure_leaves_no_file() {
    let f = setup();
    let r = stage(&f, "create_site", create_args(), json!([]));
    decide(&f, &r, ToolRequestDecision::ApproveOnce).unwrap();
    let p = f.agent.approved_site_tool(&r.id).unwrap();
    let ask: Value = serde_json::from_str(&p.proposal().continuation_json).unwrap();
    let missing = json!([{"call":ask["call"],"result":"","is_error":false}]);
    assert!(f
        .agent
        .recover_site_tool_results(&r.id, &missing.to_string())
        .is_err());
    let before = f.agent.items().len();
    f.agent.lock().store.conn().execute_batch("CREATE TRIGGER fail_site_effect BEFORE INSERT ON meta WHEN NEW.key LIKE 'site_tool:%' BEGIN SELECT RAISE(ABORT,'receipt failed'); END;").unwrap();
    assert!(f.agent.execute_site_tool(&p).is_err());
    assert_eq!(f.agent.items().len(), before);
}

#[test]
fn reviewed_base_hash_and_current_writer_are_rechecked_before_an_edit() {
    let f = setup();
    let r = stage(&f, "create_site", create_args(), json!([]));
    let created = execute(&f, &r);
    let p = f.agent.approved_site_tool(&r.id).unwrap();
    let ask: Value = serde_json::from_str(&p.proposal().continuation_json).unwrap();
    let steps = json!([{"call":ask["call"],"result":created.to_string(),"is_error":false}]);
    let args = json!({"artifactId":created["itemId"],"version":1,"sha256":created["sha256"],"html":"<h1>Approved edit</h1>"});
    let mut wrong = input(&f, "replace_site_source", args.clone(), steps.clone());
    wrong.arguments["sha256"] = json!("00".repeat(32));
    assert!(f
        .agent
        .stage_site_tool_approval(&f.space, &f.source, &f.owner, wrong)
        .is_err());
    let edit = stage(&f, "replace_site_source", args, steps);
    decide(&f, &edit, ToolRequestDecision::ApproveOnce).unwrap();
    let permit = f.agent.approved_site_tool(&edit.id).unwrap();
    let item = created["itemId"].as_str().unwrap();
    f.agent
        .lock()
        .file_new_version_checked(
            item,
            1,
            created["sha256"].as_str().unwrap(),
            b"<h1>Concurrent edit</h1>",
            None,
            "",
        )
        .unwrap();
    assert!(f.agent.execute_site_tool(&permit).is_err());
    let info = f.agent.lock().file_info_at(item, Some(2)).unwrap();
    let current = f
        .agent
        .agent_site_read(item, 2, &info.sha256, &f.source, &f.owner)
        .unwrap();
    assert_eq!(current.html, "<h1>Concurrent edit</h1>");
    assert!(f.agent.lock().file_info_at(item, Some(3)).is_err());
}

#[test]
fn human_preview_is_read_only_and_requires_exact_owner_device_and_request_hash() {
    let f = setup();
    let r = stage(&f, "create_site", create_args(), json!([]));
    let hash = &r.tool_approval.as_ref().unwrap().content_hash;
    let before = f.agent.items().len();
    let mut e = f.agent.lock();
    let actor = e.me.replace(f.owner.clone());
    // This synthetic offline fixture deliberately has no authenticated human
    // account on the agent device. Merely switching personas cannot mint review.
    assert!(e
        .site_tool_review(&f.owner, "foreign-device", &r.id, hash)
        .is_err());
    assert!(e
        .site_tool_review(&f.owner, "foreign-device", &r.id, &"00".repeat(32))
        .is_err());
    e.me = actor;
    drop(e);
    assert_eq!(f.agent.items().len(), before);
    assert!(f.agent.approved_site_tool(&r.id).is_err());
}

#[test]
fn lost_receipt_metadata_cannot_duplicate_an_existing_canonical_file() {
    let f = setup();
    let request = stage(&f, "create_site", create_args(), json!([]));
    let created = execute(&f, &request);
    let permit = f.agent.approved_site_tool(&request.id).unwrap();
    let before = f.agent.items().len();
    f.agent
        .lock()
        .store
        .conn()
        .execute_batch("DELETE FROM meta WHERE key LIKE 'site_tool:%' OR key LIKE 'file_send:%'")
        .unwrap();
    f.agent.lock().reload().unwrap();
    assert!(f.agent.execute_site_tool(&permit).is_err());
    assert_eq!(f.agent.items().len(), before);
    assert!(f
        .agent
        .agent_site_read(
            created["itemId"].as_str().unwrap(),
            1,
            created["sha256"].as_str().unwrap(),
            &f.source,
            &f.owner
        )
        .is_ok());
}

#[test]
fn receipt_identity_is_independent_of_the_envelopes_json_field_order() {
    let f = setup();
    let r = stage(&f, "create_site", create_args(), json!([]));
    let request = f
        .agent
        .tool_approval_requests(&f.space, &f.source)
        .into_iter()
        .find(|v| v.id == r.id)
        .unwrap();
    let p = request.tool_proposal.unwrap();
    let original = review::intent(&p).unwrap();
    let mut reordered = (*p).clone();
    let mut ask: Value = serde_json::from_str(&p.continuation_json).unwrap();
    let reverse: serde_json::Map<String, Value> = ask["envelope"]
        .as_object()
        .unwrap()
        .iter()
        .rev()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    ask["envelope"] = Value::Object(reverse);
    reordered.continuation_json = serde_json::to_string(&ask).unwrap();
    assert_eq!(review::intent(&reordered).unwrap(), original);
}

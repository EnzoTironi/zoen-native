use super::*;
use crate::{
    agent_binary_documents::tests::{fixture, Fixture},
    AgentToolApprovalInput, ToolRequestDecision,
};
use types::{Capability, EventBody, Grant, GrantScope, TrustLevel};

pub(super) fn setup() -> std::result::Result<Fixture, Box<dyn std::error::Error>> {
    let f = fixture()?;
    let mut e = f.agent.lock();
    let actor = e.me_id().unwrap();
    e.append(
        &f.space,
        &f.owner,
        EventBody::GrantIssued {
            grant: Grant {
                id: types::new_id("creation_trust")?,
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
    Ok(f)
}
pub(super) fn input(f: &Fixture, requested_title: &str) -> AgentToolApprovalInput {
    let e = f.agent.lock();
    let actor = e.me_id().unwrap();
    let (device, owner, _, _) = e.creation_authority(&f.space, &f.source, &f.owner).unwrap();
    let (_, inbound) = shared::source_hash(&e.state, &f.space, &f.source, &f.owner).unwrap();
    let definition = definition_unchecked(&f.space).unwrap();
    let arguments = json!({"title":requested_title});
    let ask = json!({"version":1,"policy_sha256":"77".repeat(32),
        "envelope":{"agent":actor,"device":device,"owner":owner,"space":f.space,"from":f.owner,"inbound_sha256":inbound},
        "call":{"requires_review":false,"id":"create-test","name":model_name(&f.space).unwrap(),"arguments":arguments},
        "definition":definition,"steps":[],"remaining":[],"round":0,"rule":"native:review-required"});
    AgentToolApprovalInput {
        canonical_tool: TOOL.into(),
        arguments,
        worker_binding: executor_binding(),
        continuation_json: ask.to_string(),
        gate_rule: "native:review-required".into(),
    }
}
fn stage(f: &Fixture) -> crate::AgentRequestDto {
    f.agent
        .stage_app_creation_approval(
            &f.space,
            &f.source,
            &f.owner,
            input(f, "Reviewed presentation"),
        )
        .unwrap()
}
pub(super) fn decide(
    f: &Fixture,
    id: &str,
    hash: &str,
    decision: ToolRequestDecision,
) -> R<crate::DecideOutcome> {
    let mut e = f.agent.lock();
    let actor = e.me.replace(f.owner.clone());
    let synced = e.net.synced.remove(&f.space);
    let result = e.decide_tool_request(id, hash, decision);
    if synced {
        e.net.synced.insert(f.space.clone());
    }
    e.me = actor;
    result
}
fn approve(f: &Fixture, request: &crate::AgentRequestDto) {
    decide(
        f,
        &request.id,
        &request.tool_approval.as_ref().unwrap().content_hash,
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
}

#[test]
fn staging_is_inert_deduplicated_and_approval_replays_one_native_creation() {
    let f = setup().unwrap();
    let before = f.agent.items().len();
    let request = stage(&f);
    assert_eq!(f.agent.items().len(), before);
    assert_eq!(stage(&f).id, request.id);
    assert_eq!(
        request.tool_approval.as_ref().unwrap().artifact_title,
        "Reviewed presentation"
    );
    assert!(f.agent.approved_app_creation(&request.id).is_err());
    assert!(decide(
        &f,
        &request.id,
        &"00".repeat(32),
        ToolRequestDecision::ApproveOnce
    )
    .is_err());
    approve(&f, &request);
    let permit = f.agent.approved_app_creation(&request.id).unwrap();
    let accepted: Value =
        serde_json::from_str(&f.agent.execute_app_creation(&permit).unwrap()).unwrap();
    assert_eq!(accepted["itemId"], permit.proposal().artifact);
    assert_eq!(accepted["version"], 1);
    assert_eq!(accepted["replayed"], false);
    assert_eq!(f.agent.items().len(), before + 1);
    let repeated: Value =
        serde_json::from_str(&f.agent.execute_app_creation(&permit).unwrap()).unwrap();
    assert_eq!(repeated["replayed"], true);
    assert_eq!(repeated["itemId"], accepted["itemId"]);
    assert_eq!(f.agent.items().len(), before + 1);
    let document = f
        .agent
        .app_binary_read(
            permit.proposal().artifact.clone(),
            Some(1),
            Some(permit.proposal().sha256.clone()),
        )
        .unwrap();
    assert_eq!(
        document.data_base64,
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            registration::EMPTY
        )
    );
    assert!(f.agent.verify_all().iter().all(|result| result.valid));
}

#[test]
fn denial_cancellation_and_modified_source_never_create_a_document() {
    for decision in [ToolRequestDecision::Deny, ToolRequestDecision::Cancel] {
        let f = setup().unwrap();
        let before = f.agent.items().len();
        let request = stage(&f);
        decide(
            &f,
            &request.id,
            &request.tool_approval.as_ref().unwrap().content_hash,
            decision,
        )
        .unwrap();
        assert!(f.agent.approved_app_creation(&request.id).is_err());
        assert_eq!(f.agent.items().len(), before);
    }
    let f = setup().unwrap();
    let request = stage(&f);
    approve(&f, &request);
    let permit = f.agent.approved_app_creation(&request.id).unwrap();
    let before = f.agent.items().len();
    f.agent
        .lock()
        .append(
            &f.space,
            &f.owner,
            EventBody::MessageAnnotated {
                target: f.source.clone(),
                note: types::Annotation::Edit {
                    text: "A different request".into(),
                },
            },
        )
        .unwrap();
    assert!(f.agent.execute_app_creation(&permit).is_err());
    assert_eq!(f.agent.items().len(), before);
}

#[test]
fn unexpected_fields_different_executor_or_owner_binding_cannot_stage_a_creation() {
    let f = setup().unwrap();
    for change in 0..5 {
        let mut ask = input(&f, "Reviewed presentation");
        match change {
            0 => ask.arguments["url"] = json!("https://untrusted.invalid/code"),
            1 => ask.worker_binding = "00".repeat(32),
            2 => {
                let mut continuation: Value = serde_json::from_str(&ask.continuation_json).unwrap();
                continuation["envelope"]["owner"] = json!("foreign");
                ask.continuation_json = continuation.to_string();
            }
            3 => {
                let mut continuation: Value = serde_json::from_str(&ask.continuation_json).unwrap();
                continuation["definition"]["parameters"] = json!({"type":"object"});
                ask.continuation_json = continuation.to_string();
            }
            _ => {
                // A future artifact must not become callable through the old
                // held batch after an effect-before-journal crash/reconstruction.
                let mut continuation: Value = serde_json::from_str(&ask.continuation_json).unwrap();
                continuation["remaining"] = json!([{"id":"guessed-future-call","name":"read_deckcraft__future","arguments":{},"requires_review":false}]);
                ask.continuation_json = continuation.to_string();
            }
        }
        assert!(f
            .agent
            .stage_app_creation_approval(&f.space, &f.source, &f.owner, ask)
            .is_err());
    }
    assert!(f
        .agent
        .tool_approval_requests(&f.space, &f.source)
        .is_empty());
}

#[test]
fn lost_journal_result_recovers_only_an_existing_native_receipt() {
    let f = setup().unwrap();
    let request = stage(&f);
    approve(&f, &request);
    let permit = f.agent.approved_app_creation(&request.id).unwrap();
    let ask: Value = serde_json::from_str(&permit.proposal().continuation_json).unwrap();
    let missing = json!([{"call":ask["call"],"result":"","is_error":false}]).to_string();
    assert!(f
        .agent
        .recover_app_creation_results(&request.id, &missing)
        .is_err());
    let before = f.agent.items().len();
    f.agent.execute_app_creation(&permit).unwrap();
    let recovered: Value = serde_json::from_str(
        &f.agent
            .recover_app_creation_results(&request.id, &missing)
            .unwrap(),
    )
    .unwrap();
    let result: Value = serde_json::from_str(recovered[0]["result"].as_str().unwrap()).unwrap();
    assert_eq!(result["itemId"], permit.proposal().artifact);
    assert_eq!(result["replayed"], true);
    assert_eq!(f.agent.items().len(), before + 1);
    let mut forged = recovered;
    forged[0]["result"] = json!(json!({"itemId":"foreign","version":1}).to_string());
    assert!(f
        .agent
        .recover_app_creation_results(&request.id, &forged.to_string())
        .is_err());
}

#[test]
fn revoked_writer_and_expired_signed_request_cannot_create_or_reauthorize(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = setup().unwrap();
    let request = stage(&f);
    approve(&f, &request);
    let permit = f.agent.approved_app_creation(&request.id).unwrap();
    let before = f.agent.items().len();
    f.agent
        .lock()
        .append(
            &f.space,
            &f.owner,
            EventBody::MemberAdded {
                identity: f.owner.clone(),
                role: types::Role::Reader,
            },
        )
        .unwrap();
    assert!(f.agent.execute_app_creation(&permit).is_err());
    assert_eq!(f.agent.items().len(), before);

    let f = setup().unwrap();
    let request = stage(&f);
    let e = f.agent.lock();
    let mut expired = e.state.requests[&request.id].req.clone();
    expired.id = types::new_id("expired_creation").unwrap();
    expired.tool_proposal.as_mut().unwrap().expires_at_ms = crate::engine::now_ms() - 1;
    expired.content_hash = expired.tool_proposal_hash(&f.space).unwrap();
    let id = expired.id.clone();
    let observed = expired.content_hash.clone();
    let opened = expired.tool_proposal.as_ref().unwrap().expires_at_ms - 1000;
    drop(e);
    shared::tests::append_at(&f, expired, opened)?;
    assert!(decide(&f, &id, &observed, ToolRequestDecision::ApproveOnce).is_err());
    assert!(f.agent.approved_app_creation(&id).is_err());
    assert_eq!(f.agent.items().len(), before);
    Ok(())
}

#[test]
fn renewed_creation_keeps_the_future_item_but_requires_its_own_signed_decision(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::agent_binary_documents::tests::renewal_history::{
        confirm_pending, expired_approved,
    };
    let f = setup().unwrap();
    let live = stage(&f);
    let old = expired_approved(&f, live).unwrap();
    let old_permit = f.agent.approved_app_creation(&old.id).unwrap();
    assert!(f.agent.execute_app_creation(&old_permit).is_err());
    decide(
        &f,
        &old.id,
        &old.tool_approval.as_ref().unwrap().content_hash,
        ToolRequestDecision::RequestReview,
    )
    .unwrap();
    let intent = f.agent.tool_review_pending_intent(&old.id).unwrap();
    let next = f
        .agent
        .stage_tool_review_successor(&old.id, &intent)
        .unwrap()
        .unwrap();
    assert_eq!(
        f.agent.lock().state.requests[&next.id]
            .req
            .tool_proposal
            .as_ref()
            .unwrap()
            .artifact,
        old_permit.proposal().artifact
    );
    assert!(f.agent.approved_app_creation(&next.id).is_err());
    assert_eq!(
        f.agent
            .stage_tool_review_successor(&old.id, &intent)
            .unwrap(),
        Some(next.clone())
    );
    confirm_pending(&f)?;
    approve(&f, &next);
    let permit = f.agent.approved_app_creation(&next.id).unwrap();
    let before = f.agent.items().len();
    let created: Value =
        serde_json::from_str(&f.agent.execute_app_creation(&permit).unwrap()).unwrap();
    assert_eq!(created["itemId"], old_permit.proposal().artifact);
    assert_eq!(f.agent.items().len(), before + 1);
    // The fulfilled predecessor may read the same native receipt; it cannot
    // produce another effect or substitute the successor's independent decision.
    let replay: Value =
        serde_json::from_str(&f.agent.execute_app_creation(&old_permit).unwrap()).unwrap();
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["itemId"], created["itemId"]);
    assert_eq!(f.agent.items().len(), before + 1);
    assert!(f.agent.verify_all().iter().all(|result| result.valid));
    Ok(())
}

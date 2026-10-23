//! Page review generations use the same owner intent and stable command receipt.
use super::*;
use crate::agent_binary_documents::tests::renewal_history::{confirm_pending, expired_approved};

#[test]
fn page_renewal_preserves_exact_block_intent_and_requires_a_new_approval() {
    let f = fixture();
    let t = turn(&f);
    let command = command(&t);
    let old = expired_approved(&f.actor, stage(&f, &t, &command));
    // The Page wrapper also supports historical receipt recovery. Constructing
    // it is not fresh IO authority: the actual expired write must still refuse.
    let expired_permit = f.actor.agent.approved_page_tool_turn(&old.id).unwrap();
    let before = counts(&f);
    assert!(f
        .actor
        .agent
        .agent_page_replace(expired_permit.turn(), &command)
        .is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(page(&f).version, 1);
    decide(&f, &old, ToolRequestDecision::RequestReview).unwrap();
    let intent = f.actor.agent.tool_review_pending_intent(&old.id).unwrap();
    let next = f
        .actor
        .agent
        .stage_tool_review_successor(&old.id, &intent)
        .unwrap()
        .unwrap();
    assert_eq!(next.status, crate::RequestStatus::Pending);
    assert!(f.actor.agent.approved_page_tool_turn(&next.id).is_err());
    confirm_pending(&f.actor);
    decide(&f, &next, ToolRequestDecision::ApproveOnce).unwrap();
    let permit = f.actor.agent.approved_page_tool_turn(&next.id).unwrap();
    assert_eq!(permit.proposal().schema, 2);
    let old_proposal = f.actor.agent.lock().state.requests[&old.id]
        .req
        .tool_proposal
        .clone()
        .unwrap();
    assert_eq!(
        approvals::intent(&old_proposal).unwrap(),
        approvals::intent(permit.proposal()).unwrap()
    );
    assert_eq!(
        old_proposal.arguments_json,
        permit.proposal().arguments_json
    );
    assert_eq!(
        old_proposal.continuation_json,
        permit.proposal().continuation_json
    );
    let saved = f
        .actor
        .agent
        .agent_page_replace(permit.turn(), &command)
        .unwrap();
    assert_eq!(saved.observed_base_version, 1);
    assert_eq!(page(&f).version, 2);
    let before = counts(&f);
    assert_eq!(
        f.actor
            .agent
            .agent_page_replace(permit.turn(), &command)
            .unwrap()
            .client_id,
        saved.client_id
    );
    assert_eq!(counts(&f), before);
}

#[test]
fn accepted_page_after_owner_intent_reconciles_without_a_fresh_pending_request() {
    let f = fixture();
    let t = turn(&f);
    let c = command(&t);
    let live = stage(&f, &t, &c);
    let old = expired_approved(&f.actor, live.clone());
    decide(&f, &live, ToolRequestDecision::ApproveOnce).unwrap();
    let permit = f.actor.agent.approved_page_tool_turn(&live.id).unwrap();
    decide(&f, &old, ToolRequestDecision::RequestReview).unwrap();
    let intent = f.actor.agent.tool_review_pending_intent(&old.id).unwrap();
    let saved = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
    let before = counts(&f);
    assert_eq!(
        f.actor
            .agent
            .stage_tool_review_successor(&old.id, &intent)
            .unwrap(),
        None
    );
    assert_eq!(counts(&f), before);
    assert_eq!(page(&f).content_hash, saved.applied_content_sha256);
}

fn later_step(f: &PageFixture) {
    confirm_pending(&f.actor);
    let t = turn(f);
    let mut call = input(f, &t, "read_page", json!({"artifactId":f.page}));
    let mut ask: Value = serde_json::from_str(&call.continuation_json).unwrap();
    let mut completed = ask["call"].clone();
    completed["id"] = json!("completed-prefix");
    ask["steps"] = json!([{"call":completed,"result":"{}","is_error":false}]);
    ask["call"]["id"] = json!("next-page-call");
    ask["round"] = json!(1);
    call.continuation_json = ask.to_string();
    f.actor.agent.stage_page_tool_approval(&t, call).unwrap();
    confirm_pending(&f.actor);
}

#[test]
fn renewed_page_pending_or_approved_write_cannot_execute_after_a_later_step() {
    for approved_before in [false, true] {
        let f = fixture();
        let t = turn(&f);
        let command = command(&t);
        let old = expired_approved(&f.actor, stage(&f, &t, &command));
        decide(&f, &old, ToolRequestDecision::RequestReview).unwrap();
        let intent = f.actor.agent.tool_review_pending_intent(&old.id).unwrap();
        let next = f
            .actor
            .agent
            .stage_tool_review_successor(&old.id, &intent)
            .unwrap()
            .unwrap();
        confirm_pending(&f.actor);
        let permit = if approved_before {
            decide(&f, &next, ToolRequestDecision::ApproveOnce).unwrap();
            confirm_pending(&f.actor);
            Some(f.actor.agent.approved_page_tool_turn(&next.id).unwrap())
        } else {
            None
        };
        later_step(&f);
        let before = counts(&f);
        if let Some(permit) = permit {
            assert!(f
                .actor
                .agent
                .agent_page_replace(permit.turn(), &command)
                .is_err());
        } else {
            assert!(decide(&f, &next, ToolRequestDecision::ApproveOnce).is_err());
        }
        assert_eq!(counts(&f), before);
        assert_eq!(page(&f).version, 1);
    }
}

#[test]
fn accepted_renewed_page_receipt_survives_a_later_canonical_step() {
    let f = fixture();
    let t = turn(&f);
    let command = command(&t);
    let old = expired_approved(&f.actor, stage(&f, &t, &command));
    decide(&f, &old, ToolRequestDecision::RequestReview).unwrap();
    let intent = f.actor.agent.tool_review_pending_intent(&old.id).unwrap();
    let next = f
        .actor
        .agent
        .stage_tool_review_successor(&old.id, &intent)
        .unwrap()
        .unwrap();
    confirm_pending(&f.actor);
    decide(&f, &next, ToolRequestDecision::ApproveOnce).unwrap();
    confirm_pending(&f.actor);
    let permit = f.actor.agent.approved_page_tool_turn(&next.id).unwrap();
    let saved = f
        .actor
        .agent
        .agent_page_replace(permit.turn(), &command)
        .unwrap();
    later_step(&f);
    let before = counts(&f);
    assert_eq!(
        f.actor
            .agent
            .stage_tool_review_successor(&old.id, &intent)
            .unwrap(),
        None
    );
    assert_eq!(
        f.actor
            .agent
            .agent_page_replace(permit.turn(), &command)
            .unwrap()
            .client_id,
        saved.client_id
    );
    assert_eq!(counts(&f), before);
    assert_eq!(page(&f).version, 2);
}

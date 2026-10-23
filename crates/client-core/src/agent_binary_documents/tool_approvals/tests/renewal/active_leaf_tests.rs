//! Actual signed later steps cannot be renewed through an older review card.
use super::*;

fn later_step(f: &Fixture) -> AgentRequestDto {
    let turn = turn(f);
    let mut input = input(f, &turn, "read_deckcraft");
    let mut ask: Value = serde_json::from_str(&input.continuation_json).unwrap();
    let mut completed = ask["call"].clone();
    completed["id"] = json!("already-completed-prefix");
    ask["steps"] = json!([{"call":completed,"result":"{}","is_error":false}]);
    ask["call"]["id"] = json!("later-pending-call");
    ask["round"] = json!(1);
    input.continuation_json = ask.to_string();
    let request = f.agent.stage_tool_approval(&turn, input).unwrap();
    confirm_pending(f);
    let p = proposal(f, &request.id);
    assert_eq!(p.step, 1);
    assert_eq!(request.status, RequestStatus::Pending);
    assert_eq!(
        f.agent
            .tool_approval_requests(&f.space, &f.source)
            .last()
            .unwrap()
            .id,
        request.id
    );
    request
}

#[test]
fn later_signed_pending_step_refuses_older_owner_intent_and_available_projection() {
    let f = fixture();
    let (old, _) = expired_approved(&f, "read_deckcraft");
    let next = later_step(&f);
    let before = counts(&f);
    assert!(owner_decide(
        &f,
        &old.id,
        observed(&old),
        ToolRequestDecision::RequestReview
    )
    .is_err());
    assert_eq!(counts(&f), before);
    assert!(f.agent.tool_review_pending_intent(&old.id).is_none());
    let mut e = f.agent.lock();
    let me = e.me.replace(f.owner.clone());
    assert_eq!(
        e.request_dto(&e.state.requests[&old.id])
            .tool_approval
            .unwrap()
            .review
            .unwrap()
            .state,
        ToolReviewState::Unavailable
    );
    e.me = me;
    assert_eq!(
        f.agent
            .requests()
            .iter()
            .find(|r| r.id == next.id)
            .unwrap()
            .status,
        RequestStatus::Pending
    );
}

#[test]
fn later_signed_step_retires_prior_requested_intent_without_staging_a_successor() {
    let f = fixture();
    let (old, _) = expired_approved(&f, "read_deckcraft");
    let intent = request_again(&f, &old);
    let next = later_step(&f);
    let before = counts(&f);
    assert!(f.agent.tool_review_pending_intent(&old.id).is_none());
    assert!(f
        .agent
        .stage_tool_review_successor(&old.id, &intent)
        .is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(
        f.agent
            .requests()
            .iter()
            .find(|r| r.id == old.id)
            .unwrap()
            .tool_approval
            .as_ref()
            .unwrap()
            .review
            .as_ref()
            .unwrap()
            .state,
        ToolReviewState::Reconcile
    );
    assert_eq!(
        f.agent
            .tool_approval_requests(&f.space, &f.source)
            .last()
            .unwrap()
            .id,
        next.id
    );
}

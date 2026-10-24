//! Whole decisions cases using the original shared fixture and canonical store.

use super::*;

pub(super) fn denial_and_cancellation_are_terminal_idempotent_without_tools_files_or_standing_grants(
) {
    for (decision, status) in [
        (ToolRequestDecision::Deny, RequestStatus::Denied),
        (ToolRequestDecision::Cancel, RequestStatus::Cancelled),
    ] {
        let fixture = fixture();
        let request = stage(&fixture, "read_deckcraft");
        let before = counts(&fixture);
        let outcome = owner_decide(&fixture, &request.id, observed(&request), decision).unwrap();
        assert_eq!(outcome.request.status, status);
        assert!(outcome.standing_grant_id.is_none());
        let after = counts(&fixture);
        assert_eq!(after[1..4], before[1..4]);
        assert_eq!(
            owner_decide(&fixture, &request.id, observed(&request), decision).unwrap(),
            outcome
        );
        assert!(owner_decide(
            &fixture,
            &request.id,
            observed(&request),
            ToolRequestDecision::ApproveOnce
        )
        .is_err());
        assert!(fixture
            .agent
            .approved_tool_turn(&request.id, WORKER)
            .is_err());
        assert_eq!(counts(&fixture), after);
        assert_v1(&fixture);
    }
}

pub(super) fn expired_signed_request_never_approves_and_can_be_cancelled_without_a_clock_sleep(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture();
    let original = stage(&fixture, "read_deckcraft");
    let mut expired = fixture.agent.lock().state.requests[&original.id]
        .req
        .clone();
    expired.id = types::new_id("synthetic_expired_ask")?;
    expired.tool_proposal.as_mut().unwrap().expires_at_ms = crate::engine::now_ms() - 1000;
    expired.content_hash = expired.tool_proposal_hash(&fixture.space).unwrap();
    let opened = expired.tool_proposal.as_ref().unwrap().expires_at_ms - 1000;
    let id = expired.id.clone();
    let hash = expired.content_hash.clone();
    append_at(&fixture, expired, opened)?;
    assert!(fixture.agent.lock().state.requests.contains_key(&id));
    let before = counts(&fixture);
    assert!(owner_decide(&fixture, &id, &hash, ToolRequestDecision::ApproveOnce).is_err());
    assert!(fixture.agent.approved_tool_turn(&id, WORKER).is_err());
    assert_eq!(counts(&fixture), before);
    assert_eq!(
        owner_decide(&fixture, &id, &hash, ToolRequestDecision::Cancel)
            .unwrap()
            .request
            .status,
        RequestStatus::Cancelled
    );
    assert_v1(&fixture);

    Ok(())
}

pub(super) fn signed_grant_block_cannot_be_overridden_by_an_owner_approval_request(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture();
    signed_trust(&fixture, TrustLevel::Listen)?;
    let turn = turn(&fixture);
    let before = counts(&fixture);
    assert!(fixture
        .agent
        .stage_tool_approval(&turn, input(&fixture, &turn, "deckcraft_slide_notes"))
        .is_err());
    assert_eq!(counts(&fixture), before);
    assert_v1(&fixture);

    Ok(())
}

pub(super) fn generic_standing_decision_never_resolves_or_labels_a_pending_tool_request() {
    let fixture = fixture();
    let tool = stage(&fixture, "read_deckcraft");
    let mut engine = fixture.agent.lock();
    let agent = engine.me_id().unwrap();
    let generic = engine
        .open_request(
            &fixture.space,
            &agent,
            "Legacy request",
            "Exact legacy detail",
            "This conversation",
            ActionClass::Reversible,
            None,
        )
        .unwrap()
        .unwrap();
    let prior = engine.me.replace(fixture.owner.clone());
    engine
        .decide_request(&generic, RequestDecision::AlwaysApprove)
        .unwrap();
    engine.me = prior;
    let request = engine
        .requests()
        .into_iter()
        .find(|request| request.id == tool.id)
        .unwrap();
    assert_eq!(request.status, RequestStatus::Pending);
    assert!(!request.by_standing);
    assert!(!request.can_always_approve);
}

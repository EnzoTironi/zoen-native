//! Expired requests which were never approved renew only as a fresh owner review.
use super::pending_history::*;
use super::*;

#[test]
fn expired_pending_binary_review_preserves_original_and_requires_new_approval() -> TestResult {
    let f = fixture()?;
    let live = stage(&f, "read_deckcraft");
    let old = expired_pending(&f, &live)?;
    let original = f.agent.lock().state.requests[&old.id].clone();
    assert!(owner_decide(
        &f,
        &old.id,
        observed(&old),
        ToolRequestDecision::ApproveOnce
    )
    .is_err());
    let before = counts(&f);
    let outcome = owner_decide(
        &f,
        &old.id,
        observed(&old),
        ToolRequestDecision::RequestReview,
    )?;
    assert!(outcome.standing_grant_id.is_none());
    let intent = f
        .agent
        .tool_review_pending_intent(&old.id)
        .ok_or("owner intent")?;
    let after_intent = counts(&f);
    assert_eq!(
        &after_intent[1..4],
        &before[1..4],
        "intent creates no media or effect"
    );
    let next = f
        .agent
        .stage_tool_review_successor(&old.id, &intent)?
        .ok_or("pending successor")?;
    assert_eq!(next.status, RequestStatus::Pending);
    assert!(f.agent.approved_tool_turn(&next.id, WORKER).is_err());
    assert!(f.agent.approved_tool_turn(&old.id, WORKER).is_err());
    let current = f.agent.lock().state.requests[&old.id].clone();
    assert_eq!(
        current.req, original.req,
        "old TTL and frozen call remain unchanged"
    );
    assert_eq!(current.resolution, original.resolution);
    confirm_pending(&f)?;
    owner_decide(
        &f,
        &next.id,
        observed(&next),
        ToolRequestDecision::ApproveOnce,
    )?;
    confirm_pending(&f)?;
    assert!(f.agent.approved_tool_turn(&next.id, WORKER).is_ok());
    assert!(f.agent.approved_tool_turn(&old.id, WORKER).is_err());
    assert_v1(&f);
    Ok(())
}

#[test]
fn pending_review_refuses_unexpired_denied_cancelled_and_wrong_owner_intents() -> TestResult {
    for decision in [
        None,
        Some(ToolRequestDecision::Deny),
        Some(ToolRequestDecision::Cancel),
    ] {
        let f = fixture()?;
        let live = stage(&f, "read_deckcraft");
        let old = if decision.is_some() {
            expired_pending(&f, &live)?
        } else {
            live
        };
        if let Some(decision) = decision {
            owner_decide(&f, &old.id, observed(&old), decision)?;
        }
        let before = counts(&f);
        assert!(owner_decide(
            &f,
            &old.id,
            observed(&old),
            ToolRequestDecision::RequestReview
        )
        .is_err());
        assert!(f
            .agent
            .lock()
            .decide_tool_request(&old.id, observed(&old), ToolRequestDecision::RequestReview)
            .is_err());
        assert!(f.agent.tool_review_pending_intent(&old.id).is_none());
        assert_eq!(counts(&f), before);
    }
    Ok(())
}

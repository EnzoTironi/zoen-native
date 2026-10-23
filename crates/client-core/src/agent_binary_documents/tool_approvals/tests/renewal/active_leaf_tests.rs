//! Actual signed later steps cannot be renewed through an older review card.
use super::*;

fn later_step(f: &Fixture) -> AgentRequestDto {
    let turn = f
        .agent
        .agent_binary_turn(&f.base.item_id, &f.source, &f.owner, None, None)
        .unwrap();
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
    signed(
        &f,
        &f.owner,
        EventBody::ToolReviewRequested {
            request: old.id.clone(),
            content_hash: observed(&old).into(),
        },
        crate::engine::now_ms(),
    );
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
    let mut forged = f.agent.lock().state.requests[&old.id].req.clone();
    forged.id = types::new_id("obsolete-successor");
    let p = forged.tool_proposal.as_mut().unwrap();
    p.expires_at_ms = crate::engine::now_ms() + 60_000;
    p.review = Some(types::ToolReviewLineage {
        root: old.id.clone(),
        predecessor: old.id.clone(),
        intent: intent.clone(),
        generation: 1,
    });
    forged.content_hash = forged.tool_proposal_hash(&f.space).unwrap();
    let forged_id = forged.id.clone();
    append_at(&f, forged, crate::engine::now_ms());
    assert!(!f.agent.lock().state.requests.contains_key(&forged_id));
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

#[test]
fn renewed_pending_or_approved_read_retires_when_a_later_step_is_canonical() {
    for approved_before in [false, true] {
        let f = fixture();
        let (old, _) = expired_approved(&f, "read_deckcraft");
        let intent = request_again(&f, &old);
        let next = f
            .agent
            .stage_tool_review_successor(&old.id, &intent)
            .unwrap()
            .unwrap();
        confirm_pending(&f);
        let permit = if approved_before {
            owner_decide(
                &f,
                &next.id,
                observed(&next),
                ToolRequestDecision::ApproveOnce,
            )
            .unwrap();
            confirm_pending(&f);
            Some(f.agent.approved_tool_turn(&next.id, WORKER).unwrap())
        } else {
            None
        };
        later_step(&f);
        let before = counts(&f);
        if let Some(permit) = permit {
            assert!(f.agent.agent_binary_finish_read(permit.turn()).is_err());
            assert!(f.agent.approved_tool_turn(&next.id, WORKER).is_err());
        } else {
            assert!(owner_decide(
                &f,
                &next.id,
                observed(&next),
                ToolRequestDecision::ApproveOnce
            )
            .is_err());
            assert_eq!(
                f.agent
                    .requests()
                    .iter()
                    .find(|r| r.id == next.id)
                    .unwrap()
                    .status,
                RequestStatus::Stale
            );
        }
        assert_eq!(counts(&f), before);
        // An independently signed owner approval also cannot override leaf order.
        if !approved_before {
            signed(
                &f,
                &f.owner,
                EventBody::RequestResolved {
                    request: next.id.clone(),
                    approved: true,
                    content_hash: observed(&next).into(),
                    resolution: Some(types::RequestResolution {
                        message: "Approved once".into(),
                        created_item: None,
                        ai_cost_cents: 0,
                        standing_grant: None,
                        tool_decision: Some(ToolDecision::ApproveOnce),
                    }),
                },
                crate::engine::now_ms(),
            );
            assert_eq!(
                f.agent.lock().state.requests[&next.id].status,
                ReqStatus::Pending
            );
        }
    }
}

#[test]
fn accepted_renewed_deck_receipt_recovers_before_obsolete_leaf_refusal() {
    let f = fixture();
    signed_trust(&f, TrustLevel::Suggest);
    let (old, _) = expired_approved(&f, "deckcraft_slide_notes");
    let review = request_again(&f, &old);
    let next = f
        .agent
        .stage_tool_review_successor(&old.id, &review)
        .unwrap()
        .unwrap();
    confirm_pending(&f);
    owner_decide(
        &f,
        &next.id,
        observed(&next),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    confirm_pending(&f);
    let permit = f.agent.approved_tool_turn(&next.id, WORKER).unwrap();
    let (mutation, digest) = intent(permit.proposal());
    let saved = f
        .agent
        .agent_binary_save(permit.turn(), &mutation, &digest, &STANDARD.encode(CHANGED))
        .unwrap();
    confirm_pending(&f);
    later_step(&f);
    let before = counts(&f);
    assert_eq!(
        f.agent
            .stage_tool_review_successor(&old.id, &review)
            .unwrap(),
        None
    );
    assert_eq!(
        f.agent
            .agent_binary_save(permit.turn(), &mutation, &digest, &STANDARD.encode(CHANGED))
            .unwrap(),
        saved
    );
    assert_eq!(counts(&f), before);
}

#[test]
fn leaf_order_uses_only_matching_agent_space_source_task_and_canonical_order() {
    // Predicate unit coverage complements the signed admission/race fixtures.
    let f = fixture();
    let (old, _) = expired_approved(&f, "read_deckcraft");
    let mut e = f.agent.lock();
    let original = e.state.requests[&old.id].clone();
    assert!(e.state.tool_review_active_leaf(&original));
    for scope in ["agent", "space", "source", "task"] {
        let mut other = original.clone();
        other.req.id = format!("isolated-{scope}");
        other.req.tool_proposal.as_mut().unwrap().step = 15;
        match scope {
            "agent" => other.req.agent = "other-agent".into(),
            "space" => other.space = "other-space".into(),
            "source" => other.req.tool_proposal.as_mut().unwrap().source = "other-source".into(),
            "task" => other.req.tool_proposal.as_mut().unwrap().task_sha256 = "00".repeat(32),
            _ => unreachable!(),
        }
        e.state.request_order.push(other.req.id.clone());
        e.state.requests.insert(other.req.id.clone(), other.clone());
        assert!(e.state.tool_review_active_leaf(&original), "{scope}");
        e.state.requests.remove(&other.req.id);
        e.state.request_order.pop();
    }
    // Same-step generations outrank old roots; higher step outranks generation.
    let mut generation = original.clone();
    generation.req.id = "ordered-generation".into();
    generation.req.tool_proposal.as_mut().unwrap().review = Some(types::ToolReviewLineage {
        root: old.id.clone(),
        predecessor: old.id.clone(),
        intent: "signed-intent".into(),
        generation: 99,
    });
    e.state.request_order.push(generation.req.id.clone());
    e.state
        .requests
        .insert(generation.req.id.clone(), generation.clone());
    assert!(!e.state.tool_review_active_leaf(&original));
    let mut step = original.clone();
    step.req.id = "ordered-step".into();
    step.req.tool_proposal.as_mut().unwrap().step = 1;
    e.state.request_order.push(step.req.id.clone());
    e.state.requests.insert(step.req.id.clone(), step.clone());
    assert!(e.state.tool_review_active_leaf(&step));
    assert!(!e.state.tool_review_active_leaf(&generation));
}

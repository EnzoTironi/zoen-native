use super::*;

fn message_ids(fixture: &Fixture) -> Vec<String> {
    fixture
        .agent
        .timeline(fixture.space.clone())
        .unwrap()
        .into_iter()
        .filter(|entry| matches!(entry.kind, EntryKind::Message { .. }))
        .map(|entry| entry.id)
        .collect()
}

fn canonical_decision_projection(
    decision: ToolRequestDecision,
    status: RequestStatus,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let request = stage(&fixture, "read_deckcraft");
    let before = message_ids(&fixture);
    let original = proposal(&fixture, &request.id);
    let outcome = owner_decide(&fixture, &request.id, observed(&request), decision).unwrap();
    assert_eq!(outcome.request.status, status);
    let approved = decision == ToolRequestDecision::ApproveOnce;
    let event = {
        let engine = fixture.agent.lock();
        engine.logs[&fixture.space].events().last().unwrap().clone()
    };
    let EventBody::RequestResolved {
        request: resolved,
        approved: event_approved,
        content_hash,
        resolution: Some(receipt),
    } = &event.body
    else {
        panic!("normal decision must retain its signed RequestResolved event")
    };
    assert_eq!(resolved, &request.id);
    assert_eq!(*event_approved, approved);
    assert_eq!(content_hash, observed(&request));
    assert_eq!(event.author, fixture.owner);
    assert_eq!(receipt.message, outcome.message);
    assert!(receipt.tool_decision.is_some());
    let synthetic_source = format!("{}:result", event.client_id);

    Ok(for reload in [false, true] {
        if reload {
            fixture.agent.lock().reload().unwrap();
        }
        let timeline = fixture.agent.timeline(fixture.space.clone()).unwrap();
        assert!(
            !timeline.iter().any(|entry| entry.id == synthetic_source),
            "a tool decision receipt must never become a fresh conversation source"
        );
        assert_eq!(message_ids(&fixture), before);
        let system = timeline
            .iter()
            .find(|entry| entry.id == event.client_id)
            .expect("the RequestResolved System entry remains visible");
        assert!(matches!(system.kind, EntryKind::System { .. }));
        assert_eq!(system.author.id, fixture.owner);
        assert_eq!(system.seq, event.seq);
        assert!(timeline.iter().any(|entry| {
            matches!(&entry.kind, EntryKind::Request { request_id, approved: Some(value), .. }
                if request_id == &request.id && *value == approved)
        }));
        let retained = fixture
            .agent
            .requests()
            .into_iter()
            .find(|value| value.id == request.id)
            .unwrap();
        assert_eq!(retained.status, status);
        assert_eq!(retained.tool_approval, outcome.request.tool_approval);
        assert_eq!(proposal(&fixture, &request.id), original);
        assert_eq!(
            fixture
                .agent
                .tool_approval_requests(&fixture.space, &fixture.source)
                .len(),
            1
        );
        assert!(fixture
            .agent
            .tool_approval_requests(&fixture.space, &synthetic_source)
            .is_empty());
        assert!(fixture
            .agent
            .agent_binary_turn(
                &fixture.base.item_id,
                &synthetic_source,
                &fixture.owner,
                None,
                None,
            )
            .is_err());
        let sources = fixture.agent.approved_tool_sources(&fixture.space);
        if approved {
            assert_eq!(sources, vec![fixture.source.clone()]);
            let admission = fixture
                .agent
                .approved_tool_turn(&request.id, WORKER)
                .unwrap();
            assert_eq!(admission.proposal(), &original);
        } else {
            assert!(sources.is_empty());
            assert!(fixture
                .agent
                .approved_tool_turn(&request.id, WORKER)
                .is_err());
        }
        let engine = fixture.agent.lock();
        assert_eq!(engine.logs[&fixture.space].events().last(), Some(&event));
        assert_eq!(
            engine.state.requests[&request.id].resolution.as_ref(),
            Some(receipt)
        );
    })
}

#[test]
fn approved_tool_receipt_preserves_original_continuation_without_a_new_message_task() {
    canonical_decision_projection(ToolRequestDecision::ApproveOnce, RequestStatus::Approved)
        .unwrap();
}

#[test]
fn denied_tool_receipt_preserves_decision_without_a_new_message_task() {
    canonical_decision_projection(ToolRequestDecision::Deny, RequestStatus::Denied).unwrap();
}

#[test]
fn cancelled_tool_receipt_preserves_decision_without_a_new_message_task() {
    canonical_decision_projection(ToolRequestDecision::Cancel, RequestStatus::Cancelled).unwrap();
}

#[test]
fn legacy_item_proposal_keeps_its_delivery_message_and_item_after_reprojection() {
    for approved in [true, false] {
        let fixture = fixture().unwrap();
        signed_trust(&fixture, TrustLevel::Suggest).unwrap();
        let item = types::new_id("synthetic_legacy_plan").unwrap();
        let message = "The reviewed synthetic plan is ready.";
        let (agent, request, event) = {
            let mut engine = fixture.agent.lock();
            let agent = engine.me_id().unwrap();
            let outcome = engine
                .propose_item(
                    &fixture.space,
                    &agent,
                    "Create this synthetic plan",
                    "Synthetic plan".into(),
                    types::ItemProposal {
                        item: item.clone(),
                        kind: types::ItemKind::Plan,
                        content: types::ItemContent::Plan(types::PlanDoc {
                            title: "Synthetic plan".into(),
                            summary: "Reviewed synthetic body".into(),
                            budget_cents: None,
                            sections: vec![types::PlanSection {
                                title: "Synthetic section".into(),
                                lines: vec![types::PlanLine {
                                    id: "synthetic-line".into(),
                                    text: "Reviewed task".into(),
                                    cost_cents: 0,
                                    done: false,
                                }],
                            }],
                        }),
                        origin: "Synthetic legacy projection fixture".into(),
                        message: message.into(),
                        ai_cost_cents: 0,
                    },
                )
                .unwrap();
            assert_eq!(outcome.kind, DecisionKind::Request);
            let request = engine.requests().into_iter().next().unwrap();
            assert!(request.tool_approval.is_none());
            let prior = engine.me.replace(fixture.owner.clone());
            let synced = engine.net.synced.remove(&fixture.space);
            let outcome = engine.resolve_request(&request.id, approved).unwrap();
            if synced {
                engine.net.synced.insert(fixture.space.clone());
            }
            engine.me = prior;
            assert!(!outcome.message.is_empty());
            let event = engine.logs[&fixture.space].events().last().unwrap().clone();
            (agent, request, event)
        };
        for reload in [false, true] {
            if reload {
                fixture.agent.lock().reload().unwrap();
            }
            let timeline = fixture.agent.timeline(fixture.space.clone()).unwrap();
            let delivery = timeline
                .iter()
                .find(|entry| entry.id == format!("{}:result", event.client_id))
                .expect("legacy proposal delivery remains a message");
            let EntryKind::Message { text, card } = &delivery.kind else {
                panic!("legacy proposal keeps its delivery presentation")
            };
            if approved {
                assert_eq!(delivery.author.id, agent);
                assert_eq!(text, message);
                assert_eq!(card.as_ref().unwrap().item_id, item);
                assert!(fixture.agent.item(item.clone()).is_ok());
            } else {
                assert_eq!(delivery.author.id, fixture.owner);
                assert!(card.is_none());
                assert!(fixture.agent.item(item.clone()).is_err());
            }
            assert!(timeline.iter().any(|entry| entry.id == event.client_id
                && matches!(entry.kind, EntryKind::System { .. })));
            assert!(fixture
                .agent
                .requests()
                .iter()
                .any(|value| value.id == request.id));
        }
    }
}

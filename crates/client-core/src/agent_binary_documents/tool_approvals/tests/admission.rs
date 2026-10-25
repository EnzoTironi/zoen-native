//! Whole admission cases using the original shared fixture and canonical store.

use super::*;

pub(super) fn canonical_request_is_deduplicated_and_generic_or_wrong_owner_decisions_never_authorize(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    signed_trust(&fixture, TrustLevel::Suggest)?;
    let request = stage(&fixture, "deckcraft_slide_notes");
    assert_eq!(request.status, RequestStatus::Pending);
    assert!(!request.can_always_approve);
    assert!(request.item_id.is_none());
    assert_eq!(request.tool_approval.as_ref().unwrap().version, 1);
    let before = counts(&fixture);
    assert_eq!(stage(&fixture, "deckcraft_slide_notes"), request);
    assert_eq!(counts(&fixture), before);
    assert_eq!(
        fixture
            .agent
            .tool_approval_requests(&fixture.space, &fixture.source)
            .len(),
        1
    );
    for decision in [
        RequestDecision::Approve,
        RequestDecision::Deny,
        RequestDecision::AlwaysApprove,
        RequestDecision::AlwaysDeny,
    ] {
        let mut engine = fixture.agent.lock();
        let agent = engine.me.replace(fixture.owner.clone());
        assert!(engine.decide_request(&request.id, decision).is_err());
        engine.me = agent;
    }
    assert!(fixture
        .agent
        .lock()
        .decide_tool_request(
            &request.id,
            observed(&request),
            ToolRequestDecision::ApproveOnce
        )
        .is_err());
    assert!(owner_decide(
        &fixture,
        &request.id,
        &"00".repeat(32),
        ToolRequestDecision::ApproveOnce
    )
    .is_err());
    assert!(fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .is_err());
    assert_eq!(counts(&fixture), before);
    assert_v1(&fixture);

    let approved = owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    assert_eq!(approved.request.status, RequestStatus::Approved);
    assert!(approved.standing_grant_id.is_none());
    let before = counts(&fixture);
    assert_eq!(
        owner_decide(
            &fixture,
            &request.id,
            observed(&request),
            ToolRequestDecision::ApproveOnce
        )
        .unwrap(),
        approved
    );
    assert!(owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::Deny
    )
    .is_err());
    assert!(fixture
        .agent
        .approved_tool_turn(&request.id, &"55".repeat(32))
        .is_err());
    let admission = fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .unwrap();
    assert_eq!(admission.proposal().source, fixture.source);
    assert_eq!(
        admission.proposal().arguments_json,
        proposal(&fixture, &request.id).arguments_json
    );
    assert_eq!(admission.turn().document().version, 1);
    assert_eq!(counts(&fixture), before);
    assert_v1(&fixture);

    Ok(())
}

pub(super) fn mutation_of_frozen_arguments_or_source_cannot_replace_the_reviewed_intent(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    signed_trust(&fixture, TrustLevel::Suggest)?;
    let original_turn = turn(&fixture);
    let request = stage(&fixture, "deckcraft_slide_notes");
    let before = counts(&fixture);
    let mut changed = input(&fixture, &original_turn, "deckcraft_slide_notes");
    changed.arguments["text"] = json!("Unreviewed replacement");
    let mut ask: Value = serde_json::from_str(&changed.continuation_json).unwrap();
    ask["call"]["arguments"] = changed.arguments.clone();
    changed.continuation_json = serde_json::to_string(&ask).unwrap();
    assert!(fixture
        .agent
        .stage_tool_approval(&original_turn, changed)
        .is_err());
    assert_eq!(counts(&fixture), before);
    fixture
        .agent
        .lock()
        .append(
            &fixture.space,
            &fixture.owner,
            EventBody::MessageAnnotated {
                target: fixture.source.clone(),
                note: Annotation::Edit {
                    text: "Changed signed instruction".into(),
                },
            },
        )
        .unwrap();
    let before = counts(&fixture);
    assert!(owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce
    )
    .is_err());
    assert!(fixture
        .agent
        .stage_tool_approval(
            &original_turn,
            input(&fixture, &original_turn, "deckcraft_slide_notes")
        )
        .is_err());
    assert!(fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .is_err());
    assert_eq!(counts(&fixture), before);
    assert_v1(&fixture);
    // Content changes do not prevent the owner from cancelling the exact pending request.
    assert_eq!(
        owner_decide(
            &fixture,
            &request.id,
            observed(&request),
            ToolRequestDecision::Cancel
        )
        .unwrap()
        .request
        .status,
        RequestStatus::Cancelled
    );

    Ok(())
}

pub(super) fn changed_head_and_revoked_grants_or_device_refuse_before_execution(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for failure in [
        "head",
        "app-grant",
        "device",
        "served-writer",
        "trust-block",
    ] {
        let fixture = fixture()?;
        signed_trust(&fixture, TrustLevel::Suggest)?;
        let request = stage(&fixture, "deckcraft_slide_notes");
        owner_decide(
            &fixture,
            &request.id,
            observed(&request),
            ToolRequestDecision::ApproveOnce,
        )
        .unwrap();
        let admitted = fixture
            .agent
            .approved_tool_turn(&request.id, WORKER)
            .unwrap();
        let (mutation, intent) = intent(admitted.proposal());
        match failure {
            "head" => {
                fixture
                    .agent
                    .app_binary_save(
                        fixture.base.item_id.clone(),
                        1,
                        fixture.base.sha256.clone(),
                        STANDARD.encode(CHANGED),
                        "synthetic-independent-edit".into(),
                        "66".repeat(32),
                    )
                    .unwrap();
            }
            "app-grant" => {
                let mut engine = fixture.agent.lock();
                let grant = engine
                    .state
                    .grants
                    .iter()
                    .rev()
                    .find(|grant| {
                        grant.grant.scope == GrantScope::Item(fixture.base.item_id.clone())
                            && matches!(grant.grant.capability, Capability::Trust(TrustLevel::Act))
                    })
                    .unwrap()
                    .grant
                    .clone();
                engine
                    .append(
                        &fixture.space,
                        &grant.grantor,
                        EventBody::GrantRevoked { grant: grant.id },
                    )
                    .unwrap();
            }
            "device" => {
                let device = fixture.agent.account().unwrap().device_id;
                fixture
                    .agent
                    .lock()
                    .store
                    .set_meta("devices.revoked", &json!([device]).to_string())
                    .unwrap();
            }
            "served-writer" => {
                let mut engine = fixture.agent.lock();
                let agent = engine.me_id().unwrap();
                engine
                    .append(
                        &fixture.space,
                        &agent,
                        EventBody::MemberAdded {
                            identity: fixture.owner.clone(),
                            role: Role::Reader,
                        },
                    )
                    .unwrap();
            }
            "trust-block" => signed_trust(&fixture, TrustLevel::Listen)?,
            _ => unreachable!(),
        }
        let before = counts(&fixture);
        assert!(
            fixture
                .agent
                .approved_tool_turn(&request.id, WORKER)
                .is_err(),
            "{failure}"
        );
        assert!(
            fixture
                .agent
                .agent_binary_result(admitted.turn(), &mutation, &intent)
                .is_err(),
            "{failure}"
        );
        assert!(
            fixture
                .agent
                .agent_binary_save(
                    admitted.turn(),
                    &mutation,
                    &intent,
                    &STANDARD.encode(CHANGED)
                )
                .is_err(),
            "{failure}"
        );
        assert_eq!(counts(&fixture), before, "{failure}");
        assert_eq!(
            fixture.agent.lock().state.items[&fixture.base.item_id]
                .current()
                .number,
            if failure == "head" { 2 } else { 1 }
        );
    }
    Ok(())
}

pub(super) fn raw_worker_turn_cannot_bypass_a_later_signed_core_block_at_result_or_save_boundary(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    signed_trust(&fixture, TrustLevel::Suggest)?;
    let raw_turn = turn(&fixture);
    let hash = "aa".repeat(32);
    assert!(fixture
        .agent
        .agent_binary_result(&raw_turn, "synthetic-raw-write", &hash)
        .unwrap()
        .is_none());
    signed_trust(&fixture, TrustLevel::Listen)?;
    let before = counts(&fixture);
    assert!(fixture
        .agent
        .agent_binary_result(&raw_turn, "synthetic-raw-write", &hash)
        .is_err());
    assert!(fixture
        .agent
        .agent_binary_save(
            &raw_turn,
            "synthetic-raw-write",
            &hash,
            &STANDARD.encode(CHANGED)
        )
        .is_err());
    // Reading the same authorized artifact remains possible; the denied write adds nothing.
    fixture.agent.agent_binary_finish_read(&raw_turn).unwrap();
    assert_eq!(counts(&fixture), before);
    assert_v1(&fixture);

    Ok(())
}

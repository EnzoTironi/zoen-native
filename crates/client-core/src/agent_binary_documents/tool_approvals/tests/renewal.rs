//! Real signed history: renewal is an owner intent and a new independent decision.
use super::*;

use crate::agent_binary_documents::tests::renewal_history::{
    confirm_pending, expired_approved as expire, signed,
};

fn expired_approved(f: &Fixture, tool: &str) -> (AgentRequestDto, AgentRequestDto) {
    let live = stage(f, tool);
    (expire(f, live.clone()), live)
}

fn request_again(f: &Fixture, old: &AgentRequestDto) -> String {
    owner_decide(
        f,
        &old.id,
        observed(old),
        ToolRequestDecision::RequestReview,
    )
    .unwrap();
    f.agent.tool_review_pending_intent(&old.id).unwrap()
}

#[test]
fn expired_approval_requests_only_preparation_and_successor_requires_independent_approval() {
    let f = fixture();
    let (old, _) = expired_approved(&f, "read_deckcraft");
    let original = f.agent.lock().state.requests[&old.id].clone();
    assert!(owner_decide(
        &f,
        &old.id,
        &"00".repeat(32),
        ToolRequestDecision::RequestReview
    )
    .is_err());
    assert!(f
        .agent
        .lock()
        .decide_tool_request(&old.id, observed(&old), ToolRequestDecision::RequestReview)
        .is_err());
    let intent = request_again(&f, &old);
    let before = counts(&f);
    request_again(&f, &old);
    assert_eq!(
        counts(&f),
        before,
        "lost owner ACK does not append another intent"
    );
    assert!(f.agent.approved_tool_turn(&old.id, WORKER).is_err());
    let next = f
        .agent
        .stage_tool_review_successor(&old.id, &intent)
        .unwrap()
        .unwrap();
    assert_eq!(next.status, RequestStatus::Pending);
    assert!(f.agent.approved_tool_turn(&next.id, WORKER).is_err());
    let p = proposal(&f, &next.id);
    let prior = original.req.tool_proposal.as_ref().unwrap();
    let mut expected = prior.as_ref().clone();
    expected.expires_at_ms = p.expires_at_ms;
    expected.review = p.review.clone();
    assert_eq!(
        expected, p,
        "all frozen source/args/worker/policy/budgets remain exact"
    );
    assert_eq!(p.review.as_ref().unwrap().root, old.id);
    assert_eq!(p.review.as_ref().unwrap().predecessor, old.id);
    assert_eq!(p.review.as_ref().unwrap().generation, 1);
    assert_eq!(
        f.agent
            .tool_review_journal_root(&next.id, observed(&next))
            .unwrap(),
        (old.id.clone(), observed(&old).into())
    );
    let before = counts(&f);
    assert_eq!(
        f.agent
            .stage_tool_review_successor(&old.id, &intent)
            .unwrap(),
        Some(next.clone())
    );
    assert_eq!(
        counts(&f),
        before,
        "lost staging ACK returns the same pending request"
    );
    assert_eq!(f.agent.lock().state.requests[&old.id].req, original.req);
    assert_eq!(
        f.agent.lock().state.requests[&old.id].resolution,
        original.resolution
    );
    confirm_pending(&f);
    owner_decide(
        &f,
        &next.id,
        observed(&next),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    assert!(
        f.agent.approved_tool_turn(&next.id, WORKER).is_err(),
        "optimistic approval is not renewed execution authority"
    );
    confirm_pending(&f);
    assert!(f.agent.approved_tool_turn(&next.id, WORKER).is_ok());
    assert!(f.agent.approved_tool_turn(&old.id, WORKER).is_err());
    assert_v1(&f);
}

#[test]
fn first_valid_signed_successor_wins_and_forged_or_legacy_roots_cannot_reset_lineage() {
    let f = fixture();
    let (old, _) = expired_approved(&f, "read_deckcraft");
    let intent = request_again(&f, &old);
    // Build candidates from the exact frozen predecessor before either is sequenced.
    let original = f.agent.lock().state.requests[&old.id].req.clone();
    let mut first = original.clone();
    first.id = types::new_id("first_review");
    first.tool_proposal.as_mut().unwrap().expires_at_ms = crate::engine::now_ms() + 60_000;
    first.tool_proposal.as_mut().unwrap().review = Some(types::ToolReviewLineage {
        root: old.id.clone(),
        predecessor: old.id.clone(),
        intent,
        generation: 1,
    });
    first.content_hash = first.tool_proposal_hash(&f.space).unwrap();
    for mutation in 0..4 {
        let mut forged = first.clone();
        forged.id = types::new_id("forged_review");
        let p = forged.tool_proposal.as_mut().unwrap();
        match mutation {
            0 => p.review.as_mut().unwrap().root = "other-root".into(),
            1 => p.review.as_mut().unwrap().intent = "other-intent".into(),
            2 => p.review.as_mut().unwrap().generation = 2,
            _ => p.continuation_json.push(' '),
        }
        forged.content_hash = forged.tool_proposal_hash(&f.space).unwrap();
        let id = forged.id.clone();
        append_at(&f, forged, crate::engine::now_ms());
        assert!(!f.agent.lock().state.requests.contains_key(&id));
    }
    let mut competing = first.clone();
    competing.id = types::new_id("later_review");
    competing.content_hash = competing.tool_proposal_hash(&f.space).unwrap();
    append_at(&f, first.clone(), crate::engine::now_ms());
    append_at(&f, competing.clone(), crate::engine::now_ms());
    assert!(!f.agent.lock().state.requests.contains_key(&competing.id));
    let mut bypass = original;
    bypass.id = types::new_id("legacy_bypass");
    bypass.tool_proposal.as_mut().unwrap().expires_at_ms = crate::engine::now_ms() + 60_000;
    bypass.content_hash = bypass.tool_proposal_hash(&f.space).unwrap();
    let bypass_id = bypass.id.clone();
    append_at(&f, bypass, crate::engine::now_ms());
    assert!(!f.agent.lock().state.requests.contains_key(&bypass_id));
    f.agent.lock().reload().unwrap();
    assert_eq!(
        f.agent
            .tool_approval_requests(&f.space, &f.source)
            .last()
            .unwrap()
            .id,
        first.id
    );
}

#[test]
fn receipt_arriving_after_owner_intent_reconciles_without_staging_or_repeating_the_effect() {
    let f = fixture();
    signed_trust(&f, TrustLevel::Suggest);
    let (old, live) = expired_approved(&f, "deckcraft_slide_notes");
    owner_decide(
        &f,
        &live.id,
        observed(&live),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    let live_permit = f.agent.approved_tool_turn(&live.id, WORKER).unwrap();
    let renewal = request_again(&f, &old);
    let (mutation, digest) = intent(live_permit.proposal());
    let accepted = f
        .agent
        .agent_binary_save(
            live_permit.turn(),
            &mutation,
            &digest,
            &STANDARD.encode(CHANGED),
        )
        .unwrap();
    assert_eq!(accepted.version, 2);
    let before = counts(&f);
    assert_eq!(
        f.agent
            .stage_tool_review_successor(&old.id, &renewal)
            .unwrap(),
        None
    );
    assert_eq!(counts(&f), before);
    assert!(f.agent.tool_review_pending_intent(&old.id).is_some());
    assert_eq!(current(&f), accepted);
}

#[test]
fn renewal_refuses_changed_head_and_signed_block_without_creating_an_intent() {
    for block in [false, true] {
        let f = fixture();
        signed_trust(&f, TrustLevel::Suggest);
        let (old, live) = expired_approved(&f, "deckcraft_slide_notes");
        if block {
            signed(
                &f,
                &f.owner,
                EventBody::GrantIssued {
                    grant: Grant {
                        id: types::new_id("synthetic_block"),
                        grantor: f.owner.clone(),
                        grantee: Some(f.agent.account().unwrap().identity_id),
                        scope: GrantScope::Space(f.space.clone()),
                        capability: Capability::Trust(TrustLevel::Listen),
                        expires_at_ms: None,
                    },
                },
                crate::engine::now_ms(),
            );
        } else {
            owner_decide(
                &f,
                &live.id,
                observed(&live),
                ToolRequestDecision::ApproveOnce,
            )
            .unwrap();
            let permit = f.agent.approved_tool_turn(&live.id, WORKER).unwrap();
            let (id, hash) = intent(permit.proposal());
            f.agent
                .agent_binary_save(permit.turn(), &id, &hash, &STANDARD.encode(CHANGED))
                .unwrap();
        }
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
    }
}

#[test]
fn signed_non_owner_or_wrong_hash_intents_are_inert_and_later_revocation_prevents_staging() {
    for failure in ["device", "app-grant", "served-writer", "block"] {
        let f = fixture();
        signed_trust(&f, TrustLevel::Suggest);
        let (old, _) = expired_approved(&f, "deckcraft_slide_notes");
        let agent = f.agent.account().unwrap().identity_id;
        for (author, hash) in [
            (&agent, observed(&old).to_string()),
            (&f.owner, "00".repeat(32)),
        ] {
            signed(
                &f,
                author,
                EventBody::ToolReviewRequested {
                    request: old.id.clone(),
                    content_hash: hash,
                },
                crate::engine::now_ms(),
            );
            assert!(f.agent.tool_review_pending_intent(&old.id).is_none());
        }
        let intent = request_again(&f, &old);
        match failure {
            "device" => {
                let device = f.agent.account().unwrap().device_id;
                f.agent
                    .lock()
                    .store
                    .set_meta("devices.revoked", &json!([device]).to_string())
                    .unwrap();
            }
            "app-grant" => {
                let grant = f
                    .agent
                    .lock()
                    .state
                    .grants
                    .iter()
                    .rev()
                    .find(|g| {
                        g.grant.scope == GrantScope::Item(f.base.item_id.clone())
                            && matches!(g.grant.capability, Capability::Trust(TrustLevel::Act))
                    })
                    .unwrap()
                    .grant
                    .clone();
                signed(
                    &f,
                    &grant.grantor,
                    EventBody::GrantRevoked {
                        grant: grant.id.clone(),
                    },
                    crate::engine::now_ms(),
                );
            }
            "served-writer" => signed(
                &f,
                &agent,
                EventBody::MemberAdded {
                    identity: f.owner.clone(),
                    role: Role::Reader,
                },
                crate::engine::now_ms(),
            ),
            "block" => signed(
                &f,
                &f.owner,
                EventBody::GrantIssued {
                    grant: Grant {
                        id: types::new_id("synthetic_block"),
                        grantor: f.owner.clone(),
                        grantee: Some(agent.clone()),
                        scope: GrantScope::Space(f.space.clone()),
                        capability: Capability::Trust(TrustLevel::Listen),
                        expires_at_ms: None,
                    },
                },
                crate::engine::now_ms(),
            ),
            _ => unreachable!(),
        }
        let before = counts(&f);
        let unchanged_content = serde_json::to_value(
            &f.agent.lock().state.items[&f.base.item_id]
                .current()
                .content,
        )
        .unwrap();
        assert!(
            f.agent
                .stage_tool_review_successor(&old.id, &intent)
                .is_err(),
            "{failure}"
        );
        assert_eq!(counts(&f), before, "{failure}");
        // The refused operation must leave all storage/effects untouched even
        // when current access intentionally forbids a public document read.
        assert_eq!(
            serde_json::to_value(
                &f.agent.lock().state.items[&f.base.item_id]
                    .current()
                    .content
            )
            .unwrap(),
            unchanged_content
        );
        assert_eq!(
            f.agent.lock().state.items[&f.base.item_id].current().number,
            1
        );
        if failure == "device" {
            assert!(
                f.agent
                    .app_binary_read(f.base.item_id.clone(), None, None)
                    .is_err(),
                "revoked reads remain forbidden"
            );
        }
    }
}

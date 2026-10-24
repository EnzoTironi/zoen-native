//! Revoking the newest canonical baseline invalidates pending and held approvals.
use super::*;

fn revoke_latest(fixture: &Fixture) {
    let mut e = fixture.agent.lock();
    let grant = e
        .state
        .grants
        .iter()
        .rev()
        .find(|g| {
            g.grant.grantor == fixture.owner
                && g.grant.grantee.as_deref() == Some(e.me_id().unwrap().as_str())
                && g.grant.scope == GrantScope::Space(fixture.space.clone())
                && matches!(g.grant.capability, Capability::Trust(_))
        })
        .unwrap()
        .grant
        .id
        .clone();
    e.append(
        &fixture.space,
        &fixture.owner,
        EventBody::GrantRevoked { grant },
    )
    .unwrap();
}
#[test]
fn revoked_newest_trust_cannot_be_overridden_by_pending_approve_once() {
    let fixture = fixture();
    signed_trust(&fixture, TrustLevel::Suggest);
    let request = stage(&fixture, "deckcraft_slide_notes");
    revoke_latest(&fixture);
    let before = counts(&fixture);
    assert!(owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce
    )
    .is_err());
    assert_eq!(counts(&fixture), before);
    assert!(fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .is_err());
    owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::Deny,
    )
    .unwrap();
    assert_v1(&fixture);
}
#[test]
fn held_approved_tool_cannot_execute_after_the_newest_trust_is_revoked() {
    let fixture = fixture();
    signed_trust(&fixture, TrustLevel::Suggest);
    let request = stage(&fixture, "deckcraft_slide_notes");
    owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    let approved = fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .unwrap();
    let (mutation, intent) = intent(approved.proposal());
    revoke_latest(&fixture);
    let before = counts(&fixture);
    assert!(fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .is_err());
    assert!(fixture
        .agent
        .agent_binary_save(
            approved.turn(),
            &mutation,
            &intent,
            &STANDARD.encode(CHANGED)
        )
        .is_err());
    assert_eq!(counts(&fixture), before);
    assert_v1(&fixture);
}

#[test]
fn revoked_newest_item_grant_cannot_revive_an_old_app_act_for_a_held_tool() {
    let fixture = fixture();
    signed_trust(&fixture, TrustLevel::Suggest);
    let request = stage(&fixture, "deckcraft_slide_notes");
    owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    let approved = fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .unwrap();
    let (mutation, intent) = intent(approved.proposal());
    {
        let mut e = fixture.agent.lock();
        let id = types::new_id("lower_app_trust");
        e.append(
            &fixture.space,
            &fixture.owner,
            EventBody::GrantIssued {
                grant: Grant {
                    id: id.clone(),
                    grantor: fixture.owner.clone(),
                    grantee: Some(format!("app:{}", fixture.base.item_id)),
                    scope: GrantScope::Item(fixture.base.item_id.clone()),
                    capability: Capability::Trust(TrustLevel::Listen),
                    expires_at_ms: None,
                },
            },
        )
        .unwrap();
        e.append(
            &fixture.space,
            &fixture.owner,
            EventBody::GrantRevoked { grant: id },
        )
        .unwrap();
    }
    let before = counts(&fixture);
    assert!(fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .is_err());
    assert!(fixture
        .agent
        .agent_binary_save(
            approved.turn(),
            &mutation,
            &intent,
            &STANDARD.encode(CHANGED)
        )
        .is_err());
    assert_eq!(counts(&fixture), before);
    assert_v1(&fixture);
}

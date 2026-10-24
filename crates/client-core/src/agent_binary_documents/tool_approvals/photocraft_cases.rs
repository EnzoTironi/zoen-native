use super::*;
use crate::agent_binary_documents::tests::fixture_binary;
fn photo_fixture() -> Fixture {
    fixture_binary(
        types::Privacy::Closed,
        "photocraft",
        include_bytes!("../../../../miniapps/src/photocraft/fixtures/native-asymmetric.pcraft"),
    )
}
const PHOTO_EDITED: &[u8] =
    include_bytes!("../../../../miniapps/src/photocraft/fixtures/native-rotated.pcraft");
#[test]
fn reviewed_photo_read_cannot_replace_its_stable_source_step_with_a_write() {
    let f = photo_fixture();
    signed_trust(&f, TrustLevel::Act);
    let read = stage(&f, "read_photocraft");
    owner_decide(
        &f,
        &read.id,
        observed(&read),
        ToolRequestDecision::ApproveOnce,
    )
    .expect("owner read approval");
    let permit = f
        .agent
        .approved_tool_turn(&read.id, WORKER)
        .expect("native read permit");
    assert_eq!(permit.turn().document().kind, "photocraft");
    f.agent
        .agent_binary_finish_read(permit.turn())
        .expect("finish approved read");
    let turn = turn(&f);
    let before = counts(&f);
    assert!(f
        .agent
        .stage_tool_approval(&turn, input(&f, &turn, "photocraft_transform"))
        .is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(current(&f).version, 1);
}
#[test]
fn certified_photo_turn_uses_current_native_source_and_exact_approved_binary_cas() {
    let f = photo_fixture();
    signed_trust(&f, TrustLevel::Act);
    let write = stage(&f, "photocraft_transform");
    assert!(f.agent.approved_tool_turn(&write.id, WORKER).is_err());
    owner_decide(
        &f,
        &write.id,
        observed(&write),
        ToolRequestDecision::ApproveOnce,
    )
    .expect("owner exact edit approval");
    assert!(f
        .agent
        .approved_tool_turn(&write.id, &"5".repeat(64))
        .is_err());
    let permit = f
        .agent
        .approved_tool_turn(&write.id, WORKER)
        .expect("native write permit");
    let (_, action) = parsed(permit.proposal()).expect("validated frozen command");
    let intent = command_intent(permit.proposal(), action.as_ref()).expect("intent");
    let mutation = format!("photocmd_{intent}");
    let accepted = f
        .agent
        .agent_binary_save(
            permit.turn(),
            &mutation,
            &intent,
            &STANDARD.encode(PHOTO_EDITED),
        )
        .expect("accepted original engine bytes");
    assert_eq!(accepted.kind, "photocraft");
    assert_eq!(accepted.actor_kind, "agent");
    assert_eq!(accepted.version, 2);
    assert_eq!(accepted.data_base64, STANDARD.encode(PHOTO_EDITED));
    let repeated = f
        .agent
        .agent_binary_result(permit.turn(), &mutation, &intent)
        .expect("exact lookup")
        .expect("retained result");
    assert_eq!(repeated.sha256, accepted.sha256);
    assert_eq!(repeated.version, 2);
}
#[test]
fn binary_approval_refuses_cross_format_tools_without_creating_requests() {
    for (f, tool) in [
        (photo_fixture(), "deckcraft_slide_notes"),
        (fixture(), "photocraft_transform"),
    ] {
        signed_trust(&f, TrustLevel::Act);
        let t = turn(&f);
        let before = counts(&f);
        assert!(f
            .agent
            .stage_tool_approval(&t, input(&f, &t, tool))
            .is_err());
        assert_eq!(counts(&f), before);
    }
}
#[test]
fn photo_approved_turn_cannot_outlive_current_app_grant() {
    let f = photo_fixture();
    signed_trust(&f, TrustLevel::Act);
    let request = stage(&f, "photocraft_transform");
    owner_decide(
        &f,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce,
    )
    .expect("approve");
    let permit = f
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .expect("permit");
    let (_, action) = parsed(permit.proposal()).expect("parsed");
    let intent = command_intent(permit.proposal(), action.as_ref()).expect("intent");
    {
        let mut engine = f.agent.lock();
        let grant = engine
            .state
            .grants
            .iter()
            .rev()
            .find(|grant| {
                grant.grant.scope == GrantScope::Item(f.base.item_id.clone())
                    && matches!(grant.grant.capability, Capability::Trust(TrustLevel::Act))
            })
            .expect("original item Act grant")
            .grant
            .clone();
        engine
            .append(
                &f.space,
                &grant.grantor,
                EventBody::GrantRevoked { grant: grant.id },
            )
            .expect("revoke original app grant");
    }
    let before = counts(&f);
    assert!(f
        .agent
        .agent_binary_save(
            permit.turn(),
            &format!("photocmd_{intent}"),
            &intent,
            &STANDARD.encode(PHOTO_EDITED)
        )
        .is_err());
    assert!(f.agent.approved_tool_turn(&request.id, WORKER).is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(current(&f).version, 1);
}

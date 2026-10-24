use super::tests::{decide, input, setup};
use super::*;
use crate::{AgentToolApprovalInput, ToolRequestDecision};
use base64::{engine::general_purpose::STANDARD, Engine as _};
fn grid_input(f: &crate::agent_binary_documents::tests::Fixture) -> AgentToolApprovalInput {
    let mut input = input(f, "Reviewed spreadsheet");
    input.canonical_tool = GRID_TOOL.into();
    input.worker_binding = executor_binding_for(GRID_TOOL).expect("fixed grid binding");
    let mut ask: Value =
        serde_json::from_str(&input.continuation_json).expect("fixture continuation");
    ask["call"]["name"] = json!(GRID_TOOL);
    ask["definition"] =
        definition_unchecked_for(GRID_TOOL, &f.space).expect("fixed grid definition");
    input.continuation_json = ask.to_string();
    input
}
#[test]
fn grid_creation_is_pending_until_exact_owner_review_and_replays_the_original_real_seed() {
    let f = setup();
    let before = f.agent.items().len();
    let dto = f
        .agent
        .stage_app_creation_approval(&f.space, &f.source, &f.owner, grid_input(&f))
        .expect("stage grid");
    assert_eq!(f.agent.items().len(), before);
    assert!(f.agent.approved_app_creation(&dto.id).is_err());
    decide(
        &f,
        &dto.id,
        &dto.tool_approval.as_ref().expect("review").content_hash,
        ToolRequestDecision::ApproveOnce,
    )
    .expect("owner exact approval");
    let permit = f
        .agent
        .approved_app_creation(&dto.id)
        .expect("native permit");
    assert_eq!(
        permit.proposal().engine_revision,
        Format::Gridcraft.revision()
    );
    assert!(permit.proposal().artifact.starts_with("it_grid_"));
    let first: Value =
        serde_json::from_str(&f.agent.execute_app_creation(&permit).expect("create grid"))
            .expect("receipt");
    let again: Value = serde_json::from_str(
        &f.agent
            .execute_app_creation(&permit)
            .expect("original receipt"),
    )
    .expect("receipt");
    assert_eq!(first["itemId"], again["itemId"]);
    assert_eq!(first["sha256"], again["sha256"]);
    assert_eq!(again["replayed"], true);
    assert_eq!(f.agent.items().len(), before + 1);
    let document = f
        .agent
        .app_binary_read(first["itemId"].as_str().expect("item").into(), None, None)
        .expect("real binary read");
    assert_eq!(document.kind, "gridcraft");
    assert_eq!(document.actor_kind, "agent");
    assert_eq!(
        document.data_base64,
        STANDARD.encode(Format::Gridcraft.seed())
    );
    assert_ne!(executor_binding_for(GRID_TOOL), executor_binding_for(TOOL));
}
#[test]
fn grid_creation_cannot_borrow_a_deck_seed_binding_or_change_frozen_tool_arguments() {
    let f = setup();
    let before = f.agent.items().len();
    let mut wrong = grid_input(&f);
    wrong.worker_binding = executor_binding();
    assert!(f
        .agent
        .stage_app_creation_approval(&f.space, &f.source, &f.owner, wrong)
        .is_err());
    let mut wrong = grid_input(&f);
    let mut ask: Value = serde_json::from_str(&wrong.continuation_json).expect("ask");
    ask["definition"] = definition_unchecked(&f.space).expect("deck schema");
    wrong.continuation_json = ask.to_string();
    assert!(f
        .agent
        .stage_app_creation_approval(&f.space, &f.source, &f.owner, wrong)
        .is_err());
    assert_eq!(f.agent.items().len(), before);
}

//! PDF creation reuses the existing exact signed owner review and native seed.
#![warn(clippy::pedantic, clippy::perf)]
use super::tests::{decide, input, setup};
use super::*;
use crate::{AgentToolApprovalInput, ToolRequestDecision};
use base64::{engine::general_purpose::STANDARD, Engine as _};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn pdf_input(
    f: &crate::agent_binary_documents::tests::Fixture,
) -> TestResult<AgentToolApprovalInput> {
    let mut value = input(f, "Reviewed PDF");
    value.canonical_tool = PDF_TOOL.into();
    value.worker_binding = executor_binding_for(PDF_TOOL).ok_or("missing PDF binding")?;
    let mut ask: Value = serde_json::from_str(&value.continuation_json)?;
    ask["call"]["name"] = json!(PDF_TOOL);
    ask["definition"] = definition_unchecked_for(PDF_TOOL, &f.space).ok_or("missing PDF schema")?;
    value.continuation_json = ask.to_string();
    Ok(value)
}

#[test]
fn pdf_creation_requires_exact_owner_review_and_replays_one_canonical_seed() -> TestResult {
    let f = setup();
    let before = f.agent.items().len();
    let request =
        f.agent
            .stage_app_creation_approval(&f.space, &f.source, &f.owner, pdf_input(&f)?)?;
    assert_eq!(f.agent.items().len(), before);
    assert!(f.agent.approved_app_creation(&request.id).is_err());
    decide(
        &f,
        &request.id,
        &request
            .tool_approval
            .as_ref()
            .ok_or("missing review")?
            .content_hash,
        ToolRequestDecision::ApproveOnce,
    )?;
    let permit = f.agent.approved_app_creation(&request.id)?;
    assert_eq!(
        permit.proposal().engine_revision,
        Format::Pdfcraft.revision()
    );
    assert!(permit.proposal().artifact.starts_with("it_pdf_"));
    let first: Value = serde_json::from_str(&f.agent.execute_app_creation(&permit)?)?;
    let replay: Value = serde_json::from_str(&f.agent.execute_app_creation(&permit)?)?;
    assert_eq!(first["itemId"], replay["itemId"]);
    assert_eq!(first["sha256"], replay["sha256"]);
    assert_eq!(replay["replayed"], true);
    assert_eq!(f.agent.items().len(), before + 1);
    let doc = f.agent.app_binary_read(
        first["itemId"].as_str().ok_or("missing item")?.into(),
        None,
        None,
    )?;
    assert_eq!(doc.kind, "pdfcraft");
    assert_eq!(doc.actor_kind, "agent");
    assert_eq!(doc.data_base64, STANDARD.encode(Format::Pdfcraft.seed()));
    assert_ne!(executor_binding_for(PDF_TOOL), executor_binding_for(TOOL));
    Ok(())
}

#[test]
fn pdf_creation_refuses_another_format_binding_or_frozen_schema() -> TestResult {
    let f = setup();
    let before = f.agent.items().len();
    let mut wrong = pdf_input(&f)?;
    wrong.worker_binding = executor_binding();
    assert!(f
        .agent
        .stage_app_creation_approval(&f.space, &f.source, &f.owner, wrong)
        .is_err());
    let mut wrong = pdf_input(&f)?;
    let mut ask: Value = serde_json::from_str(&wrong.continuation_json)?;
    ask["definition"] = definition_unchecked(&f.space).ok_or("missing Deck schema")?;
    wrong.continuation_json = ask.to_string();
    assert!(f
        .agent
        .stage_app_creation_approval(&f.space, &f.source, &f.owner, wrong)
        .is_err());
    assert_eq!(f.agent.items().len(), before);
    Ok(())
}

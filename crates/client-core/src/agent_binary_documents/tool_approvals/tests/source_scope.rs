//! Scope stays independent of each canonical ApproveOnce decision.
use super::*;
use crate::agent_binary_documents::source_scope_tests::scope;

#[test]
fn scoped_approved_steps_keep_original_source_and_require_a_new_exact_decision(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = fixture()?;
    scope(&mut f, |_| {})?;
    signed_trust(&f, TrustLevel::Suggest)?;
    let mut previous_call = None;
    for step in 0..2 {
        let t = turn(&f);
        let mut proposed = input(&f, &t, "deckcraft_slide_notes");
        let mut ask: Value = serde_json::from_str(&proposed.continuation_json).unwrap();
        if let Some(call) = previous_call.take() {
            ask["steps"] = json!([{"call":call,"result":"accepted","is_error":false}]);
            ask["round"] = json!(1);
        }
        ask["call"]["id"] = json!(format!("scoped-step-{step}"));
        previous_call = Some(ask["call"].clone());
        proposed.continuation_json = ask.to_string();
        let request = f.agent.stage_tool_approval(&t, proposed).unwrap();
        assert!(f.agent.approved_tool_turn(&request.id, WORKER).is_err());
        owner_decide(
            &f,
            &request.id,
            observed(&request),
            ToolRequestDecision::ApproveOnce,
        )
        .unwrap();
        let approved = f.agent.approved_tool_turn(&request.id, WORKER).unwrap();
        let (mutation, intent) = intent(approved.proposal());
        let bytes = if step == 0 { CHANGED } else { ORIGINAL };
        f.base = f
            .agent
            .agent_binary_save(approved.turn(), &mutation, &intent, &STANDARD.encode(bytes))
            .unwrap();
        assert_eq!(f.base.version, step + 2);
        assert_eq!(
            f.agent
                .agent_binary_result(approved.turn(), &mutation, &intent)
                .unwrap()
                .unwrap()
                .version,
            step + 2
        );
    }
    assert_eq!(f.agent.tool_approval_requests(&f.space, &f.source).len(), 2);
    assert!(f
        .agent
        .agent_binary_turn(&f.base.item_id, &f.source, &f.owner, None, None)
        .is_ok());

    Ok(())
}

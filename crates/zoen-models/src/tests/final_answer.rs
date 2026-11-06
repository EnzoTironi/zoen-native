//! A final turn retains tool provenance and cannot propose more paid work.
use super::*;

fn final_request() -> ModelRequest {
    let mut input = request();
    input.tool_mode = ToolMode::FinalAnswer;
    input.messages.push(InputMessage::Assistant {
        blocks: vec![OutputBlock::ToolCall {
            call_id: "history-call".into(),
            name: "lookup".into(),
            arguments: json!({"q":"scope"}),
        }],
    });
    input.messages.push(InputMessage::ToolResult {
        call_id: "history-call".into(),
        name: "lookup".into(),
        text: "verified original data".into(),
    });
    input
}
#[tokio::test]
async fn final_answer_preserves_tool_role_and_disables_more_calls(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::json(reply()).await;
    let output = ModelGateway::new(config(&fixture.base))?
        .complete(final_request(), &TestAuthority::default())
        .await?;
    assert!(output.output.is_ok());
    let requests = fixture.seen.lock().map_err(|_| "requests")?;
    assert_eq!(requests[0].1["tool_choice"], "none");
    assert_eq!(requests[0].1["messages"][3]["role"], "tool");
    assert_eq!(requests[0].1["messages"][3]["tool_call_id"], "history-call");
    assert_eq!(
        requests[0].1["messages"][3]["content"],
        "verified original data"
    );
    Ok(())
}
#[tokio::test]
async fn final_answer_rejects_provider_tool_proposals_without_losing_usage(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut body = reply();
    body["choices"][0]["message"] = json!({"role":"assistant", "content":null, "tool_calls":[{"id":"another-call", "type":"function", "function":{"name":"lookup","arguments":"{\"q\":\"scope\"}"}}]});
    body["choices"][0]["finish_reason"] = "tool_calls".into();
    let fixture = Fixture::json(body).await;
    let output = ModelGateway::new(config(&fixture.base))?
        .complete(final_request(), &TestAuthority::default())
        .await?;
    assert_eq!(output.output, Err(OutputFailure::InvalidToolCall));
    assert!(matches!(output.receipt.usage, UsageEvidence::Reported(_)));
    assert!(output.evidence.complete);
    Ok(())
}
#[test]
fn legacy_auto_requests_keep_their_serialized_shape() -> Result<(), Box<dyn std::error::Error>> {
    let original = serde_json::to_value(request())?;
    assert!(original.get("tool_mode").is_none());
    let restored: ModelRequest = serde_json::from_value(original.clone())?;
    assert_eq!(serde_json::to_value(restored)?, original);
    assert_eq!(
        serde_json::to_value(final_request())?["tool_mode"],
        "final_answer"
    );
    Ok(())
}

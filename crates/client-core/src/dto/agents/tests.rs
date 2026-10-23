use super::*;

#[test]
fn legacy_tool_decision_order_json_and_absent_review_dto_remain_explicit() {
    let legacy = [
        ToolRequestDecision::ApproveOnce,
        ToolRequestDecision::Deny,
        ToolRequestDecision::Cancel,
    ];
    for (value, bytes) in legacy
        .into_iter()
        .zip(["\"ApproveOnce\"", "\"Deny\"", "\"Cancel\""])
    {
        assert_eq!(serde_json::to_string(&value).unwrap(), bytes);
        assert_eq!(
            serde_json::from_str::<ToolRequestDecision>(bytes).unwrap(),
            value
        );
    }
    assert_eq!(
        serde_json::to_string(&ToolRequestDecision::RequestReview).unwrap(),
        "\"RequestReview\""
    );
    let old = r#"{"content_hash":"hash","artifact_id":"item","artifact_title":"Title","version":1,"operation":"Read","expires_ms":100}"#;
    let dto: ToolApprovalDto = serde_json::from_str(old).unwrap();
    assert!(dto.review.is_none());
    assert_eq!(serde_json::to_string(&dto).unwrap(), old);
    // Actual UniFFI generation must qualify the appended fourth enum variant
    // and the optional record field. JSON compatibility is not ABI equivalence.
}

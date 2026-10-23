use super::*;

#[test]
fn review_uses_the_same_strict_calls_while_say_remains_only_text() {
    let proposal = |kind: &str| {
        json!({"outcome":{"kind":kind,"calls":[{"id":"r1","name":"edit_deck","arguments_json":"{\"shapeId\":42,\"text\":\"Reviewed text\"}"}]}}).to_string()
    };
    for (kind, required) in [("call", false), ("review", true)] {
        let BrainOut::CallTools(calls) = parse_output(&proposal(kind), &[tool()]).unwrap() else {
            panic!("structured proposal")
        };
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].requires_review, required);
        assert_eq!(
            calls[0].arguments,
            json!({"shapeId":42,"text":"Reviewed text"})
        );
    }
    let say = json!({"outcome":{"kind":"say","text":"May I edit this?"}}).to_string();
    assert_eq!(
        parse_output(&say, &[tool()]).unwrap(),
        BrainOut::Say("May I edit this?".into())
    );
    for mut bad in [
        json!({"outcome":{"kind":"review","text":"I also said something","calls":[{"id":"r1","name":"edit_deck","arguments_json":"{}"}]}}),
        json!({"outcome":{"kind":"review","calls":[]}}),
        json!({"outcome":{"kind":"review","calls":[{"id":"r1","name":"unoffered","arguments_json":"{}"}]}}),
    ] {
        assert!(parse_output(&bad.to_string(), &[tool()]).is_err());
        bad["outcome"]["calls"] =
            json!([{"id":"r1","name":"edit_deck","arguments_json":"{}","requires_review":false}]);
        assert!(
            parse_output(&bad.to_string(), &[tool()]).is_err(),
            "model cannot add fields or suppress review"
        );
    }
    assert!(output_schema(&[tool()])["properties"]["outcome"]["anyOf"]
        .as_array()
        .unwrap()
        .iter()
        .any(|variant| variant["properties"]["kind"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("review"))));
}

#[tokio::test]
async fn actual_fake_codex_admission_transmits_guidance_and_returns_review_proposal() {
    let (_dir, brain, marker) = fixture("review");
    let BrainOut::CallTools(calls) = think(&brain).await.unwrap() else {
        panic!("review")
    };
    assert_eq!(calls.len(), 1);
    assert!(calls[0].requires_review);
    assert_eq!(calls[0].name, "edit_deck");
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "turn\n");
}

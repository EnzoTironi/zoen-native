use super::*;

#[tokio::test]
async fn actual_http_preserves_reported_zero_ids_and_pinned_request() {
    let fixture = Fixture::json(reply()).await;
    let gateway = ModelGateway::new(config(&fixture.base)).unwrap();
    let result = gateway
        .complete(request(), &TestAuthority::default())
        .await
        .unwrap();
    assert_eq!(
        result.output,
        Ok(vec![OutputBlock::Text { text: BODY.into() }])
    );
    assert_eq!(result.receipt.response_id.as_deref(), Some("response-23"));
    assert_eq!(result.receipt.request_id.as_deref(), Some("transport-17"));
    assert_eq!(
        result.receipt.usage,
        UsageEvidence::Reported(ReportedUsage {
            input_tokens: Some(10),
            output_tokens: Some(0),
            total_tokens: Some(10),
            ..Default::default()
        })
    );
    let seen = fixture.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].0.starts_with("POST /v1/chat/completions HTTP/1.1"));
    assert!(seen[0].0.contains(KEY));
    assert_eq!(seen[0].1["model"], "fixture-model");
    assert_eq!(
        seen[0].1["messages"][0]["content"][0]["text"],
        "pinned instructions"
    );
    assert_eq!(seen[0].1["messages"][1]["content"], PROMPT);
    assert_eq!(seen[0].1["max_tokens"], 20);
    assert!(result.evidence.complete);
}

#[tokio::test]
async fn absent_usage_is_not_zero_and_absent_output_is_not_inferred() {
    for usage in [
        Value::Null,
        json!({"prompt_tokens":10,"total_tokens":13}),
        json!({"prompt_tokens_details":{}}),
    ] {
        let mut body = reply();
        body["usage"] = usage.clone();
        let fixture = Fixture::json(body).await;
        let result = ModelGateway::new(config(&fixture.base))
            .unwrap()
            .complete(request(), &TestAuthority::default())
            .await
            .unwrap();
        if usage.is_null() {
            assert_eq!(result.receipt.usage, UsageEvidence::Missing);
        } else {
            let UsageEvidence::Reported(counters) = result.receipt.usage else {
                panic!("direct counters")
            };
            assert_eq!(counters.output_tokens, None);
            assert_eq!(counters.cache_read_tokens, None);
        }
    }
}

#[tokio::test]
async fn invalid_counters_and_overflow_do_not_become_billable_zero() {
    for usage in [
        json!({"prompt_tokens":-1}),
        json!({"completion_tokens":"2"}),
        json!({"prompt_tokens":2,"completion_tokens":1,"total_tokens":7}),
        json!({"prompt_tokens":u64::MAX,"completion_tokens":1,"total_tokens":0}),
        json!({"prompt_tokens":1,"prompt_tokens_details":{"cached_tokens":2}}),
        json!({"prompt_tokens":10,"total_tokens":3}),
        json!({"completion_tokens":10,"total_tokens":3}),
        json!({"prompt_tokens":u64::MAX,"completion_tokens":1}),
        json!({"total_tokens":3,"prompt_tokens_details":{"cached_tokens":10}}),
        json!({"total_tokens":3,"completion_tokens_details":{"reasoning_tokens":10}}),
    ] {
        let mut body = reply();
        body["usage"] = usage;
        let fixture = Fixture::json(body).await;
        let result = ModelGateway::new(config(&fixture.base))
            .unwrap()
            .complete(request(), &TestAuthority::default())
            .await
            .unwrap();
        assert_eq!(result.receipt.usage, UsageEvidence::Invalid);
        assert!(result.output.is_ok());
    }
}

#[tokio::test]
async fn valid_partial_counter_totals_never_fill_the_missing_counter() {
    let mut body = reply();
    body["usage"] = json!({"prompt_tokens":10,"total_tokens":13});
    let fixture = Fixture::json(body).await;
    let result = ModelGateway::new(config(&fixture.base))
        .unwrap()
        .complete(request(), &TestAuthority::default())
        .await
        .unwrap();
    let UsageEvidence::Reported(counters) = result.receipt.usage else {
        panic!("valid partial usage");
    };
    assert_eq!(counters.input_tokens, Some(10));
    assert_eq!(counters.total_tokens, Some(13));
    assert_eq!(counters.output_tokens, None);
    assert!(result.evidence.complete);
    assert_eq!(fixture.connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn exact_tool_call_is_a_proposal_and_invalid_arguments_retain_receipt() {
    for (arguments, valid) in [
        (r#"{"q":"tenant-scoped"}"#, true),
        (r#"{"q":"cut"#, false),
        ("null", false),
        ("[]", false),
    ] {
        let mut body = reply();
        body["choices"][0]["message"] = json!({"role":"assistant","content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"lookup","arguments":arguments}}]});
        body["choices"][0]["finish_reason"] = json!("tool_calls");
        let fixture = Fixture::json(body).await;
        let result = ModelGateway::new(config(&fixture.base))
            .unwrap()
            .complete(request(), &TestAuthority::default())
            .await
            .unwrap();
        if valid {
            assert_eq!(
                result.output,
                Ok(vec![OutputBlock::ToolCall {
                    call_id: "call-1".into(),
                    name: "lookup".into(),
                    arguments: json!({"q":"tenant-scoped"})
                }])
            );
        } else {
            assert_eq!(result.output, Err(OutputFailure::InvalidToolCall));
        }
        assert!(matches!(result.receipt.usage, UsageEvidence::Reported(_)));
        assert!(result.evidence.complete);
    }
}

#[tokio::test]
async fn valid_tool_history_preserves_exact_call_and_result_identity() {
    let fixture = Fixture::json(reply()).await;
    let mut input = request();
    input.messages.push(InputMessage::Assistant {
        blocks: vec![OutputBlock::ToolCall {
            call_id: "approved-call".into(),
            name: "lookup".into(),
            arguments: json!({"q":"scope"}),
        }],
    });
    input.messages.push(InputMessage::ToolResult {
        call_id: "approved-call".into(),
        name: "lookup".into(),
        text: "verified result".into(),
    });
    ModelGateway::new(config(&fixture.base))
        .unwrap()
        .complete(input, &TestAuthority::default())
        .await
        .unwrap();
    let seen = fixture.seen.lock().unwrap();
    assert_eq!(
        seen[0].1["messages"][2]["tool_calls"][0]["id"],
        "approved-call"
    );
    assert_eq!(seen[0].1["messages"][3]["tool_call_id"], "approved-call");
    assert_eq!(seen[0].1["messages"][3]["content"], "verified result");
}

#[tokio::test]
async fn reused_historical_call_id_is_invalid_output_and_keeps_the_bill() {
    let mut body = reply();
    body["choices"][0]["message"] = json!({"role":"assistant","content":null,
        "tool_calls":[{"id":"approved-call","type":"function","function":{"name":"lookup","arguments":"{}"}}]});
    body["choices"][0]["finish_reason"] = json!("tool_calls");
    let fixture = Fixture::json(body).await;
    let mut input = request();
    input.messages.extend([
        InputMessage::Assistant {
            blocks: vec![OutputBlock::ToolCall {
                call_id: "approved-call".into(),
                name: "lookup".into(),
                arguments: json!({}),
            }],
        },
        InputMessage::ToolResult {
            call_id: "approved-call".into(),
            name: "lookup".into(),
            text: "verified earlier result".into(),
        },
    ]);
    let result = ModelGateway::new(config(&fixture.base))
        .unwrap()
        .complete(input, &TestAuthority::default())
        .await
        .unwrap();
    assert_eq!(result.output, Err(OutputFailure::InvalidToolCall));
    assert!(matches!(result.receipt.usage, UsageEvidence::Reported(_)));
    assert_eq!(result.receipt.response_id.as_deref(), Some("response-23"));
    assert!(result.evidence.complete);
    assert!(
        serde_json::from_slice::<Value>(&result.evidence.body).unwrap()["choices"][0]["message"]
            ["tool_calls"][0]["id"]
            == "approved-call"
    );
    assert_eq!(fixture.connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_fresh_http_tool_proposal_and_its_result_continue_without_history_rejection() {
    let mut body = reply();
    body["choices"][0]["message"] = json!({"role":"assistant","content":null,
        "tool_calls":[{"id":"next-call","type":"function","function":{"name":"lookup","arguments":"{}"}}]});
    body["choices"][0]["finish_reason"] = json!("tool_calls");
    let first = Fixture::json(body).await;
    let mut input = request();
    input.messages.extend([
        InputMessage::Assistant {
            blocks: vec![OutputBlock::ToolCall {
                call_id: "approved-call".into(),
                name: "lookup".into(),
                arguments: json!({}),
            }],
        },
        InputMessage::ToolResult {
            call_id: "approved-call".into(),
            name: "lookup".into(),
            text: "earlier result".into(),
        },
    ]);
    let result = ModelGateway::new(config(&first.base))
        .unwrap()
        .complete(input.clone(), &TestAuthority::default())
        .await
        .unwrap();
    input.messages.extend([
        InputMessage::Assistant {
            blocks: result.output.unwrap(),
        },
        InputMessage::ToolResult {
            call_id: "next-call".into(),
            name: "lookup".into(),
            text: "next verified result".into(),
        },
    ]);
    input.context.attempt_id = "continued-attempt".into();
    let second = Fixture::json(reply()).await;
    let result = ModelGateway::new(config(&second.base))
        .unwrap()
        .complete(input, &TestAuthority::default())
        .await
        .unwrap();
    assert!(result.output.is_ok());
    assert_eq!(first.connections.load(Ordering::SeqCst), 1);
    assert_eq!(second.connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn tool_argument_shape_limits_match_the_next_history_input() {
    for depth in [64, 65] {
        let mut arguments = Value::Null;
        for _ in 0..depth {
            arguments = json!({"n":arguments});
        }
        let mut body = reply();
        body["choices"][0]["message"] = json!({"role":"assistant","content":null,
            "tool_calls":[{"id":"nested-call","type":"function","function":{"name":"lookup","arguments":arguments.to_string()}}]});
        body["choices"][0]["finish_reason"] = json!("tool_calls");
        let fixture = Fixture::json(body).await;
        let mut input = request();
        input.tools[0].parameters = json!({"type":"object"});
        let result = ModelGateway::new(config(&fixture.base))
            .unwrap()
            .complete(input.clone(), &TestAuthority::default())
            .await
            .unwrap();
        assert!(matches!(result.receipt.usage, UsageEvidence::Reported(_)));
        assert!(result.evidence.complete);
        if depth == 65 {
            assert_eq!(result.output, Err(OutputFailure::InvalidToolCall));
        } else {
            input.messages.extend([
                InputMessage::Assistant {
                    blocks: result.output.unwrap(),
                },
                InputMessage::ToolResult {
                    call_id: "nested-call".into(),
                    name: "lookup".into(),
                    text: "bounded result".into(),
                },
            ]);
            input.context.attempt_id = "nested-continuation".into();
            let continuation = Fixture::json(reply()).await;
            assert!(ModelGateway::new(config(&continuation.base))
                .unwrap()
                .complete(input, &TestAuthority::default())
                .await
                .unwrap()
                .output
                .is_ok());
            assert_eq!(continuation.connections.load(Ordering::SeqCst), 1);
        }
        assert_eq!(fixture.connections.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn unknown_finish_reason_and_duplicate_or_unoffered_tools_preserve_usage() {
    let mut cases = Vec::new();
    let mut body = reply();
    body["choices"][0]["finish_reason"] = json!("future-reason");
    cases.push((body, OutputFailure::IncompleteOutput));
    for name in ["unoffered", "lookup"] {
        let mut body = reply();
        let call =
            json!({"id":"duplicate","type":"function","function":{"name":name,"arguments":"{}"}});
        body["choices"][0]["message"] =
            json!({"role":"assistant","content":null,"tool_calls":[call.clone(),call]});
        body["choices"][0]["finish_reason"] = json!("tool_calls");
        cases.push((body, OutputFailure::InvalidToolCall));
    }
    for (body, failure) in cases {
        let fixture = Fixture::json(body).await;
        let result = ModelGateway::new(config(&fixture.base))
            .unwrap()
            .complete(request(), &TestAuthority::default())
            .await
            .unwrap();
        assert_eq!(result.output, Err(failure));
        assert!(matches!(result.receipt.usage, UsageEvidence::Reported(_)));
    }
}

#[tokio::test]
async fn lost_response_does_not_retry_or_invent_usage() {
    let fixture = Fixture::serve(200, String::new(), "", false, Duration::ZERO, true).await;
    let result = ModelGateway::new(config(&fixture.base))
        .unwrap()
        .complete(request(), &TestAuthority::default())
        .await
        .unwrap();
    assert_eq!(result.output, Err(OutputFailure::Transport));
    assert_eq!(result.receipt.usage, UsageEvidence::Missing);
    assert!(!result.evidence.complete);
    assert_eq!(fixture.connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn lost_body_preserves_bounded_partial_evidence_without_terminal_usage() {
    let prefix = r#"{"id":"partial","usage":{"prompt_tokens":10,"completion_tokens":0}}"#;
    let fixture = Fixture::serve(200, prefix.into(), "", false, Duration::ZERO, true).await;
    let result = ModelGateway::new(config(&fixture.base))
        .unwrap()
        .complete(request(), &TestAuthority::default())
        .await
        .unwrap();
    assert_eq!(result.output, Err(OutputFailure::Transport));
    assert_eq!(result.receipt.http_status, Some(200));
    assert_eq!(result.receipt.usage, UsageEvidence::Missing);
    assert_eq!(result.evidence.body, prefix.as_bytes());
    assert!(!result.evidence.complete);
    assert_eq!(fixture.connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn provider_failure_and_redirect_never_retry_or_follow() {
    for status in [307, 429, 500, 503] {
        let fixture = Fixture::serve(
            status,
            format!("private error {KEY} {PROMPT}"),
            "location: http://127.0.0.1:9/forbidden\r\nretry-after: 0\r\n",
            false,
            Duration::ZERO,
            false,
        )
        .await;
        let result = ModelGateway::new(config(&fixture.base))
            .unwrap()
            .complete(request(), &TestAuthority::default())
            .await
            .unwrap();
        assert_eq!(result.output, Err(OutputFailure::ProviderRejected));
        assert_eq!(result.receipt.http_status, Some(status));
        assert!(result.evidence.complete);
        assert_eq!(fixture.connections.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn timeout_is_one_uncertain_attempt() {
    let fixture = Fixture::serve(
        200,
        reply().to_string(),
        "",
        false,
        Duration::from_secs(1),
        false,
    )
    .await;
    let mut cfg = config(&fixture.base);
    cfg.limits.timeout = Duration::from_millis(40);
    cfg.limits.connect_timeout = Duration::from_millis(20);
    let result = ModelGateway::new(cfg)
        .unwrap()
        .complete(request(), &TestAuthority::default())
        .await
        .unwrap();
    assert_eq!(result.output, Err(OutputFailure::Timeout));
    assert!(!result.evidence.complete);
    assert_eq!(fixture.connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn content_length_and_chunked_success_or_error_body_are_capped_before_decode() {
    for status in [200, 500] {
        for chunked in [false, true] {
            let fixture =
                Fixture::serve(status, "x".repeat(4096), "", chunked, Duration::ZERO, false).await;
            let mut cfg = config(&fixture.base);
            cfg.limits.max_response_bytes = 128;
            let result = ModelGateway::new(cfg)
                .unwrap()
                .complete(request(), &TestAuthority::default())
                .await
                .unwrap();
            assert_eq!(result.output, Err(OutputFailure::ResponseTooLarge));
            assert!(result.evidence.body.len() <= 128);
            assert!(!result.evidence.complete);
        }
    }
}

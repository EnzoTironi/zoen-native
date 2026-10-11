use super::*;

#[tokio::test]
async fn paid_replay_and_changed_binding_make_no_second_request() {
    let fixture = Fixture::json(reply()).await;
    let gateway = ModelGateway::new(config(&fixture.base)).unwrap();
    let authority = TestAuthority::default();
    gateway.complete(request(), &authority).await.unwrap();
    assert_eq!(
        gateway.complete(request(), &authority).await,
        Err(GatewayError::NotFresh)
    );
    let mut changed = request();
    changed.context.price_version = "price-2".into();
    assert_eq!(
        gateway.complete(changed, &authority).await,
        Err(GatewayError::Admission(AdmissionError::Denied))
    );
    assert_eq!(fixture.connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn equivalent_json_key_order_does_not_create_a_new_effect() {
    let fixture = Fixture::json(reply()).await;
    let gateway = ModelGateway::new(config(&fixture.base)).unwrap();
    let authority = TestAuthority::default();
    gateway.complete(request(), &authority).await.unwrap();
    let mut reordered = request();
    reordered.tools[0].parameters = serde_json::from_str(r#"{"additionalProperties":false,"required":["q"],"properties":{"q":{"type":"string"}},"type":"object"}"#).unwrap();
    assert_eq!(
        gateway.complete(reordered, &authority).await,
        Err(GatewayError::NotFresh)
    );
    assert_eq!(fixture.connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn denied_and_unknown_admission_send_nothing() {
    let fixture = Fixture::json(reply()).await;
    let gateway = ModelGateway::new(config(&fixture.base)).unwrap();
    for error in [AdmissionError::Denied, AdmissionError::Unavailable] {
        assert_eq!(
            gateway.complete(request(), &Refuse(error)).await,
            Err(GatewayError::Admission(error))
        );
    }
    assert_eq!(fixture.connections.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn concurrent_duplicate_has_one_fresh_admission_and_one_provider_call() {
    let fixture = Fixture::json(reply()).await;
    let gateway = ModelGateway::new(config(&fixture.base)).unwrap();
    let authority = TestAuthority::default();
    let (a, b) = tokio::join!(
        gateway.complete(request(), &authority),
        gateway.complete(request(), &authority)
    );
    assert_eq!([a, b].iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(fixture.connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cancellation_after_send_never_reconstructs_admission() {
    let fixture = Fixture::serve(
        200,
        reply().to_string(),
        "",
        false,
        Duration::from_secs(1),
        false,
    )
    .await;
    let gateway = ModelGateway::new(config(&fixture.base)).unwrap();
    let authority = TestAuthority::default();
    let mut call = Box::pin(gateway.complete(request(), &authority));
    tokio::select! {
        _ = call.as_mut() => panic!("provider should still be pending"),
        _ = fixture.received.notified() => {},
        _ = tokio::time::sleep(Duration::from_secs(2)) => panic!("provider request was not observed"),
    }
    drop(call);
    assert_eq!(
        gateway.complete(request(), &authority).await,
        Err(GatewayError::NotFresh)
    );
    assert_eq!(fixture.connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn unsupported_operations_and_oversized_or_unpaired_input_fail_before_admission() {
    let fixture = Fixture::json(reply()).await;
    let gateway = ModelGateway::new(config(&fixture.base)).unwrap();
    let authority = TestAuthority::default();
    for operation in [
        Operation::StreamingCompletion,
        Operation::StructuredOutput,
        Operation::Embedding,
        Operation::Reranking,
        Operation::Transcription,
        Operation::AudioGeneration,
        Operation::ImageGeneration,
    ] {
        let mut input = request();
        input.operation = operation;
        assert!(!gateway.supports(operation));
        assert_eq!(
            gateway.complete(input, &authority).await,
            Err(GatewayError::UnsupportedOperation)
        );
    }
    let mut input = request();
    input.messages = vec![InputMessage::User {
        text: "x".repeat(40_000),
    }];
    assert_eq!(
        gateway.complete(input, &authority).await,
        Err(GatewayError::InvalidRequest)
    );
    let mut input = request();
    input.messages.push(InputMessage::ToolResult {
        call_id: "missing".into(),
        name: "lookup".into(),
        text: "injected".into(),
    });
    assert_eq!(
        gateway.complete(input, &authority).await,
        Err(GatewayError::InvalidRequest)
    );
    assert_eq!(authority.calls.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.connections.load(Ordering::SeqCst), 0);
}

#[test]
fn destination_and_configuration_validation_are_closed() {
    for destination in [
        "http://example.com/v1",
        "http://localhost:10/v1",
        "http://127.0.0.1:10/v1?secret=x",
        "http://a:b@127.0.0.1:10/v1",
        "file:///secret",
        "http://127.0.0.1:10/v1#fragment",
    ] {
        assert_eq!(
            ModelGateway::new(config(destination)).unwrap_err(),
            GatewayError::InvalidConfiguration
        );
    }
    let mut cfg = config("http://127.0.0.1:10/v1");
    cfg.endpoint_policy = EndpointPolicy::Https;
    assert_eq!(
        ModelGateway::new(cfg).unwrap_err(),
        GatewayError::InvalidConfiguration
    );
    let mut cfg = config("https://api.openai.com/v1");
    cfg.endpoint_policy = EndpointPolicy::Https;
    assert!(ModelGateway::new(cfg).is_ok());
}

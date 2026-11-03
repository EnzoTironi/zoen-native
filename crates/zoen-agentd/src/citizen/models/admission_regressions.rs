use super::*;
use zoen_models::{Admission, DispatchAuthority, DispatchRequest};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

pub(super) fn dispatch() -> TestResult<DispatchRequest> {
    dispatch_for("one", "owner")
}

pub(super) fn dispatch_for(attempt: &str, owner: &str) -> TestResult<DispatchRequest> {
    let gateway = ModelGateway::new(GatewayConfig {
        base_url: "http://127.0.0.1:1/v1".into(),
        endpoint_policy: EndpointPolicy::LoopbackFixture,
        model: "fixture".into(),
        credential_ref: "fixture".into(),
        api_key: "fixture".into(),
        limits: ModelLimits {
            max_request_bytes: 32768,
            max_response_bytes: 32768,
            max_output_tokens: 100,
            timeout: Duration::from_secs(2),
            connect_timeout: Duration::from_secs(1),
        },
    })?;
    Ok(gateway.preflight(&ModelRequest {
        context: AttemptContext {
            attempt_id: attempt.into(),
            run_id: "run".into(),
            owner: owner.into(),
            agent: "agent".into(),
            device: "device".into(),
            space: "space".into(),
            authority_version: "authority-v1".into(),
            definition_version: "definition-v1".into(),
            price_version: "price-v1".into(),
        },
        operation: Operation::ChatCompletion,
        messages: vec![InputMessage::User {
            text: "Private text never enters quota state".into(),
        }],
        tools: vec![],
        max_output_tokens: 10,
    })?)
}

#[tokio::test]
async fn a_denied_attempt_never_turns_into_an_already_dispatched_result() -> TestResult {
    let authority = CheapTierAuthority::new(0);
    let request = dispatch()?;
    assert_eq!(authority.admit(&request).await, Err(AdmissionError::Denied));
    assert_eq!(authority.admit(&request).await, Err(AdmissionError::Denied));
    Ok(())
}

#[tokio::test]
async fn a_known_attempt_cannot_borrow_another_owner_or_gateway_binding() -> TestResult {
    let authority = CheapTierAuthority::new(1);
    let request = dispatch()?;
    assert_eq!(authority.admit(&request).await, Ok(Admission::Fresh));
    let mut foreign = request.clone();
    foreign.context.owner = "another-owner".into();
    assert_eq!(authority.admit(&foreign).await, Err(AdmissionError::Denied));
    let mut changed = request.clone();
    changed.descriptor.model = "different-model".into();
    assert_eq!(authority.admit(&changed).await, Err(AdmissionError::Denied));
    assert_eq!(authority.admit(&request).await, Ok(Admission::AlreadyKnown));
    Ok(())
}

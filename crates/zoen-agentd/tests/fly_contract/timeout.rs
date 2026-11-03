use super::*;

#[tokio::test]
async fn subsecond_timeout_never_dispatches_a_command() -> Result<(), SandboxError> {
    let api = MockFly::new();
    let fly = api.provider();
    let lease = fly.acquire(&spec(), "ana").await?;
    for timeout in [Duration::ZERO, Duration::from_millis(999)] {
        let request = ExecRequest {
            timeout,
            ..ExecRequest::sh("true")
        };
        let output = fly.exec(&lease, request).await?;
        assert!(output.timed_out);
        assert_eq!(output.exit_code, None);
    }
    assert!(matches!(
        api.guest.lock().map(|guest| guest.exec_calls),
        Ok(0)
    ));
    fly.release(lease).await
}

#[tokio::test]
async fn zero_timeout_keeps_a_cold_session_cold() -> Result<(), SessionError> {
    let api = MockFly::new();
    let session = session(api.provider());
    let request = ExecRequest {
        timeout: Duration::ZERO,
        ..ExecRequest::sh("true")
    };
    assert!(session.exec(request, NOW).await?.timed_out);
    assert_eq!(session.state().await, "cold");
    Ok(())
}

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
    fly.release(lease).await
}

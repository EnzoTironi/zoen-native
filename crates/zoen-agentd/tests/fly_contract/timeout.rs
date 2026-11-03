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

#[tokio::test]
async fn poisoned_budget_refuses_new_work_but_allows_cleanup() -> Result<(), SessionError> {
    let api = MockFly::new();
    let budget = Arc::new(Mutex::new(DailyBudget::new(3600, 0, NOW)));
    let session = ToolSession::new(
        api.provider(),
        spec(),
        "ana".into(),
        Duration::ZERO,
        budget.clone(),
    );
    session.exec(ExecRequest::sh("true"), NOW).await?;
    assert!(std::thread::spawn(move || {
        let _guard = budget.lock();
        panic!("inject budget lock poisoning");
    })
    .join()
    .is_err());
    assert!(matches!(
        session.exec(ExecRequest::sh("true"), NOW).await,
        Err(SessionError::BudgetUnavailable)
    ));
    session.finish(NOW).await?;
    session.finish(NOW).await?;
    assert_eq!(session.state().await, "done");
    assert_eq!(session.usage().len(), 1);
    Ok(())
}

#[tokio::test]
async fn invalid_exit_code_is_a_backend_error() -> Result<(), SandboxError> {
    let api = MockFly::new();
    let fly = api.provider();
    let lease = fly.acquire(&spec(), "ana").await?;
    for value in ["2147483648", "1.5", r#""0""#] {
        let request = ExecRequest::sh(&format!("exit-code-fixture:{value}"));
        let result = fly.exec(&lease, request).await;
        assert!(matches!(result, Err(SandboxError::Backend(message)) if message.contains("invalid exit code")));
    }
    fly.release(lease).await
}

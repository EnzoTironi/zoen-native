use super::*;

#[tokio::test]
async fn resume_recovers_when_start_errors_but_machine_is_running() {
    let api = MockFly::new();
    let session = session(api.provider());
    session
        .exec(ExecRequest::sh("echo before"), NOW)
        .await
        .unwrap();
    assert!(session.tick(NOW).await.unwrap());
    {
        let mut guest = api.guest.lock().unwrap();
        guest.resume_failures = 1;
        guest.state_response = Some((200, json!({"state": "started"})));
    }
    session
        .exec(ExecRequest::sh("echo after"), NOW)
        .await
        .unwrap();
    assert_eq!(session.state().await, "running");
    assert_eq!(api.guest.lock().unwrap().resume_calls, 1);
    session.finish(NOW).await.unwrap();
}

#[tokio::test]
async fn failed_suspend_charges_running_time_only_after_successful_retry() {
    let api = MockFly::new();
    api.guest.lock().unwrap().suspend_failures = 1;
    let session = session(api.provider());
    session
        .exec(ExecRequest::sh("echo running"), NOW)
        .await
        .unwrap();
    assert!(session.tick(NOW).await.is_err());
    assert_eq!(session.state().await, "running");
    assert!(session.usage().is_empty());

    assert!(session.tick(NOW).await.unwrap());
    assert_eq!(session.state().await, "suspended");
    assert_eq!(session.usage().len(), 1);
    session.finish(NOW).await.unwrap();
    assert_eq!(session.usage().len(), 1);
}

#[tokio::test]
async fn suspend_recovers_when_response_errors_but_machine_is_suspended() {
    let api = MockFly::new();
    let session = session(api.provider());
    session
        .exec(ExecRequest::sh("echo running"), NOW)
        .await
        .unwrap();
    {
        let mut guest = api.guest.lock().unwrap();
        guest.suspend_failures = 1;
        guest.state_response = Some((200, json!({"state": "suspended"})));
    }
    assert!(session.tick(NOW).await.unwrap());
    assert_eq!(session.state().await, "suspended");
    session.finish(NOW).await.unwrap();
    assert_eq!(session.usage().len(), 1);
}

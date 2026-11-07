//! Existing signed WebSocket fixture, with real Net and SQLite recovery.
use super::*;
use protocol::managed_agents::{Record as ManagedRecord, Request as ManagedRequest, Service};
use zeroize::Zeroizing;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn service() -> Service {
    Service {
        id: "01".repeat(32),
        template_sha256: "02".repeat(32),
    }
}
fn prepare(client: &Arc<Client>) -> tokio::task::JoinHandle<R<ManagedRecord>> {
    let client = client.clone();
    tokio::spawn(async move {
        let account = client.account().ok_or_else(changed)?;
        client
            .prepare_managed_enrollment(
                &service(),
                "Helper",
                "managed_helper",
                &account.identity_id,
                &account.device_id,
            )
            .await
    })
}
fn queued(request: &ManagedRequest, owner: &str) -> ManagedRecord {
    ManagedRecord {
        request: Some(request.clone()),
        owner: owner.into(),
        state: "queued".into(),
        agent: String::new(),
        offer_digest: String::new(),
        sealed_offer: vec![],
        expires_ms: 0,
        onboarding_completed: false,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn managed_owner_transport_retains_unknown_and_replays_after_cold_restart() -> TestResult {
    let dir = Directory::new()?;
    let mut relay = relay().await;
    let (client, device_key) = owner(dir.path(), &relay.url);
    let account = client.account().ok_or("account")?;
    let agreement = Zeroizing::new(
        client
            .lock()
            .net
            .profiles
            .agreement
            .as_ref()
            .ok_or("agreement")?
            .secret(),
    );
    online(&client).await;
    let running = prepare(&client);
    let sent = call(&mut relay.calls).await;
    let AgentRequest::PrepareManaged(request) = sent.op else {
        return Err("prepare request".into());
    };
    // The durable intent already exists while the response remains unresolved.
    assert_eq!(
        client.pending_managed_enrollment(&account.identity_id, &account.device_id)?,
        Some(request.clone())
    );
    sent.response
        .send(Err("injected failure after dispatch".into()))
        .map_err(|_| "dropped response")?;
    assert!(running.await?.is_err());
    client.stop_sync();
    drop(client);

    let cold = Client::open(dir.path(), "en".into())?;
    assert!(cold.lock().unlock(Some(device_key.to_vec()))?);
    cold.lock().unlock_profile(Some(agreement.to_vec()))?;
    assert_eq!(
        cold.pending_managed_enrollment(&account.identity_id, &account.device_id)?,
        Some(request.clone())
    );
    online(&cold).await;
    assert!(cold
        .prepare_managed_enrollment(
            &service(),
            "Changed",
            "managed_helper",
            &account.identity_id,
            &account.device_id
        )
        .await
        .is_err());
    assert!(relay.calls.try_recv().is_err());
    let running = prepare(&cold);
    let replay = call(&mut relay.calls).await;
    assert_eq!(replay.op, AgentRequest::PrepareManaged(request.clone()));
    replay
        .response
        .send(Ok(Reply::ManagedAgent(queued(
            &request,
            &account.identity_id,
        ))))
        .map_err(|_| "dropped response")?;
    assert_eq!(running.await??, queued(&request, &account.identity_id));

    let another = cold.clone();
    let identity = account.identity_id.clone();
    let device = account.device_id.clone();
    let running =
        tokio::spawn(async move { another.managed_enrollment_status(&identity, &device).await });
    let status = call(&mut relay.calls).await;
    assert_eq!(
        status.op,
        AgentRequest::ManagedStatus {
            service: service().id,
            operation: request.operation.clone()
        }
    );
    status
        .response
        .send(Ok(Reply::ManagedAgent(queued(&request, &"03".repeat(32)))))
        .map_err(|_| "dropped response")?;
    assert!(running.await?.is_err());
    assert_eq!(
        cold.pending_managed_enrollment(&account.identity_id, &account.device_id)?,
        Some(request)
    );
    cold.stop_sync();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn managed_owner_transport_rejects_old_connection_completion() -> TestResult {
    let dir = Directory::new()?;
    let mut relay = relay().await;
    let (client, _) = owner(dir.path(), &relay.url);
    let account = client.account().ok_or("account")?;
    online(&client).await;
    let running = prepare(&client);
    let sent = call(&mut relay.calls).await;
    let AgentRequest::PrepareManaged(request) = sent.op else {
        return Err("prepare request".into());
    };
    client.stop_sync();
    online(&client).await;
    // A reply from the replaced channel cannot be applied to the new connection.
    let _ = sent.response.send(Ok(Reply::ManagedAgent(queued(
        &request,
        &account.identity_id,
    ))));
    assert!(running.await?.is_err());
    assert_eq!(
        client.pending_managed_enrollment(&account.identity_id, &account.device_id)?,
        Some(request)
    );
    client.stop_sync();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn managed_terminal_restart_preserves_ambiguous_approval_and_checks_canonical_state(
) -> TestResult {
    let dir = Directory::new()?;
    let mut relay = relay().await;
    let (client, _) = owner(dir.path(), &relay.url);
    let account = client.account().ok_or("account")?;
    online(&client).await;
    let running = prepare(&client);
    let sent = call(&mut relay.calls).await;
    let AgentRequest::PrepareManaged(request) = sent.op else {
        return Err("prepare request".into());
    };
    sent.response
        .send(Ok(Reply::ManagedAgent(queued(
            &request,
            &account.identity_id,
        ))))
        .map_err(|_| "dropped response")?;
    running.await??;
    for state in ["queued", "offered", "registered", "expired", "rejected"] {
        let mut record = queued(&request, &account.identity_id);
        record.state = state.into();
        if ["offered", "registered", "expired"].contains(&state) {
            record.agent = "21".repeat(32);
            record.offer_digest = "22".repeat(32);
            record.sealed_offer = vec![1; 40];
            record.expires_ms = 1;
        }
        // Presence alone is sufficient to preserve even an unreadable approval.
        if state == "expired" {
            client
                .lock()
                .store
                .set_meta("agent.enrollment.owner-receipt.v1", "unknown")?;
        }
        let another = client.clone();
        let operation = request.operation.clone();
        let identity = account.identity_id.clone();
        let device = account.device_id.clone();
        let running = tokio::spawn(async move {
            another
                .discard_managed_enrollment(&operation, &identity, &device)
                .await
        });
        let status = call(&mut relay.calls).await;
        status
            .response
            .send(Ok(Reply::ManagedAgent(record)))
            .map_err(|_| "dropped response")?;
        if state == "rejected" {
            assert!(running.await??);
        } else {
            assert!(running.await?.is_err());
        }
        if state == "expired" {
            client
                .lock()
                .store
                .meta_delete("agent.enrollment.owner-receipt.v1")?;
        }
    }
    assert!(client
        .pending_managed_enrollment(&account.identity_id, &account.device_id)?
        .is_none());
    client.stop_sync();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn managed_cancel_unknown_replays_the_same_disposition_after_cold_restart() -> TestResult {
    let dir = Directory::new()?;
    let mut relay = relay().await;
    let (client, key) = owner(dir.path(), &relay.url);
    let account = client.account().ok_or("account")?;
    let agreement = Zeroizing::new(
        client
            .lock()
            .net
            .profiles
            .agreement
            .as_ref()
            .ok_or("agreement")?
            .secret(),
    );
    online(&client).await;
    let preparing = prepare(&client);
    let sent = call(&mut relay.calls).await;
    let AgentRequest::PrepareManaged(request) = sent.op else {
        return Err("prepare".into());
    };
    sent.response
        .send(Ok(Reply::ManagedAgent(queued(
            &request,
            &account.identity_id,
        ))))
        .map_err(|_| "prepare response")?;
    preparing.await??;
    let running = {
        let client = client.clone();
        let operation = request.operation.clone();
        let identity = account.identity_id.clone();
        let device = account.device_id.clone();
        tokio::spawn(async move {
            client
                .cancel_managed_enrollment(&operation, &identity, &device)
                .await
        })
    };
    let sent = call(&mut relay.calls).await;
    assert_eq!(sent.op, AgentRequest::CancelManaged(request.clone()));
    assert!(client.managed_enrollment_cancelling(&account.identity_id, &account.device_id)?);
    sent.response
        .send(Err("lost cancellation ACK".into()))
        .map_err(|_| "cancel response")?;
    assert!(running.await?.is_err());
    client.stop_sync();
    drop(client);
    let cold = Client::open(dir.path(), "en".into())?;
    assert!(cold.lock().unlock(Some(key.to_vec()))?);
    cold.lock().unlock_profile(Some(agreement.to_vec()))?;
    online(&cold).await;
    let running = {
        let client = cold.clone();
        let identity = account.identity_id.clone();
        let device = account.device_id.clone();
        tokio::spawn(async move {
            client
                .managed_enrollment_read_view(&identity, &device)
                .await
        })
    };
    let sent = call(&mut relay.calls).await;
    assert_eq!(sent.op, AgentRequest::CancelManaged(request.clone()));
    let mut cancelled = queued(&request, &account.identity_id);
    cancelled.state = "cancelled".into();
    sent.response
        .send(Ok(Reply::ManagedAgent(cancelled)))
        .map_err(|_| "replay response")?;
    let view = running.await??.ok_or("view")?;
    assert_eq!(view.state, "cancelled");
    assert_eq!(view.operation_id, request.operation);
    cold.stop_sync();
    Ok(())
}

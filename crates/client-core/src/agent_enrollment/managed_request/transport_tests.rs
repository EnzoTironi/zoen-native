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

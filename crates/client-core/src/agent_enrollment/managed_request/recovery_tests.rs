//! Signed socket fixture: discovery is observational; adoption is scoped and durable.
use super::*;
use protocol::managed_agents::{Page, Record as ManagedRecord, Request as ManagedRequest, Service};
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn managed_discovery_recovery_refuses_foreign_binding_and_survives_cold_restart() -> TestResult
{
    let dir = Directory::new()?;
    let mut relay = relay().await;
    let (client, key) = owner(dir.path(), &relay.url);
    let account = client.account().ok_or("account")?;
    let agreement = zeroize::Zeroizing::new(
        client
            .lock()
            .net
            .profiles
            .agreement
            .as_ref()
            .ok_or("agreement")?
            .secret(),
    );
    let request = ManagedRequest {
        operation: "11".repeat(32),
        service: Some(Service {
            id: "22".repeat(32),
            template_sha256: "33".repeat(32),
        }),
        name: "Recovered".into(),
        handle: "recovered_agent".into(),
        recipient_public_key: client
            .lock()
            .net
            .profiles
            .agreement
            .as_ref()
            .ok_or("agreement")?
            .public_hex(),
    };
    let record = ManagedRecord {
        request: Some(request.clone()),
        owner: account.identity_id.clone(),
        state: "queued".into(),
        agent: String::new(),
        offer_digest: String::new(),
        sealed_offer: vec![],
        expires_ms: 0,
        onboarding_completed: false,
    };
    online(&client).await;
    for invalid in [false, true] {
        let c = client.clone();
        let a = account.clone();
        let task = tokio::spawn(async move {
            c.discover_managed_enrollments("", &a.identity_id, &a.device_id)
                .await
        });
        let sent = call(&mut relay.calls).await;
        assert_eq!(
            sent.op,
            AgentRequest::DiscoverManaged {
                after: String::new()
            }
        );
        let mut response = record.clone();
        if invalid {
            response.owner = "44".repeat(32);
        }
        sent.response
            .send(Ok(Reply::ManagedAgents(Page {
                records: vec![response],
                next_operation: String::new(),
            })))
            .map_err(|_| "response")?;
        if invalid {
            assert!(task.await?.is_err());
        } else {
            assert_eq!(task.await??.requests[0].operation_id, request.operation);
        }
        assert!(client
            .pending_managed_enrollment(&account.identity_id, &account.device_id)?
            .is_none());
    }
    for invalid in [true, false] {
        let c = client.clone();
        let a = account.clone();
        let operation = request.operation.clone();
        let task = tokio::spawn(async move {
            c.recover_managed_enrollment_view(&operation, &a.identity_id, &a.device_id)
                .await
        });
        let sent = call(&mut relay.calls).await;
        assert_eq!(
            sent.op,
            AgentRequest::RecoverManaged {
                operation: request.operation.clone()
            }
        );
        let mut response = record.clone();
        if invalid {
            response
                .request
                .as_mut()
                .ok_or("request")?
                .recipient_public_key = "55".repeat(32);
        }
        sent.response
            .send(Ok(Reply::ManagedAgent(response)))
            .map_err(|_| "response")?;
        if invalid {
            assert!(task.await?.is_err());
            assert!(client
                .pending_managed_enrollment(&account.identity_id, &account.device_id)?
                .is_none());
        } else {
            assert_eq!(task.await??.operation_id, request.operation);
        }
    }
    client.stop_sync();
    drop(client);
    let cold = Client::open(dir.path(), "en".into())?;
    assert!(cold.lock().unlock(Some(key.to_vec()))?);
    cold.lock().unlock_profile(Some(agreement.to_vec()))?;
    assert_eq!(
        cold.pending_managed_enrollment(&account.identity_id, &account.device_id)?,
        Some(request.clone())
    );
    online(&cold).await;
    let c = cold.clone();
    let a = account.clone();
    let operation = request.operation.clone();
    let task = tokio::spawn(async move {
        c.recover_managed_enrollment_view(&operation, &a.identity_id, &a.device_id)
            .await
    });
    let sent = call(&mut relay.calls).await;
    cold.stop_sync();
    online(&cold).await;
    let _ = sent.response.send(Ok(Reply::ManagedAgent(record)));
    assert!(task.await?.is_err());
    assert_eq!(
        cold.pending_managed_enrollment(&account.identity_id, &account.device_id)?,
        Some(request)
    );
    for malformed_receipt in [true, false] {
        if malformed_receipt {
            cold.lock()
                .store
                .set_meta("agent.enrollment.owner-receipt.v1", "unknown")?;
        }
        let c = cold.clone();
        let a = account.clone();
        let task = tokio::spawn(async move {
            c.managed_enrollment_read_view(&a.identity_id, &a.device_id)
                .await
        });
        let sent = call(&mut relay.calls).await;
        let AgentRequest::PrepareManaged(ref held) = sent.op else {
            return Err("exact recovery replay".into());
        };
        let completed = ManagedRecord {
            request: Some(held.clone()),
            owner: account.identity_id.clone(),
            state: "registered".into(),
            agent: "66".repeat(32),
            offer_digest: "77".repeat(32),
            sealed_offer: vec![1; 128],
            expires_ms: 1,
            onboarding_completed: true,
        };
        sent.response
            .send(Ok(Reply::ManagedAgent(completed)))
            .map_err(|_| "response")?;
        if malformed_receipt {
            assert!(task.await?.is_err());
            assert_eq!(
                cold.lock()
                    .store
                    .meta("agent.enrollment.owner-receipt.v1")?
                    .as_deref(),
                Some("unknown")
            );
            assert!(cold
                .pending_managed_enrollment(&account.identity_id, &account.device_id)?
                .is_some());
            cold.lock()
                .store
                .meta_delete("agent.enrollment.owner-receipt.v1")?;
        } else {
            assert!(task.await??.is_none());
            assert!(cold
                .pending_managed_enrollment(&account.identity_id, &account.device_id)?
                .is_none());
        }
    }
    cold.stop_sync();
    Ok(())
}

//! Reuse the existing authenticated offline definition fixture, with lifecycle RPCs.
use super::*;
use crate::{AgentControl, AgentControlDto};
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn control_record(
    status: &str,
    revision: u64,
    operation: &str,
    device: &str,
) -> protocol::agent_control::Record {
    protocol::agent_control::Record {
        status: status.into(),
        revision,
        operation: operation.into(),
        action: if operation.is_empty() { "" } else { "pause" }.into(),
        device: device.into(),
    }
}
fn dispatched_op(agent: &str, operation: &str) -> AgentRequest {
    AgentRequest::ControlReviewed {
        agent: agent.into(),
        operation: operation.into(),
        action: protocol::AgentAction::Pause,
        revision: 1,
    }
}
fn answer(call: Call, record: protocol::agent_control::Record) -> TestResult {
    assert!(matches!(call.op, AgentRequest::ControlStatus { .. }));
    call.response
        .send(Ok(Reply::AgentControl(record)))
        .map_err(|_| "response dropped")?;
    Ok(())
}
fn control(c: &Arc<Client>, op: &str, action: AgentControl) -> tokio::task::JoinHandle<R<String>> {
    let c2 = c.clone();
    let mut input = command(c, op);
    if op == "agent_control_change" {
        input["observed_revision"] = "1".into();
        input["action"] = serde_json::json!(action);
    }
    tokio::spawn(async move { c2.client_request(input.to_string()).await })
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_unknown_survives_cold_restart_and_never_dispatches_opposite() -> TestResult {
    let dir = Directory::new()?;
    let mut r = relay().await;
    let (c, key) = owner(dir.path(), &r.url);
    online(&c).await;
    let task = control(&c, "agent_control_change", AgentControl::Pause);
    answer(
        call(&mut r.calls).await,
        control_record("active", 1, "", ""),
    )?;
    let dispatched = call(&mut r.calls).await;
    let operation = match &dispatched.op {
        AgentRequest::ControlReviewed {
            action: protocol::AgentAction::Pause,
            operation,
            revision: 1,
            ..
        } => operation.clone(),
        other => return Err(format!("unexpected {other:?}").into()),
    };
    dispatched
        .response
        .send(Err("lost ACK before late delivery".into()))
        .map_err(|_| "response dropped")?;
    answer(
        call(&mut r.calls).await,
        control_record("active", 1, "", ""),
    )?;
    let dto: AgentControlDto = serde_json::from_str(&task.await??)?;
    assert_eq!(dto.pending, Some(AgentControl::Pause));
    c.stop_sync();
    drop(c);
    let cold = Client::open(dir.path(), "en".into())?;
    assert!(cold.lock().unlock(Some(key.to_vec()))?);
    online(&cold).await;
    // Contradictory retry after process death performs only a canonical read.
    let opposite = control(&cold, "agent_control_change", AgentControl::Resume);
    assert!(opposite.await?.is_err());
    assert!(r.calls.try_recv().is_err());
    let retained = control(&cold, "agent_control_read", AgentControl::Pause);
    answer(
        call(&mut r.calls).await,
        control_record("active", 1, "", ""),
    )?;
    let dto: AgentControlDto = serde_json::from_str(&retained.await??)?;
    assert_eq!(dto.pending, Some(AgentControl::Pause));
    // Explicit retry after cold reopen uses the original identity and bytes.
    let mut wrong = target(&cold);
    wrong.identity = "f".repeat(64);
    assert!(cold
        .agent_control_retry(wrong, "1", &operation, AgentControl::Pause)
        .await
        .is_err());
    let c2 = cold.clone();
    let held_id = operation.clone();
    let retry = tokio::spawn(async move {
        c2.agent_control_retry(target(&c2), "1", &held_id, AgentControl::Pause)
            .await
    });
    answer(
        call(&mut r.calls).await,
        control_record("active", 1, "", ""),
    )?;
    let resend = call(&mut r.calls).await;
    assert_eq!(resend.op, dispatched_op(&target(&cold).agent, &operation));
    resend
        .response
        .send(Ok(Reply::AgentControl(control_record(
            "paused",
            2,
            &operation,
            &target(&cold).device,
        ))))
        .map_err(|_| "response dropped")?;
    let dto = retry.await??;
    assert_eq!(dto.pending, None);
    assert_eq!(dto.status, AgentStatus::Paused);
    assert_eq!(dto.outcome, crate::AgentControlOutcome::Confirmed);
    assert!(cold
        .agent_control_retry(target(&cold), "1", &operation, AgentControl::Pause)
        .await
        .is_err());
    assert!(r.calls.try_recv().is_err());
    cold.stop_sync();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_public_api_does_not_acknowledge_a_different_winner() -> TestResult {
    let dir = Directory::new()?;
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    let c2 = c.clone();
    let agent = target(&c).agent;
    let task = tokio::spawn(async move { c2.control_agent(agent, AgentControl::Pause).await });
    answer(
        call(&mut r.calls).await,
        control_record("active", 1, "", ""),
    )?;
    answer(
        call(&mut r.calls).await,
        control_record("active", 1, "", ""),
    )?;
    let sent = call(&mut r.calls).await;
    assert!(matches!(sent.op, AgentRequest::ControlReviewed { .. }));
    let mut winner = control_record("active", 2, &"e".repeat(64), &"f".repeat(64));
    winner.action = "resume".into();
    sent.response
        .send(Ok(Reply::AgentControl(winner)))
        .map_err(|_| "response dropped")?;
    assert!(task.await?.is_err());
    assert!(r.calls.try_recv().is_err());
    c.stop_sync();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_contradictory_receipt_cannot_clear_durable_intent() -> TestResult {
    let dir = Directory::new()?;
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    let task = control(&c, "agent_control_change", AgentControl::Pause);
    answer(
        call(&mut r.calls).await,
        control_record("active", 1, "", ""),
    )?;
    let sent = call(&mut r.calls).await;
    let operation = match &sent.op {
        AgentRequest::ControlReviewed { operation, .. } => operation.clone(),
        other => return Err(format!("unexpected {other:?}").into()),
    };
    // Exact id/action/device with the wrong target is not a canonical receipt.
    sent.response
        .send(Ok(Reply::AgentControl(control_record(
            "active",
            2,
            &operation,
            &target(&c).device,
        ))))
        .map_err(|_| "response dropped")?;
    assert!(task.await?.is_err());
    let read = control(&c, "agent_control_read", AgentControl::Pause);
    answer(
        call(&mut r.calls).await,
        control_record("paused", 1, "", ""),
    )?;
    let dto: AgentControlDto = serde_json::from_str(&read.await??)?;
    assert_eq!(dto.pending, Some(AgentControl::Pause));
    assert_eq!(dto.outcome, crate::AgentControlOutcome::Unknown);
    c.stop_sync();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_overlap_cannot_borrow_another_actions_or_revisions_receipt() -> TestResult {
    let dir = Directory::new()?;
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    let observed = control(&c, "agent_control_read", AgentControl::Revoke);
    answer(
        call(&mut r.calls).await,
        control_record("active", 1, "", ""),
    )?;
    let observed: AgentControlDto = serde_json::from_str(&observed.await??)?;
    let pause = control(&c, "agent_control_change", AgentControl::Pause);
    answer(
        call(&mut r.calls).await,
        control_record("active", 1, "", ""),
    )?;
    let sent = call(&mut r.calls).await;
    let operation = match &sent.op {
        AgentRequest::ControlReviewed { operation, .. } => operation.clone(),
        other => return Err(format!("unexpected {other:?}").into()),
    };
    // A reviewed Revoke overlaps B's already durable Pause. Neither an opposite
    // action nor a stale revision may borrow B's eventual Confirmed receipt.
    assert!(c
        .agent_control_change(target(&c), &observed.revision, AgentControl::Revoke)
        .await
        .is_err());
    assert!(c
        .agent_control_change(target(&c), "2", AgentControl::Pause)
        .await
        .is_err());
    assert!(r.calls.try_recv().is_err());
    sent.response
        .send(Ok(Reply::AgentControl(control_record(
            "paused",
            2,
            &operation,
            &target(&c).device,
        ))))
        .map_err(|_| "response dropped")?;
    let result: AgentControlDto = serde_json::from_str(&pause.await??)?;
    assert_eq!(result.outcome, crate::AgentControlOutcome::Confirmed);
    assert_eq!(result.status, AgentStatus::Paused);
    c.stop_sync();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_explicit_retry_reconciles_accepted_and_unsupported_preflight_never_sends(
) -> TestResult {
    let dir = Directory::new()?;
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    let initial = control(&c, "agent_control_change", AgentControl::Pause);
    answer(
        call(&mut r.calls).await,
        control_record("active", 1, "", ""),
    )?;
    let sent = call(&mut r.calls).await;
    let operation = match &sent.op {
        AgentRequest::ControlReviewed { operation, .. } => operation.clone(),
        other => return Err(format!("unexpected {other:?}").into()),
    };
    sent.response
        .send(Err("ACK lost".into()))
        .map_err(|_| "response dropped")?;
    answer(
        call(&mut r.calls).await,
        control_record("active", 1, "", ""),
    )?;
    assert!(serde_json::from_str::<AgentControlDto>(&initial.await??)?
        .pending
        .is_some());
    assert!(c
        .agent_control_retry(target(&c), "1", &"e".repeat(64), AgentControl::Pause)
        .await
        .is_err());
    let c2 = c.clone();
    let held_id = operation.clone();
    let unsupported = tokio::spawn(async move {
        c2.agent_control_retry(target(&c2), "1", &held_id, AgentControl::Pause)
            .await
    });
    let preflight = call(&mut r.calls).await;
    assert!(matches!(preflight.op, AgentRequest::ControlStatus { .. }));
    preflight
        .response
        .send(Ok(Reply::Done))
        .map_err(|_| "response dropped")?;
    assert!(unsupported.await?.is_err());
    assert!(r.calls.try_recv().is_err());
    let c2 = c.clone();
    let held_id = operation.clone();
    let accepted = tokio::spawn(async move {
        c2.agent_control_retry(target(&c2), "1", &held_id, AgentControl::Pause)
            .await
    });
    answer(
        call(&mut r.calls).await,
        control_record("paused", 2, &operation, &target(&c).device),
    )?;
    assert_eq!(
        accepted.await??.outcome,
        crate::AgentControlOutcome::Confirmed
    );
    assert!(r.calls.try_recv().is_err());
    c.stop_sync();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_cli_alias_repeated_pause_uses_a_fresh_confirmed_cas() -> TestResult {
    let dir = Directory::new()?;
    let mut r = relay().await;
    let (c, _) = owner(dir.path(), &r.url);
    online(&c).await;
    let c2 = c.clone();
    let agent = target(&c).agent;
    let task = tokio::spawn(async move { c2.agent_control(agent, "pause".into()).await });
    answer(
        call(&mut r.calls).await,
        control_record("paused", 1, "", ""),
    )?;
    answer(
        call(&mut r.calls).await,
        control_record("paused", 1, "", ""),
    )?;
    let sent = call(&mut r.calls).await;
    let operation = match &sent.op {
        AgentRequest::ControlReviewed {
            action: protocol::AgentAction::Pause,
            operation,
            revision: 1,
            ..
        } => operation.clone(),
        other => return Err(format!("unexpected {other:?}").into()),
    };
    sent.response
        .send(Ok(Reply::AgentControl(control_record(
            "paused",
            2,
            &operation,
            &target(&c).device,
        ))))
        .map_err(|_| "response dropped")?;
    task.await??;
    assert!(r.calls.try_recv().is_err());
    c.stop_sync();
    Ok(())
}

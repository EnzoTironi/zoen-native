//! The existing signed loopback fixture exercises a content-free service, even
//! when an active Agent has ordinary backlog and a genuinely approved source.
use super::*;
use crate::agent::controls;

fn content_stayed_idle(server: &super::relay::Relay, f: &Fixture) {
    let calls = server.calls.lock().unwrap();
    for forbidden in [
        "agent_advance",
        "forbidden_agent_operation",
        "read_receipt",
        "event_publish",
    ] {
        assert!(!calls.contains(&forbidden), "{forbidden}: {calls:?}");
    }
    assert!(server.cursors.lock().unwrap().is_empty());
    assert!(!f.path.join("agent-tool-steps.json").exists());
    assert!(!f.path.join("source-steps.json").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn active_controls_host_keeps_an_actual_approved_source_and_content_untouched(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let server = relay(Mode::Normal).await;
    let mut f = Fixture::new(&server.url)?;
    online(&f.client, true).await;
    f.client.stop_sync();
    // Reuse a static core fixture and its real signed owner approval. No editor
    // worker is constructed and no engine command is executed by this setup.
    let base = deck(&f);
    let request = stage_review(&f, &base.item_id, &f.source, true);
    f.approve(&request).await?;
    assert!(f.client.approved_tool_sources(&f.space).contains(&f.source));
    let before = f.client.timeline(f.space.clone())?;
    let before_request = f.client.requests();
    server.calls.lock().unwrap().clear();
    let lock = crate::agent_host_lock::HostLock::acquire(&f.path)?;
    let ready = startup::Readiness::reset(&f.path, &lock)?;

    tokio::time::timeout(
        Duration::from_secs(4),
        controls::host(&f.client, &ready, Some(1)),
    )
    .await??;

    assert!(f.path.join("agent-ready").exists());
    assert!(server.calls.lock().unwrap().contains(&"agent_state"));
    assert!(f.client.approved_tool_sources(&f.space).contains(&f.source));
    // The stopped and connected client may present delivery differently. Its
    // complete durable timeline and the pending approval must remain unchanged.
    let durable = |entries: Vec<TimelineEntry>| {
        entries
            .into_iter()
            .map(|entry| {
                let mut value = serde_json::to_value(entry).unwrap();
                value.as_object_mut().unwrap().remove("delivery");
                value
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        durable(f.client.timeline(f.space.clone())?),
        durable(before)
    );
    assert_eq!(
        serde_json::to_value(f.client.requests())?,
        serde_json::to_value(before_request)?
    );
    assert_eq!(
        f.client.app_binary_read(base.item_id, None, None)?.version,
        base.version
    );
    content_stayed_idle(&server, &f);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn paused_controls_host_is_ready_without_resuming_the_agent_or_consuming_backlog(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let server = relay(Mode::Paused).await;
    let f = Fixture::new(&server.url)?;
    online(&f.client, true).await;
    let lock = crate::agent_host_lock::HostLock::acquire(&f.path)?;
    let ready = startup::Readiness::reset(&f.path, &lock)?;
    server.calls.lock().unwrap().clear();

    tokio::time::timeout(
        Duration::from_secs(4),
        controls::host(&f.client, &ready, Some(1)),
    )
    .await??;

    assert!(f.path.join("agent-ready").exists());
    content_stayed_idle(&server, &f);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn controls_host_never_publishes_ready_or_reads_agent_state_before_sync_settles(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let server = relay(Mode::Unsynced).await;
    let f = Fixture::new(&server.url)?;
    online(&f.client, false).await;
    let lock = crate::agent_host_lock::HostLock::acquire(&f.path)?;
    let ready = startup::Readiness::reset(&f.path, &lock)?;
    server.calls.lock().unwrap().clear();
    let observe = async {
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(!f.path.join("agent-ready").exists());
        assert!(!server.calls.lock().unwrap().contains(&"agent_state"));
        content_stayed_idle(&server, &f);
        server.released.store(true, Ordering::SeqCst);
        server.release.notify_waiters();
    };
    let (result, ()) = tokio::join!(controls::host(&f.client, &ready, Some(1)), observe);
    result?;
    assert!(f.path.join("agent-ready").exists());
    content_stayed_idle(&server, &f);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revoked_or_unknown_state_refuses_before_ready_or_any_content_effect(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for mode in [Mode::Revoked, Mode::InvalidStatus, Mode::InvalidKind] {
        let server = relay(mode).await;
        let f = Fixture::new(&server.url)?;
        online(&f.client, true).await;
        let lock = crate::agent_host_lock::HostLock::acquire(&f.path)?;
        let ready = startup::Readiness::reset(&f.path, &lock)?;
        server.calls.lock().unwrap().clear();

        assert!(controls::host(&f.client, &ready, Some(1)).await.is_err());

        assert!(!f.path.join("agent-ready").exists());
        content_stayed_idle(&server, &f);
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_account_and_non_agent_owner_scope_never_publish_readiness(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let server = relay(Mode::Normal).await;
    let f = Fixture::new(&server.url)?;
    let lock = crate::agent_host_lock::HostLock::acquire(&f.path)?;
    let ready = startup::Readiness::reset(&f.path, &lock)?;
    let missing = Client::open(":memory:".into(), "en".into())?;
    assert!(controls::host(&missing, &ready, Some(1)).await.is_err());
    assert!(!f.path.join("agent-ready").exists());
    let person = Client::open(
        f.path.join("person.sqlite").to_string_lossy().into_owned(),
        "en".into(),
    )?;
    person.create_account(
        "Person".into(),
        "person_controls_fixture".into(),
        server.url.clone(),
        Arc::new(super::relay::Vault::default()),
    )?;
    online(&person, true).await;
    server.calls.lock().unwrap().clear();

    assert!(controls::host(&person, &ready, Some(1)).await.is_err());

    assert!(!f.path.join("agent-ready").exists());
    content_stayed_idle(&server, &f);
    person.stop_sync();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_direct_host_bounds_refuse_before_sync_or_any_worker_start(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let server = relay(Mode::Normal).await;
    let f = Fixture::new(&server.url)?;
    online(&f.client, true).await;
    let lock = crate::agent_host_lock::HostLock::acquire(&f.path)?;
    let ready = startup::Readiness::reset(&f.path, &lock)?;
    server.calls.lock().unwrap().clear();
    for bound in [0, u64::MAX] {
        assert!(controls::host(&f.client, &ready, Some(bound))
            .await
            .is_err());
        assert!(!f.path.join("agent-ready").exists());
        let calls = server.calls.lock().unwrap().clone();
        assert!(!calls.contains(&"sync"));
        assert!(!calls.contains(&"agent_state"));
        content_stayed_idle(&server, &f);
    }
    Ok(())
}

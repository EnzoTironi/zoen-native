//! Standard agent entry point owns the same durable executor and owner controls.
use super::*;
use std::time::{Duration, Instant};

pub(super) fn refuse_external(w: &World, path: &Path) -> TestResult {
    let refused = w
        .cmd("citizen", &["agent-host", "--for", "1"])
        .env("ZOEN_REPLY_HOST_BIN", env!("CARGO_BIN_EXE_zoen-reply-host"))
        .env("ZOEN_REPLY_HOST_CONFIG", path)
        .env("ZOEN_BRAIN", "legacy-provider-must-not-run")
        .output()?;
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("agent_host_managed_executor_required")
    );
    assert!(!w.dir.join("citizen/agent-ready").exists());
    Ok(())
}

pub(super) async fn run(
    w: &World,
    path: &Path,
    agent: &str,
    space: &str,
    code: &device_link::LinkCode,
    provider: &provider::Provider,
) -> TestResult {
    // A source selector cannot switch this configured account to legacy tools.
    let refused = w
        .cmd(
            "citizen",
            &["agent-host", "--source", "original", "--for", "1"],
        )
        .env("ZOEN_REPLY_HOST_BIN", env!("CARGO_BIN_EXE_zoen-reply-host"))
        .env("ZOEN_REPLY_HOST_CONFIG", path)
        .output()?;
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr)
        .contains("agent_host_source_unavailable_in_projection"));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
    let mut host = owner_budget::Companion::start_host(w, path)?;
    owner_budget::set(w, &mut host, agent, space, true).await?;
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 5);

    let owner = Client::open(
        w.dir
            .join("owner/zoen.sqlite")
            .to_string_lossy()
            .into_owned(),
        "en".into(),
    )?;
    assert!(owner.unlock(Arc::new(file_vault::FileVault {
        dir: w.dir.join("owner/vault"),
    }))?);
    owner.start_sync(None)?;
    owner.wait_until_settled(20_000).await?;
    owner.send_message(
        space.into(),
        "Can you answer with the client closed?".into(),
    )?;
    owner.wait_until_settled(20_000).await?;
    owner.stop_sync();

    // No manual discover/advance and no owner connection during model execution.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        host.check()?;
        if w.scalar("SELECT sum(spent_units)::text FROM runtime_periods")
            .await
            == "24"
        {
            break;
        }
        if Instant::now() >= deadline {
            return Err("standard host did not settle its fourth original".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    owner.start_sync(None)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        host.check()?;
        let answers = owner
            .timeline(space.into())?
            .iter()
            .filter(|entry| {
                matches!(&entry.kind, client_core::EntryKind::Message { text, .. }
                if text == provider::ANSWER)
            })
            .count();
        if answers == 4 {
            break;
        }
        if Instant::now() >= deadline {
            return Err("standard host reply did not reach owner MLS".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    owner.stop_sync();
    // Core owns an exclusive SQLite connection. Retire this reader before the
    // cold quote opens the same account through its normal client lifecycle.
    drop(owner);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 4);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 4);
    assert_eq!(
        w.scalar("SELECT sum(held_units)::text FROM runtime_periods")
            .await,
        "0"
    );
    assert!(!w.dir.join("citizen/agent-tool-steps.json").exists());
    drop(host);
    managed_startup::retired(w, path, agent, code).await?;

    // Cold standard host and a fresh signed budget quote recover the same state.
    let mut host = owner_budget::Companion::start_host(w, path)?;
    owner_budget::observe_enabled(w, &mut host, agent, space).await?;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 4);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 4);
    assert_eq!(
        w.scalar("SELECT sum(spent_units)::text FROM runtime_periods")
            .await,
        "24"
    );
    drop(host);
    managed_startup::retired(w, path, agent, code).await?;
    println!("standard agent-host: managed enrollment, canonical budget, automatic fourth reply at owner MLS, cold controls and no legacy provider/duplicate call/charge/output PASS");
    Ok(())
}

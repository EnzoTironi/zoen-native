//! The standard configured host serves one selected message through its own leaf.
use super::*;
use std::{
    process::{Child, Stdio},
    time::{Duration, Instant},
};
struct Process {
    child: Child,
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
async fn selected(w: &World, path: &Path, source: &str, log_name: &str) -> TestResult {
    let log = w.dir.join(log_name);
    let output = std::fs::File::create(&log)?;
    let child = w
        .cmd(
            "citizen",
            &["agent-host", "--source", source, "--for", "120"],
        )
        .env("ZOEN_REPLY_HOST_BIN", env!("CARGO_BIN_EXE_zoen-reply-host"))
        .env("ZOEN_REPLY_HOST_CONFIG", path)
        .env("ZOEN_BRAIN", "legacy-provider-must-not-run")
        .env("ZOEN_AGENT_GATE", "legacy-gate-must-not-run")
        .stdout(Stdio::from(output.try_clone()?))
        .stderr(Stdio::from(output))
        .spawn()?;
    let mut process = Process { child };
    let deadline = Instant::now() + Duration::from_secs(150);
    loop {
        if let Some(status) = process.child.try_wait()? {
            let log = std::fs::read_to_string(log)?;
            assert!(status.success(), "selected source process: {log}");
            assert!(log.contains("agent_host_source_reply_completed"), "{log}");
            break;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "selected source did not complete: {}",
                std::fs::read_to_string(log)?
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}
pub(super) async fn run(
    w: &World,
    path: &Path,
    space: &str,
    code: &device_link::LinkCode,
    agent: &str,
    provider: &provider::Provider,
) -> TestResult {
    let owner = Client::open(
        w.dir
            .join("owner/zoen.sqlite")
            .to_string_lossy()
            .into_owned(),
        "en".into(),
    )?;
    assert!(owner.unlock(Arc::new(file_vault::FileVault {
        dir: w.dir.join("owner/vault")
    }))?);
    owner.start_sync(None)?;
    owner.wait_until_settled(20_000).await?;
    owner.send_message(space.into(), "Older work must wait".into())?;
    let chosen = owner.send_message(space.into(), "Answer only this selected question".into())?;
    owner.send_message(space.into(), "Later work must wait".into())?;
    owner.wait_until_settled(20_000).await?;
    owner.stop_sync();
    let before = w.scalar("SELECT coalesce(jsonb_agg(to_jsonb(c) ORDER BY agent,space),'[]'::jsonb)::text FROM agent_space_cursors c").await;
    // The actual owner connection is closed during execution.
    drop(owner);
    selected(w, path, &chosen.id, "reply-selected-first.log").await?;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 5);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 5);
    assert_eq!(
        w.scalar("SELECT sum(spent_units)::text FROM runtime_periods")
            .await,
        "30"
    );
    assert_eq!(
        w.scalar("SELECT sum(held_units)::text FROM runtime_periods")
            .await,
        "0"
    );
    managed_startup::retired(w, path, agent, code).await?;
    selected(w, path, &chosen.id, "reply-selected-cold.log").await?;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 5);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 5);
    assert_eq!(w.scalar("SELECT coalesce(jsonb_agg(to_jsonb(c) ORDER BY agent,space),'[]'::jsonb)::text FROM agent_space_cursors c").await, before);
    let owner = Client::open(
        w.dir
            .join("owner/zoen.sqlite")
            .to_string_lossy()
            .into_owned(),
        "en".into(),
    )?;
    assert!(owner.unlock(Arc::new(file_vault::FileVault {
        dir: w.dir.join("owner/vault")
    }))?);
    owner.start_sync(None)?;
    owner.wait_until_settled(20_000).await?;
    let replies = owner
        .timeline(space.into())?
        .into_iter()
        .filter(|entry| {
            entry.author.id == agent
                && entry
                    .reply_to
                    .as_ref()
                    .is_some_and(|reply| reply.id == chosen.id)
        })
        .count();
    assert_eq!(replies, 1);
    owner.stop_sync();
    drop(owner);
    managed_startup::retired(w, path, agent, code).await?;
    println!("standard selected source: actual configured executor, closed owner, one accepted MLS reply, cold reuse, no unrelated inference or cursor changes PASS");
    Ok(())
}

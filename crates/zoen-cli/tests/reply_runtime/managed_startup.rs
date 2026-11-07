//! A killed control service cannot leave a detached native custody owner.
use super::*;
use std::{
    process::{Child, Stdio},
    time::{Duration, Instant},
};

struct Parent(Child);
impl Drop for Parent {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
pub(super) async fn killed_parent(
    w: &World,
    path: &Path,
    agent: &str,
    code: &device_link::LinkCode,
) -> TestResult {
    let log = w.dir.join("managed-parent.log");
    let output = std::fs::File::create(&log)?;
    let mut parent = Parent(
        w.cmd("citizen", &["agent-host", "--for", "180"])
            .env("ZOEN_REPLY_HOST_BIN", env!("CARGO_BIN_EXE_zoen-reply-host"))
            .env("ZOEN_REPLY_HOST_CONFIG", path)
            .env_remove("ZOEN_IMAGE_HOST_BIN")
            .env_remove("ZOEN_IMAGE_HOST_CONFIG")
            .env("ZOEN_BRAIN", "legacy-provider-must-not-run")
            .env("ZOEN_AGENT_GATE", "legacy-gate-must-not-run")
            .stdout(Stdio::from(output.try_clone()?))
            .stderr(Stdio::from(output))
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    while !w.dir.join("citizen/agent-ready").exists() {
        if parent.0.try_wait()?.is_some() || Instant::now() >= deadline {
            return Err(format!(
                "managed parent did not become ready: {}",
                std::fs::read_to_string(&log)?
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    parent.0.kill()?;
    parent.0.wait()?;
    retired(w, path, agent, code).await
}

pub(super) async fn retired(
    w: &World,
    path: &Path,
    agent: &str,
    code: &device_link::LinkCode,
) -> TestResult {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        // This action must acquire the SAME home lock held throughout custody
        // lifetime. Success demonstrates retirement, not just a missing marker.
        if let Ok(prepared) = call(
            path,
            "prepare-managed",
            Some(json!({"agent":agent,"relay_url":w.relay_url()})),
        ) {
            assert_eq!(prepared["code"], code.encode());
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("native executor outlived its killed supervisor".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

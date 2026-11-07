//! Real service process, owner Core consent, dedicated native leaf and cold reply.
use super::*;
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub(super) struct ServiceProcess {
    pub(super) child: Child,
    pub(super) log: std::path::PathBuf,
}
impl Drop for ServiceProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl ServiceProcess {
    pub(super) fn start(w: &World, path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let log = w.dir.join(types::new_id("managed-service-log")?);
        let output = std::fs::File::create(&log)?;
        let child = Command::new(env!("CARGO_BIN_EXE_zoen-reply-host"))
            .arg("--config")
            .arg(path)
            .arg("manage")
            .stdout(Stdio::from(output.try_clone()?))
            .stderr(Stdio::from(output))
            .spawn()?;
        Ok(Self { child, log })
    }
    #[cfg(unix)]
    pub(super) fn signal(&self, signal: &str) -> Result<(), Box<dyn std::error::Error>> {
        if !Command::new("kill")
            .args([signal, &self.child.id().to_string()])
            .status()?
            .success()
        {
            return Err("owned service signal failed".into());
        }
        Ok(())
    }
    pub(super) async fn frame(
        &mut self,
        status: &str,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        self.frame_for(status, Duration::from_secs(20)).await
    }
    pub(super) async fn frame_for(
        &mut self,
        status: &str,
        budget: Duration,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        self.observe(status, budget, false, 0).await
    }
    // A paid synthetic probe stops at the first policy hold/review rather than
    // waiting through another worker cycle and inferring a new decision.
    pub(super) async fn live_output(
        &mut self,
        budget: Duration,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        self.observe("output", budget, true, 0).await
    }
    pub(super) fn observation_point(&self) -> Result<usize, Box<dyn std::error::Error>> {
        Ok(std::fs::read_to_string(&self.log)?.lines().count())
    }
    pub(super) async fn frame_after(
        &mut self,
        status: &str,
        after: usize,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        self.observe(status, Duration::from_secs(20), true, after)
            .await
    }
    async fn observe(
        &mut self,
        status: &str,
        budget: Duration,
        stop_on_hold: bool,
        after: usize,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let until = Instant::now() + budget;
        loop {
            let body = std::fs::read_to_string(&self.log)?;
            for frame in body
                .lines()
                .skip(after)
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            {
                if frame["status"] == status {
                    return Ok(frame);
                }
                if stop_on_hold
                    && matches!(
                        frame["status"].as_str(),
                        Some(
                            "held"
                                | "review"
                                | "recovery"
                                | "deferred"
                                | "worker_stopped"
                                | "enrollment_deferred"
                        )
                    )
                {
                    return Err(format!("live probe stopped at {}", frame["status"]).into());
                }
            }
            if let Some(exit) = self.child.try_wait()? {
                return Err(format!("service exited {exit}: {body}").into());
            }
            if Instant::now() >= until {
                return Err(format!("service missing {status}: {body}").into());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_probe_stops_before_a_later_success_after_policy_hold(
) -> Result<(), Box<dyn std::error::Error>> {
    for status in [
        "held",
        "review",
        "recovery",
        "deferred",
        "worker_stopped",
        "enrollment_deferred",
    ] {
        let log = std::env::temp_dir().join(types::new_id("live-observer")?);
        std::fs::write(
            &log,
            format!("{{\"status\":\"{status}\"}}\n{{\"status\":\"output\"}}\n"),
        )?;
        let child = Command::new("sleep").arg("10").spawn()?;
        let mut service = ServiceProcess {
            child,
            log: log.clone(),
        };
        assert!(service
            .live_output(Duration::from_millis(100))
            .await
            .is_err());
        // Ordinary fixture observers retain their existing behavior.
        assert_eq!(service.frame("output").await?["status"], "output");
        drop(service);
        std::fs::remove_file(log)?;
    }
    Ok(())
}

pub(super) fn open_owner(w: &World) -> Result<Arc<Client>, Box<dyn std::error::Error>> {
    let client = Client::open(
        w.dir.join("owner/zoen.sqlite").to_string_lossy().into(),
        "en".into(),
    )?;
    if !client.unlock(Arc::new(file_vault::FileVault {
        dir: w.dir.join("owner/vault"),
    }))? {
        return Err("owner did not unlock".into());
    }
    Ok(client)
}

pub(super) async fn bridge(
    owner: &Client,
    op: &str,
    mut input: Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let account = owner.account().ok_or("account")?;
    input["op"] = op.into();
    input["expected_identity_id"] = account.identity_id.into();
    input["expected_device_id"] = account.device_id.into();
    Ok(serde_json::from_str(
        &owner.client_request(input.to_string()).await?,
    )?)
}

// The log also receives legitimate key/device synchronization events. Compare
// actual trust grant hashes, not the unrelated total event count.
pub(super) fn trust_grants(
    owner: &Client,
    space: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(owner
        .log_events(space.into())?
        .into_iter()
        .filter(|event| event.label.starts_with("Grant: trust "))
        .map(|event| event.hash)
        .collect())
}

pub(super) async fn status(
    owner: &Client,
    process: &mut ServiceProcess,
    wanted: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let record = bridge(owner, "managed_agent_status", json!({})).await?;
        if record["state"] == wanted {
            return Ok(record);
        }
        if let Some(exit) = process.child.try_wait()? {
            return Err(format!(
                "service stopped {exit}: {}",
                std::fs::read_to_string(&process.log)?
            )
            .into());
        }
        if Instant::now() >= until {
            return Err(format!(
                "enrollment missing {wanted}: {}",
                std::fs::read_to_string(&process.log)?
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The host reports locators, not plaintext. Validate the accepted client ID and
/// actual authenticated text through the cold owner's native projection.
pub(super) async fn assert_recovered_reply(
    owner: &Client,
    space: &str,
    agent: &str,
    output: &Value,
    expected: &str,
) -> TestResult {
    let accepted = output["client_id"].as_str().ok_or("accepted client ID")?;
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let history = owner.timeline(space.into())?;
        let replies: Vec<_> = history
            .iter()
            .filter(|entry| {
                entry.author.id == agent
                    && matches!(&entry.kind, client_core::EntryKind::Message { .. })
            })
            .collect();
        if !replies.is_empty() {
            assert_eq!(replies.len(), 1);
            assert_eq!(replies[0].id, accepted);
            assert!(matches!(&replies[0].kind,
                client_core::EntryKind::Message {text,..}
                    if text.to_ascii_lowercase().contains(&expected.to_ascii_lowercase())));
            return Ok(());
        }
        if Instant::now() >= until {
            return Err("cold owner did not recover the accepted encrypted reply".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

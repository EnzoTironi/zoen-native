//! Real autonomous process: source discovery, notifications and cold backlog.
use super::*;
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Worker {
    child: Child,
    log: std::path::PathBuf,
    subject: String,
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Worker {
    async fn start(w: &World, path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let log = w.dir.join(types::new_id("worker-log")?);
        let output = std::fs::File::create(&log)?;
        let child = Command::new(env!("CARGO_BIN_EXE_zoen-reply-host"))
            .arg("--config")
            .arg(path)
            .arg("watch")
            .stdout(Stdio::from(output.try_clone()?))
            .stderr(Stdio::from(output))
            .spawn()?;
        let mut worker = Self {
            child,
            log,
            subject: String::new(),
        };
        let ready = worker.wait("worker_ready").await?;
        assert_eq!(ready["notifications"], true);
        worker.subject = ready["subject"].as_str().ok_or("worker subject")?.into();
        Ok(worker)
    }
    async fn wait(&mut self, status: &str) -> Result<Value, Box<dyn std::error::Error>> {
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            let body = std::fs::read_to_string(&self.log)?;
            if let Some(frame) = body
                .lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .find(|value| value["status"] == status)
            {
                return Ok(frame);
            }
            if let Some(exit) = self.child.try_wait()? {
                return Err(format!("worker exited {exit}: {body}").into());
            }
            if Instant::now() >= until {
                return Err(format!("worker missing {status}: {body}").into());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
async fn message(w: &World, agent: &str, space: &str) -> TestResult {
    let owner = Client::open(
        w.dir
            .join("owner")
            .join("zoen.sqlite")
            .to_string_lossy()
            .into_owned(),
        "en".into(),
    )?;
    assert!(owner.unlock(Arc::new(file_vault::FileVault {
        dir: w.dir.join("owner").join("vault")
    }))?);
    owner.start_sync(None)?;
    owner.wait_until_settled(20_000).await?;
    owner.set_trust(agent.into(), space.into(), TrustLevelDto::Listen)?;
    owner.send_message(
        space.into(),
        "Can you answer with the client closed?".into(),
    )?;
    owner.wait_until_settled(20_000).await?;
    owner.stop_sync();
    drop(owner);
    Ok(())
}
pub(super) async fn run(
    w: &World,
    path: &Path,
    agent: &str,
    space: &str,
    provider: &provider::Provider,
) -> TestResult {
    // A new signed source arrives while no worker/client remains open.
    message(w, agent, space).await?;
    let mut worker = Worker::start(w, path).await?;
    let output = worker.wait("output").await?;
    assert_eq!(output["delivery"]["status"], "accepted");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    drop(worker);
    // Recover a later source after an actual process termination, using only the
    // durable source/native wake queue. No discover/advance command is supplied.
    message(w, agent, space).await?;
    let mut worker = Worker::start(w, path).await?;
    let output = worker.wait("output").await?;
    assert_eq!(output["delivery"]["status"], "accepted");
    let bus = async_nats::connect(std::env::var("ZOEN_NATS_URL")?).await?;
    for _ in 0..128 {
        bus.publish(
            worker.subject.clone(),
            r#"{"approved":true,"command":"advance","run":"forged"}"#.into(),
        )
        .await?;
    }
    bus.flush().await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 3);
    assert_eq!(
        w.scalar("SELECT sum(spent_units)::text FROM runtime_periods")
            .await,
        "18"
    );
    assert_eq!(
        w.scalar("SELECT sum(held_units)::text FROM runtime_periods")
            .await,
        "0"
    );
    #[cfg(unix)]
    {
        assert!(Command::new("kill")
            .arg("-TERM")
            .arg(worker.child.id().to_string())
            .status()?
            .success());
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = worker.child.try_wait()? {
                assert!(status.success());
                break;
            }
            if Instant::now() >= deadline {
                return Err("worker did not drain after SIGTERM".into());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    drop(worker);
    let history = w.zoen("owner", &["read", "@reply_citizen"]);
    assert_eq!(history.matches(provider::ANSWER).count(), 3);
    println!("automatic worker: canonical membership discovery, closed clients, actual process termination/cold backlog, three originals and no duplicate call/charge/response PASS");
    Ok(())
}

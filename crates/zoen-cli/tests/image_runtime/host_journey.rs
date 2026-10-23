//! Actual normal citizen ToolHost -> private IPC -> production executable.
#[path = "owner_controls.rs"]
mod owner_controls;
use super::*;
use client_core::{AgentRequestDto, RequestStatus, ToolRequestDecision};
use event_log::{Author, Signer};
use sha2::{Digest, Sha256};
use std::{
    process::{Child, Stdio},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
#[path = "provider.rs"]
mod provider;
pub(super) use provider::Provider;

struct Host {
    child: Child,
    log: std::path::PathBuf,
}
impl Host {
    async fn start(w: &World, config: &Path) -> Self {
        let script = w.dir.join("image-script.json");
        if !script.exists() {
            write_json(
                &script,
                &serde_json::json!({"Create the image through the tool": format!("I will prepare the image for your review.\n```tools\n[{{\"name\":\"generate_image\",\"arguments\":{{\"prompt\":\"{}\"}}}}]\n```",provider::PROMPT)}),
            );
        }
        let ready = w.dir.join("citizen").join("agent-ready");
        match std::fs::remove_file(&ready) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("ready cleanup: {e}"),
        }
        let log = w.dir.join(types::new_id("image-agent-log"));
        let output = std::fs::File::create(&log).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_zoen"))
            .arg("--home")
            .arg(w.dir.join("citizen"))
            .args(["agent-host", "--for", "240"])
            .env("ZOEN_AGENT_SCRIPT", &script)
            .env("ZOEN_AGENT_GATE", "offline")
            .env("ZOEN_IMAGE_HOST_BIN", env!("CARGO_BIN_EXE_zoen-image-host"))
            .env("ZOEN_IMAGE_HOST_CONFIG", config)
            .stdout(Stdio::from(output.try_clone().unwrap()))
            .stderr(Stdio::from(output))
            .spawn()
            .unwrap();
        let mut host = Self { child, log };
        let deadline = Instant::now() + Duration::from_secs(30);
        while !ready.exists() {
            host.alive();
            assert!(
                Instant::now() < deadline,
                "host never ready: {}",
                host.output()
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        host
    }
    fn output(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap()
    }
    fn alive(&mut self) {
        assert!(
            self.child.try_wait().unwrap().is_none(),
            "host exited: {}",
            self.output()
        );
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn owner_client(w: &World) -> Arc<Client> {
    let home = w.dir.join("owner");
    let client = Client::open(
        home.join("zoen.sqlite").to_string_lossy().into_owned(),
        "en".into(),
    )
    .unwrap();
    assert!(client
        .unlock(Arc::new(file_vault::FileVault {
            dir: home.join("vault")
        }))
        .unwrap());
    client.start_sync(None).unwrap();
    client
}
pub(super) async fn install_policy(w: &World, config: &config::Config, runtime: &RuntimeAuthority) {
    // These are only this World's generated keys. Use the same actual owner
    // device/certificate; no SQL policy/financial authority rows are inserted.
    let vault = file_vault::FileVault {
        dir: w.dir.join("owner").join("vault"),
    };
    let key = |name: &str| -> Signer {
        Signer::from_secret(&vault.load(name.into()).unwrap().try_into().unwrap())
    };
    let author = Author::device(&key("zoen.identity.v1"), key("zoen.device.v1"));
    let date = w
        .scalar("SELECT to_char(now() AT TIME ZONE 'UTC','YYYY-MM')")
        .await;
    let (year, month) = date.split_once('-').unwrap();
    let end=w.scalar("SELECT ((extract(epoch FROM (date_trunc('month',now())+interval '1 month'))*1000)::bigint)::text").await.parse().unwrap();
    // This fixture also exercises general reply discovery to prove the two
    // MLS leaves opened the same granted source. Authorize both exact profiles
    // explicitly; the Images-only production path does not need chat discovery.
    let mut profiles = vec![
        config.image_price.digest().unwrap(),
        config.chat_price.digest().unwrap(),
    ];
    profiles.sort();
    let policy = zoen_runtime::OwnerPeriodPolicy {
        owner: author.identity.clone(),
        year: year.parse().unwrap(),
        month: month.parse().unwrap(),
        version: 1,
        previous_digest: None,
        currency: "USD".into(),
        scale: 6,
        limit_units: 100,
        max_attempt_units: 10,
        expires_at_ms: end,
        enabled: true,
        allowed_profiles: profiles,
    };
    let signed = event_log::owner_budget::sign(&author, policy).unwrap();
    let mut tampered = signed.clone();
    tampered.policy.limit_units += 1;
    assert_eq!(
        runtime.install_policy(tampered).await.unwrap_err(),
        zoen_runtime::RuntimeError::InvalidPolicy
    );
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 0);
    runtime.install_policy(signed).await.unwrap();
}
async fn review(owner: &Client, space: &str, source: &str, host: &mut Host) -> AgentRequestDto {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        host.alive();
        if let Some(request) = owner.requests().into_iter().find(|r| {
            r.space_id == space
                && r.status == RequestStatus::Pending
                && r.tool_approval
                    .as_ref()
                    .is_some_and(|a| a.image.as_ref().is_some_and(|i| i.source_id == source))
        }) {
            return request;
        }
        assert!(
            Instant::now() < deadline,
            "no real image review: {}",
            host.output()
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
async fn delivered(w: &World, source: &str, host: &mut Host) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(90);
    let path = w.dir.join("citizen").join("agent-tool-steps.json");
    loop {
        host.alive();
        if let Ok(bytes) = std::fs::read(&path) {
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let entries = value
                .as_array()
                .or_else(|| value.get("steps").and_then(serde_json::Value::as_array))
                .unwrap();
            if let Some(steps) = entries
                .iter()
                .find(|entry| entry[0] == source)
                .and_then(|entry| entry[1].as_array())
            {
                for step in steps {
                    if step["call"]["name"] == "generate_image" {
                        let progress: serde_json::Value =
                            serde_json::from_str(step["result"].as_str().unwrap()).unwrap();
                        if progress["progress"]["status"] == "delivered" {
                            return progress["progress"].clone();
                        }
                    }
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "image did not deliver: {}",
            host.output()
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
pub(super) async fn run(
    w: &World,
    path: &Path,
    config: &config::Config,
    runtime: RuntimeAuthority,
    space: &str,
    provider: &Provider,
) {
    let runtime = runtime
        .open_managed_image_continuity(&config.continuity, &config.agent, &config.device)
        .await
        .unwrap()
        .qualify_managed_images_output(config.host().unwrap().target)
        .await
        .unwrap();
    assert_eq!(
        runtime
            .prepare_reply_output("unqualified-reply")
            .await
            .unwrap_err(),
        zoen_runtime::RuntimeError::CoreAuthorityUnavailable,
        "Images qualification must not open general reply output"
    );
    drop(runtime);
    let owner = owner_client(w);
    owner.wait_until_settled(20_000).await.unwrap();
    owner
        .set_trust(config.agent.clone(), space.into(), TrustLevelDto::Act)
        .unwrap();
    owner.wait_until_settled(20_000).await.unwrap();
    let mut host = Host::start(w, path).await;
    let source = owner
        .send_message(space.into(), "Create the image through the tool".into())
        .unwrap()
        .id;
    owner.wait_until_settled(20_000).await.unwrap();
    let request = review(&owner, space, &source, &mut host).await;
    assert_eq!(
        provider.sends.load(Ordering::SeqCst),
        0,
        "provider reached before owner review"
    );
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    owner_controls::run(w, &owner, config, &request, &mut host, provider).await;
    // Stop the actual ordinary host before the human's decision; the new host
    // must recover the same scheduling/review through its existing journal.
    drop(host);
    let observed = &request.tool_approval.as_ref().unwrap().content_hash;
    assert!(owner
        .decide_tool_request(
            request.id.clone(),
            "0".repeat(64),
            ToolRequestDecision::ApproveOnce
        )
        .is_err());
    assert_eq!(
        owner
            .requests()
            .into_iter()
            .find(|r| r.id == request.id)
            .unwrap()
            .status,
        RequestStatus::Pending
    );
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    owner
        .decide_tool_request(
            request.id.clone(),
            observed.clone(),
            ToolRequestDecision::ApproveOnce,
        )
        .unwrap();
    owner.wait_until_settled(20_000).await.unwrap();
    let mut restarted = Host::start(w, path).await;
    let first = delivered(w, &source, &mut restarted).await;
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    let item = first["item"].as_str().unwrap().to_owned();
    let sha = hex::encode(Sha256::digest(provider::PNG));
    assert_eq!(first["sha256"], sha);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if owner.items().iter().any(|i| i.id == item) {
            break;
        }
        restarted.alive();
        assert!(
            Instant::now() < deadline,
            "accepted FileDoc absent from owner's normal projection"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let info = owner.file_info(item.clone()).unwrap().unwrap();
    assert_eq!(info.sha256, sha);
    assert!(
        owner
            .ensure_file_version(item.clone(), 1, 1_000_000, 20_000)
            .await
            .unwrap()
            .ready
    );
    assert_eq!(
        owner.file_bytes(item.clone(), None).unwrap().unwrap(),
        provider::PNG
    );
    drop(restarted);
    let mut cold = Host::start(w, path).await;
    let recovered = delivered(w, &source, &mut cold).await;
    assert_eq!(
        recovered, first,
        "restart changed original run/output receipt"
    );
    tokio::time::sleep(Duration::from_secs(2)).await;
    cold.alive();
    drop(cold);
    // The ordinary host correctly skips terminal scheduling. Also reopen the
    // actual Runtime executable: this must recover its accepted original, not
    // merely return the ordinary host's cached scheduling projection.
    use tokio::io::AsyncWriteExt;
    let mut process = tokio::process::Command::new(env!("CARGO_BIN_EXE_zoen-image-host"))
        .arg("--config")
        .arg(path)
        .arg("execute")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let bytes = serde_json::to_vec(&serde_json::json!({"command":"advance","agent":config.agent,"space":space,"request":request.id})).unwrap();
    let mut stdin = process.stdin.take().unwrap();
    stdin.write_all(&bytes).await.unwrap();
    drop(stdin);
    let output = tokio::time::timeout(Duration::from_secs(150), process.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "cold Runtime advance failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        actual, first,
        "cold Runtime changed the retained output receipt"
    );
    assert_eq!(
        provider.sends.load(Ordering::SeqCst),
        1,
        "restart sent the provider again"
    );
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 1);
    assert_eq!(w.count("SELECT count(*) FROM runtime_claims").await, 1);
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_dispositions WHERE kind='settle'")
            .await,
        1
    );
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_periods WHERE held_units=0 AND spent_units=1")
            .await,
        1
    );
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_dispositions WHERE kind='settle' AND amount_units=1")
            .await,
        1
    );
    let files: Vec<_> = owner
        .items()
        .into_iter()
        .filter(|i| i.space_id == space && i.file.as_ref().is_some_and(|file| file.sha256 == sha))
        .collect();
    assert_eq!(
        files.len(),
        1,
        "one capture must not create duplicate FileDocs"
    );
    assert_eq!(files[0].id, item);
    assert_eq!(
        owner
            .requests()
            .iter()
            .filter(|r| r.id == request.id)
            .count(),
        1
    );
    owner.stop_sync();
    println!("image product: actual ToolHost IPC review -> owner ApproveOnce -> cold restart -> one provider/charge/accepted FileDoc -> same output PASS");
}

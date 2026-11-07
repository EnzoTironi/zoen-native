//! Real service process, owner Core consent, dedicated native leaf and cold reply.
use super::*;
use protocol::managed_agents::Record;
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use zoen_runtime::RuntimeAuthority;

struct ServiceProcess {
    child: Child,
    log: std::path::PathBuf,
}
impl Drop for ServiceProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl ServiceProcess {
    fn start(w: &World, path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
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
    async fn frame(&mut self, status: &str) -> Result<Value, Box<dyn std::error::Error>> {
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            let body = std::fs::read_to_string(&self.log)?;
            if let Some(frame) = body
                .lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .find(|frame| frame["status"] == status)
            {
                return Ok(frame);
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

fn open_owner(w: &World) -> Result<Arc<Client>, Box<dyn std::error::Error>> {
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

async fn status(
    owner: &Client,
    process: &mut ServiceProcess,
    wanted: &str,
) -> Result<Record, Box<dyn std::error::Error>> {
    let account = owner.account().ok_or("account")?;
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let record = owner
            .managed_enrollment_status(&account.identity_id, &account.device_id)
            .await?;
        if record.state == wanted {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn managed_service_owner_consent_first_reply_and_cold_recovery() -> TestResult {
    let mut w = World::new_with("managed_service", None, &[]).await;
    let result: TestResult = async {
        w.init("owner", "Owner");
        let directory = w.dir.join("operator");
        std::fs::create_dir(&directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        }
        let registry = directory.join("managed.json");
        write_json(&registry, &json!({"live":true,"devices":[]}))?;
        w.stop_relay();
        w.configure_relay_env("ZOEN_MANAGED_DEVICE_CONFIG", registry.to_str());
        w.start_relay();
        let provider = provider::Provider::start().await?;
        let definition = zoen_agentd::Definition {
            version: 1,
            kind: zoen_agentd::AgentKind::User,
            persona: "Answer the owner".into(),
            rules: vec!["Ask before effects.".into()],
            state: json!({}),
            model_tier: zoen_agentd::ModelTier::Cheap,
        }
        .encode()?;
        let mut value = configuration(&w, "", &provider.endpoint);
        value["admission_config"] = registry.to_string_lossy().to_string().into();
        value["managed_service"] = json!({"id":"11".repeat(32),
            "definition":String::from_utf8(definition)?, "zoen_binary":env!("CARGO_BIN_EXE_zoen")});
        let config: config::Config = serde_json::from_value(value.clone())?;
        std::fs::create_dir(&config.home)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&config.home, std::fs::Permissions::from_mode(0o700))?;
        }
        let vault = Arc::new(file_vault::FileVault {
            dir: config.home.join("vault"),
        });
        for (key, bytes) in [
            ("pg".to_string(), w.db_url.as_bytes().to_vec()),
            ("evidence".into(), vec![73; 32]),
            ("chat".into(), b"fixture-chat".to_vec()),
            (
                RuntimeAuthority::native_capsule_vault_key(&config.namespace, 1)?,
                vec![74; 32],
            ),
        ] {
            if !vault.save(key, bytes) {
                return Err("service fixture key save".into());
            }
        }
        let path = w.dir.join("managed-service.json");
        write_json(&path, &value)?;
        assert_eq!(
            call(&path, "initialize-service", None)?["status"],
            "service_initialized"
        );
        let mut process = ServiceProcess::start(&w, &path)?;
        process.frame("service_ready").await?;
        let owner = open_owner(&w)?;
        owner.start_sync(None)?;
        owner.wait_until_settled(20_000).await?;
        let account = owner.account().ok_or("account")?;
        let available = owner
            .managed_agent_service(&account.identity_id, &account.device_id)
            .await?
            .ok_or("service not available")?;
        let record = owner
            .prepare_managed_enrollment(
                &available,
                "Helpful agent",
                "managed_helper",
                &account.identity_id,
                &account.device_id,
            )
            .await?;
        let request = record.request.ok_or("request")?;
        let offered = status(&owner, &mut process, "offered").await?;
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
        let review = owner.managed_enrollment_preview(
            &request,
            &offered,
            &account.identity_id,
            &account.device_id,
        )?;
        let approved = owner
            .approve_managed_enrollment(
                &request,
                &offered,
                &review.offer_digest,
                &account.identity_id,
                &account.device_id,
            )
            .await?;
        assert_eq!(
            approved.outcome,
            client_core::AgentEnrollmentDeliveryOutcome::Sent
        );
        let installed = status(&owner, &mut process, "registered").await?;
        assert_eq!(installed.agent, offered.agent);
        let agent = installed.agent;
        let executor = config
            .home
            .join(&account.identity_id)
            .join(&request.operation)
            .join("executor/reply-config.json");
        let signed = signed_policy(
            &w,
            "owner",
            &account.identity_id,
            config.chat_price.digest()?,
        )
        .await?;
        assert_eq!(
            call(
                &executor,
                "control",
                Some(json!({"command":"policy_import","signed":signed}))
            )?["status"],
            "policy_installed"
        );
        let people = owner.find_people(request.handle.clone()).await?;
        assert!(people.iter().any(|person| person.id == agent));
        let space = owner.start_direct(agent.clone())?;
        owner.wait_until_settled(20_000).await?;
        owner.set_trust(agent.clone(), space.clone(), TrustLevelDto::Listen)?;
        owner.send_message(space, "Can you answer with the client closed?".into())?;
        owner.wait_until_settled(20_000).await?;
        owner.stop_sync();
        drop(owner);
        let output = process.frame("output").await?;
        assert_eq!(output["delivery"]["status"], "accepted");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 1);
        assert_eq!(
            w.scalar("SELECT sum(spent_units)::text FROM runtime_periods")
                .await,
            "6"
        );
        drop(process);
        let mut restarted = ServiceProcess::start(&w, &path)?;
        restarted.frame("worker_ready").await?;
        let pending = call(
            &executor,
            "control",
            Some(json!({"command":"pending","after":null})),
        )?;
        assert_eq!(pending["status"], "pending");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 1);
        assert_eq!(
            w.scalar("SELECT sum(spent_units)::text FROM runtime_periods")
                .await,
            "6"
        );
        assert_eq!(
            w.scalar("SELECT sum(held_units)::text FROM runtime_periods")
                .await,
            "0"
        );
        let history = w.zoen("owner", &["read", "@managed_helper"]);
        assert_eq!(history.matches(provider::ANSWER).count(), 1);
        drop(restarted);
        Ok(())
    }
    .await;
    if let Err(error) = result {
        panic!("managed service journey failed: {error}");
    }
    Ok(())
}

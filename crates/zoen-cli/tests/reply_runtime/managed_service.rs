//! Real service process, owner Core consent, dedicated native leaf and cold reply.
use super::*;
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
    #[cfg(unix)]
    fn signal(&self, signal: &str) -> Result<(), Box<dyn std::error::Error>> {
        if !Command::new("kill")
            .args([signal, &self.child.id().to_string()])
            .status()?
            .success()
        {
            return Err("owned service signal failed".into());
        }
        Ok(())
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

async fn bridge(
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
fn trust_grants(owner: &Client, space: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(owner
        .log_events(space.into())?
        .into_iter()
        .filter(|event| event.label.starts_with("Grant: trust "))
        .map(|event| event.hash)
        .collect())
}

async fn status(
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
        let started = bridge(
            &owner,
            "managed_agent_start",
            json!({"name":"Helpful agent"}),
        )
        .await?;
        let request = owner
            .pending_managed_enrollment(&account.identity_id, &account.device_id)?
            .ok_or("request")?;
        assert_eq!(started["operation_id"], request.operation);
        // The linked device receives the identity through the production sealed
        // pairing protocol, not a copied database or a fixture-injected secret.
        let linked_home = w.dir.join("owner-linked");
        std::fs::create_dir(&linked_home)?;
        let linked = Client::open(linked_home.join("zoen.sqlite").to_string_lossy().into(), "en".into())?;
        let vault: Arc<dyn client_core::SecretVault> = Arc::new(file_vault::FileVault { dir: linked_home.join("vault") });
        let code = linked.link_request(account.relay_url.clone(), vault.clone())?;
        let waiting = { let client = linked.clone(); let vault = vault.clone(); tokio::spawn(async move { client.link_wait(vault, 20_000).await }) };
        let linked_device = owner.link_device(code.code, Arc::new(file_vault::FileVault { dir: w.dir.join("owner/vault") })).await?;
        let linked_account = waiting.await??;
        assert_eq!(linked_account.identity_id, account.identity_id);
        assert_ne!(linked_account.device_id, account.device_id);
        linked.start_sync(None)?;
        linked.wait_until_settled(20_000).await?;
        owner.send_history(linked_device.device_id, None).await?;
        linked.receive_history(20_000, None).await?;
        assert!(bridge(&linked, "managed_agent_status", json!({})).await?.is_null());
        let discovery = bridge(&linked, "managed_agent_discover", json!({"after_operation":""})).await?;
        assert_eq!(discovery["requests"].as_array().ok_or("recovery page")?.len(), 1);
        assert_eq!(discovery["requests"][0]["operation_id"], request.operation);
        bridge(&linked, "managed_agent_recover", json!({"operation_id":request.operation})).await?;
        owner.stop_sync();
        drop(owner);
        let owner = linked;
        let offered = status(&owner, &mut process, "review").await?;
        for secret_field in [
            "pairing_secret",
            "sealed_offer",
            "hpke_public",
            "zoen-agent-enroll:",
        ] {
            assert!(!offered.to_string().contains(secret_field));
        }
        assert_eq!(offered["preview"]["pairing_check"], "");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
        assert!(bridge(
            &owner,
            "managed_agent_complete",
            json!({"operation_id":request.operation,"agent_id":offered["agent_id"]})
        )
        .await
        .is_err());
        #[cfg(unix)]
        {
            process.frame("enrollment_waiting").await?;
            process.signal("-STOP")?;
        }
        let approved = bridge(
            &owner,
            "managed_agent_approve",
            json!({"operation_id":request.operation,
            "observed_offer_digest":offered["preview"]["offer_digest"]}),
        )
        .await?;
        assert_eq!(approved["state"], "installing");
        #[cfg(unix)]
        {
            // The owned service parent is paused, its real enrollment child
            // continues. Wait for canonical registration, not a fixed sleep.
            let pool = sqlx::PgPool::connect(&w.db_url).await?;
            let until = Instant::now() + Duration::from_secs(20);
            loop {
                let registered: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM agent_citizens WHERE agent=$1)",
                )
                .bind(offered["agent_id"].as_str().ok_or("agent")?)
                .fetch_one(&pool)
                .await?;
                if registered {
                    break;
                }
                if Instant::now() >= until {
                    return Err("child registration was not observed".into());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            pool.close().await;
            // Simulate the owner dismissing a connection receipt on another
            // surface while its already-consumed consent remains authoritative.
            let receipt = bridge(&owner, "agent_enrollment_status", json!({})).await?;
            assert_eq!(bridge(&owner, "agent_enrollment_dismiss", json!({
                "receipt_id":receipt["delivery"]["receipt_id"], "observed_offer_digest":receipt["preview"]["offer_digest"]
            })).await?, true);
            let cancellation = bridge(
                &owner,
                "managed_agent_cancel",
                json!({"operation_id":request.operation}),
            )
            .await?;
            assert_eq!(cancellation["state"], "cancelling");
            process.signal("-CONT")?;
        }
        let installed = status(&owner, &mut process, "registered").await?;
        assert_eq!(installed["agent_id"], offered["agent_id"]);
        let agent = installed["agent_id"].as_str().ok_or("agent")?.to_owned();
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
        let context = json!({"operation_id":request.operation,"agent_id":agent});
        let space = bridge(&owner, "managed_agent_chat", context.clone())
            .await?
            .as_str()
            .ok_or("private chat")?
            .to_owned();
        owner.wait_until_settled(20_000).await?;
        let before = trust_grants(&owner, &space)?;
        assert_eq!(before.len(), 1);
        assert_eq!(
            bridge(&owner, "managed_agent_chat", context.clone()).await?,
            space
        );
        assert_eq!(trust_grants(&owner, &space)?, before);
        // Subsequent choices remain authoritative; reopening cannot silently
        // downgrade the owner's explicit grant to Listen.
        owner.set_trust(agent.clone(), space.clone(), TrustLevelDto::Suggest)?;
        owner.wait_until_settled(20_000).await?;
        let changed = trust_grants(&owner, &space)?;
        assert_eq!(changed.len(), 2);
        assert_eq!(bridge(&owner, "managed_agent_chat", context).await?, space);
        assert_eq!(trust_grants(&owner, &space)?, changed);
        owner.set_trust(agent.clone(), space.clone(), TrustLevelDto::Listen)?;
        let completed = json!({"operation_id":request.operation,"agent_id":agent});
        assert_eq!(
            bridge(&owner, "managed_agent_complete", completed.clone()).await?,
            true
        );
        assert_eq!(
            bridge(&owner, "managed_agent_complete", completed.clone()).await?,
            true
        );
        assert!(bridge(&owner, "managed_agent_status", json!({}))
            .await?
            .is_null());
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
        let history = w.zoen("owner", &["read", &format!("@{}", request.handle)]);
        assert_eq!(history.matches(provider::ANSWER).count(), 1);
        let primary = open_owner(&w)?;
        primary.start_sync(None)?;
        primary.wait_until_settled(20_000).await?;
        assert!(bridge(&primary, "managed_agent_status", json!({})).await?.is_null());
        assert!(bridge(&primary, "managed_agent_discover", json!({"after_operation":""})).await?["requests"].as_array().ok_or("completed discovery")?.is_empty());
        primary.stop_sync();
        drop(primary);
        let owner = Client::open(w.dir.join("owner-linked/zoen.sqlite").to_string_lossy().into(), "en".into())?;
        assert!(owner.unlock(Arc::new(file_vault::FileVault { dir: w.dir.join("owner-linked/vault") }))?);
        owner.start_sync(None)?;
        owner.wait_until_settled(20_000).await?;
        assert_eq!(
            bridge(&owner, "managed_agent_complete", completed.clone()).await?,
            true
        );
        let another = bridge(
            &owner,
            "managed_agent_start",
            json!({"name":"Another helper"}),
        )
        .await?;
        assert_ne!(another["operation_id"], request.operation);
        assert!(bridge(&owner, "managed_agent_complete", completed)
            .await
            .is_err());
        assert_eq!(
            bridge(&owner, "managed_agent_status", json!({})).await?["operation_id"],
            another["operation_id"]
        );
        let citizens = w.count("SELECT count(*) FROM agent_citizens").await;
        let cancel = bridge(
            &owner,
            "managed_agent_cancel",
            json!({"operation_id":another["operation_id"]}),
        )
        .await?;
        assert!(matches!(
            cancel["state"].as_str(),
            Some("cancelling" | "cancelled")
        ));
        let cancelled = status(&owner, &mut restarted, "cancelled").await?;
        assert_eq!(cancelled["operation_id"], another["operation_id"]);
        assert_eq!(
            w.count("SELECT count(*) FROM agent_citizens").await,
            citizens
        );
        assert_eq!(
            bridge(
                &owner,
                "managed_agent_discard",
                json!({"operation_id":another["operation_id"]})
            )
            .await?,
            true
        );
        assert!(bridge(&owner, "managed_agent_status", json!({}))
            .await?
            .is_null());
        owner.stop_sync();
        drop(owner);
        drop(restarted);
        Ok(())
    }
    .await;
    if let Err(error) = result {
        panic!("managed service journey failed: {error}");
    }
    Ok(())
}

//! One explicit external HTTPS model call through the real native service.
use super::managed_support::*;
use super::*;
use std::{fs, time::Duration};
use zoen_runtime::RuntimeAuthority;
#[path = "managed_live_profile.rs"]
mod profile;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit isolated profile, one-shot marker and external model credential required"]
async fn managed_live_chat_provider() -> TestResult {
    let profile = profile::Profile::read()?;
    let token = zeroize::Zeroizing::new(
        std::env::var("ZOEN_LIVE_CHAT_TOKEN").map_err(|_| "live chat credential unavailable")?,
    );
    if token.is_empty() || token.len() > 4096 || token.bytes().any(|b| b.is_ascii_control()) {
        return Err("invalid live credential".into());
    }
    let challenge = profile::challenge()?;
    profile.claim()?;
    let mut w = World::new_with("managed_live_chat", None, &[]).await;
    // Declared after World so credentials retire before failed-fixture diagnostics.
    let credentials = profile::CredentialCopies(w.dir.join("reply-runtime"));
    let mut outcome = json!({"status":"incomplete","synthetic_only":true,"automatic_retry":false,
        "configured_model":profile::MODEL,"gate":profile.gate.label(),"ui_qa":false});
    let result = run(&mut w, &profile, &token, &challenge, &mut outcome).await;
    outcome["observed_attempts"] = w
        .count("SELECT count(*) FROM runtime_attempts")
        .await
        .into();
    outcome["observed_settled_microdollars"] = w
        .scalar("SELECT COALESCE(sum(amount_units),0)::text FROM runtime_dispositions WHERE kind='settle'")
        .await
        .parse::<u64>()?
        .into();
    credentials.retire()?;
    outcome["operator_credential_retired"] = true.into();
    write_json(&profile.output_directory.join("outcome.json"), &outcome)?;
    if let Err(error) = result {
        panic!("synthetic live provider journey failed: {error}");
    }
    Ok(())
}

async fn run(
    w: &mut World,
    profile: &profile::Profile,
    token: &str,
    challenge: &str,
    outcome: &mut Value,
) -> TestResult {
    w.init("owner", "Synthetic live provider owner");
    let directory = w.dir.join("operator");
    fs::create_dir(&directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    }
    let registry = directory.join("managed.json");
    write_json(&registry, &json!({"live":true,"devices":[]}))?;
    w.stop_relay();
    w.configure_relay_env("ZOEN_MANAGED_DEVICE_CONFIG", registry.to_str());
    w.start_relay();
    let definition = zoen_agentd::Definition {
        version: 1,
        kind: zoen_agentd::AgentKind::User,
        persona:
            "Answer the owner briefly and accurately. This is a synthetic transport qualification."
                .into(),
        rules: vec!["Do not call tools or request private information.".into()],
        state: json!({}),
        model_tier: zoen_agentd::ModelTier::Cheap,
    }
    .encode()?;
    let mut value = configuration(w, "", &profile.endpoint);
    profile.price(&mut value)?;
    value["admission_config"] = registry.to_string_lossy().to_string().into();
    value["managed_service"] = json!({"id":"11".repeat(32),"definition":String::from_utf8(definition)?,"zoen_binary":env!("CARGO_BIN_EXE_zoen")});
    let config: config::Config = serde_json::from_value(value.clone())?;
    fs::create_dir(&config.home)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&config.home, fs::Permissions::from_mode(0o700))?;
    }
    let vault = Arc::new(file_vault::FileVault {
        dir: config.home.join("vault"),
    });
    for (key, bytes) in [
        ("pg".to_string(), w.db_url.as_bytes().to_vec()),
        ("evidence".into(), vec![73; 32]),
        ("chat".into(), token.as_bytes().to_vec()),
        (
            RuntimeAuthority::native_capsule_vault_key(&config.namespace, 1)?,
            vec![74; 32],
        ),
    ] {
        if !vault.save(key, bytes) {
            return Err("live operator credential save failed".into());
        }
    }
    if profile.gate == profile::GateMode::ClefFlash
        && !vault.save("live-clef".into(), token.as_bytes().to_vec())
    {
        return Err("live Gate credential save failed".into());
    }
    let path = w.dir.join("managed-service.json");
    write_json(&path, &value)?;
    if call(&path, "initialize-service", None)?["status"] != "service_initialized" {
        return Err("service initialization failed".into());
    }
    let mut service = ServiceProcess::start(w, &path)?;
    service.frame("service_ready").await?;
    let owner = open_owner(w)?;
    owner.start_sync(None)?;
    owner.wait_until_settled(20_000).await?;
    let account = owner.account().ok_or("owner")?;
    let started = bridge(
        &owner,
        "managed_agent_start",
        json!({"name":"Synthetic live model"}),
    )
    .await?;
    let operation = started["operation_id"]
        .as_str()
        .ok_or("operation")?
        .to_owned();
    let offered = status(&owner, &mut service, "review").await?;
    bridge(&owner,"managed_agent_approve",json!({"operation_id":operation,"observed_offer_digest":offered["preview"]["offer_digest"]})).await?;
    let installed = status(&owner, &mut service, "registered").await?;
    let agent = installed["agent_id"].as_str().ok_or("agent")?.to_owned();
    let executor = config
        .home
        .join(&account.identity_id)
        .join(&operation)
        .join("executor/reply-config.json");
    let signed = signed_policy_limits(
        w,
        "owner",
        &account.identity_id,
        config.chat_price.digest()?,
        10000,
        3000,
    )
    .await?;
    if call(
        &executor,
        "control",
        Some(json!({"command":"policy_import","signed":signed})),
    )?["status"]
        != "policy_installed"
    {
        return Err("signed policy unavailable".into());
    }
    let context = json!({"operation_id":operation,"agent_id":agent});
    let space = bridge(&owner, "managed_agent_chat", context.clone())
        .await?
        .as_str()
        .ok_or("conversation")?
        .to_owned();
    bridge(&owner, "managed_agent_complete", context).await?;
    owner.send_message(
        space.clone(),
        format!("Reply with exactly this phrase and nothing else: {challenge}"),
    )?;
    owner.wait_until_settled(20_000).await?;
    owner.stop_sync();
    drop(owner);
    // Observe chat (90s), optional inbound/outbound Gate (60s each), and
    // lifecycle overhead. Ordinary fixture and production deadlines stay intact.
    let output = service
        .live_output(Duration::from_secs(profile.gate.observation_seconds()))
        .await?;
    if output["delivery"]["status"] != "accepted" {
        return Err("live reply missing its delivery receipt".into());
    }
    if profile.gate == profile::GateMode::ClefFlash {
        let audit = &output["audit"];
        if audit["provider"] != profile.gate.label()
            || audit["outcome"] != "allow"
            || audit["answers"]
                .as_object()
                .is_none_or(|answers| answers.is_empty())
            || !audit["rule"]
                .as_str()
                .is_some_and(|rule| rule.starts_with("model:"))
        {
            return Err("live reply lacks a parsed allowing Clef decision".into());
        }
        outcome["outbound_gate_audit"] = json!({"provider":audit["provider"],
            "outcome":audit["outcome"],"rule":audit["rule"],
            "answers":audit["answers"],"latency_ms":audit["latency_ms"]});
    }
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 1);
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_dispositions WHERE kind='settle'")
            .await,
        1
    );
    let charge = w
        .scalar("SELECT sum(amount_units)::text FROM runtime_dispositions WHERE kind='settle'")
        .await
        .parse::<u64>()?;
    if charge == 0 || charge > 3000 {
        return Err("live reported usage outside admitted price bound".into());
    }
    let attempts = w.count("SELECT count(*) FROM runtime_attempts").await;
    drop(service);
    let mut restarted = ServiceProcess::start(w, &path)?;
    restarted.frame("service_ready").await?;
    // Exact replay of owner onboarding must not dispatch a new model attempt.
    let cold = open_owner(w)?;
    cold.start_sync(None)?;
    cold.wait_until_settled(20_000).await?;
    assert!(bridge(&cold, "managed_agent_status", json!({}))
        .await?
        .is_null());
    assert_recovered_reply(&cold, &space, &agent, &output, challenge).await?;
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_attempts").await,
        attempts
    );
    assert_eq!(
        w.scalar("SELECT sum(amount_units)::text FROM runtime_dispositions WHERE kind='settle'")
            .await
            .parse::<u64>()?,
        charge
    );
    cold.stop_sync();
    drop(cold);
    drop(restarted);
    outcome["status"] = "passed".into();
    outcome["attempts"] = attempts.into();
    outcome["settled_microdollars"] = charge.into();
    outcome["delivery"] = "accepted".into();
    outcome["recovered_without_another_attempt"] = true.into();
    Ok(())
}

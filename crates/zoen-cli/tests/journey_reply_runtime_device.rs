//! Actual enrolled device, executable host, owner MLS and cold process recovery.
mod common;
#[path = "../src/reply_host/config.rs"]
mod config;
#[path = "../src/file_vault.rs"]
mod file_vault;
#[path = "../src/gate_transport.rs"]
mod gate_transport;
#[path = "reply_runtime/provider.rs"]
mod provider;
#[path = "../src/image_host/provision.rs"]
mod provision;
#[path = "../src/runtime_host_config.rs"]
mod runtime_host_config;
#[path = "reply_runtime/support.rs"]
mod support;
#[path = "reply_runtime/worker.rs"]
mod worker;
use client_core::{Client, SecretVault, TrustLevelDto};
use common::World;
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{atomic::Ordering, Arc},
};
use support::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_reply_process_uses_its_own_leaf_and_recovers_original() -> TestResult {
    let mut w = World::new_with(
        "reply_dedicated_device",
        None,
        &[(
            "ZOEN_WORKER_WAKE_NATS_URL",
            &std::env::var("ZOEN_NATS_URL")?,
        )],
    )
    .await;
    w.init("owner", "Owner");
    let agent = w.zoen("citizen", &["agent-keys"]).trim().to_owned();
    let owner = account(&w, "owner")?["identity"]
        .as_str()
        .ok_or("owner")?
        .to_owned();
    let proof = w
        .zoen("owner", &["authorize-agent", &agent])
        .trim()
        .to_owned();
    let definition = w.dir.join("definition.json");
    write_json(
        &definition,
        &json!({"version":1,"kind":"user","persona":"Answer the owner","rules":[],"state":{},"model_tier":"cheap"}),
    )?;
    w.zoen(
        "citizen",
        &[
            "agent-init",
            "--name",
            "Reply citizen",
            "--handle",
            "reply_citizen",
            "--owner",
            &owner,
            "--proof",
            &proof,
            "--definition",
            definition.to_str().ok_or("path")?,
        ],
    );
    let citizen_device = account(&w, "citizen")?["device"]
        .as_str()
        .ok_or("citizen device")?
        .to_owned();
    let provider = provider::Provider::start().await?;
    let mut value = configuration(&w, &agent, &provider.endpoint);
    value["worker_bus_key"] = "bus".into();
    let config: config::Config = serde_json::from_value(value.clone())?;
    std::fs::create_dir(&config.home)?;
    let vault = Arc::new(file_vault::FileVault {
        dir: config.home.join("vault"),
    });
    let code = provision::prepare(&config.enrollment_scope(), &vault)?;
    assert_ne!(code.device, citizen_device);
    assert_eq!(
        provision::prepare(&config.enrollment_scope(), &vault)?,
        code
    );
    w.zoen("citizen", &["link", &code.encode()]);
    value["device"] = code.device.clone().into();
    let config: config::Config = serde_json::from_value(value.clone())?;
    provision::validate_installed(&config.enrollment_scope(), &vault)?;
    for (name, bytes) in [
        ("pg", w.db_url.as_bytes().to_vec()),
        ("evidence", vec![73; 32]),
        ("chat", b"local-chat-fixture".to_vec()),
    ] {
        assert!(vault.save(name.into(), bytes));
    }
    assert!(vault.save("bus".into(), std::env::var("ZOEN_NATS_URL")?.into_bytes()));
    let path = w.dir.join("reply-host.json");
    write_json(&path, &value)?;
    let checked = config::Config::read(&path)?;
    assert_eq!(checked.worker_bus_key.as_deref(), Some("bus"));
    let _ = checked.runtime(&vault)?;
    let _ = checked.host()?;
    let _ = checked.gate(&vault)?;
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        assert!(provision::run(&checked.enrollment_scope(), &vault).is_err());
    }
    assert_eq!(call(&path, "initialize", None)?["status"], "initialized");
    assert_eq!(call(&path, "initialize", None)?["status"], "initialized");
    let directory = w.dir.join("operator");
    std::fs::create_dir(&directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
    }
    let registry = directory.join("managed.json");
    let witness = w
        .scalar("SELECT witness FROM runtime_deployment_binding WHERE singleton")
        .await;
    write_json(
        &registry,
        &json!({"devices":[],"runtimes":[{"deployment":{"namespace":config.namespace,"witness":witness,"source":w.cell},"anchor_directory":config.continuity,"principals":[{"agent":agent,"device":code.device}]}]}),
    )?;
    w.stop_relay();
    w.configure_relay_env("ZOEN_MANAGED_DEVICE_CONFIG", registry.to_str());
    w.start_relay();
    value["relay_key"] = w.relay_name().into();
    std::fs::remove_file(&path)?;
    write_json(&path, &value)?;
    // A valid unrelated owner signature is not authority to install a policy
    // through this configured agent. Rejection must precede financial writes.
    w.init("outsider", "Unrelated owner");
    let outsider = account(&w, "outsider")?["identity"]
        .as_str()
        .ok_or("outsider identity")?
        .to_owned();
    let unrelated = signed_policy(&w, "outsider", &outsider, config.chat_price.digest()?).await?;
    assert!(call(
        &path,
        "execute",
        Some(json!({"command":"policy_import","signed":unrelated})),
    )
    .is_err());
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 0);
    let signed = signed_policy(&w, "owner", &owner, config.chat_price.digest()?).await?;
    assert_eq!(
        call(
            &path,
            "execute",
            Some(json!({"command":"policy_import","signed":signed.clone()}))
        )?["status"],
        "policy_installed"
    );
    // Exact replay acknowledges the retained installation, including a cold
    // process; it never makes a second version or resets the monthly budget.
    assert_eq!(
        call(
            &path,
            "execute",
            Some(json!({"command":"policy_import","signed":signed}))
        )?["status"],
        "policy_installed"
    );
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 1);
    let space = w
        .zoen("owner", &["dm", "@reply_citizen", "A calm conversation"])
        .trim()
        .to_owned();
    let client = Client::open(
        w.dir
            .join("owner")
            .join("zoen.sqlite")
            .to_string_lossy()
            .into_owned(),
        "en".into(),
    )?;
    assert!(client.unlock(Arc::new(file_vault::FileVault {
        dir: w.dir.join("owner").join("vault")
    }))?);
    client.start_sync(None)?;
    client.wait_until_settled(20_000).await?;
    client.set_trust(agent.clone(), space.clone(), TrustLevelDto::Listen)?;
    client.send_message(
        space.clone(),
        "Can you answer with the client closed?".into(),
    )?;
    client.wait_until_settled(20_000).await?;
    client.stop_sync();
    drop(client);
    let scan = call(
        &path,
        "execute",
        Some(json!({"command":"discover","space":space})),
    )?;
    assert_eq!(scan["complete"], true);
    assert_eq!(scan["caught_up"], true);
    let runs = scan["runs"].as_array().ok_or("runs")?;
    let run = runs.last().ok_or("run")?["run"].as_str().ok_or("run id")?;
    let first = call(
        &path,
        "execute",
        Some(json!({"command":"advance","run":run})),
    )?;
    assert_eq!(first["status"], "output");
    assert_eq!(first["delivery"]["status"], "accepted");
    assert_eq!(first["financial"], json!({"status":"settled","units":6}));
    for _ in 0..2 {
        let repeated = call(
            &path,
            "execute",
            Some(json!({"command":"advance","run":run})),
        )?;
        assert_eq!(repeated["status"], "completed");
        assert_eq!(repeated["client_id"], first["client_id"]);
    }
    let frames = serve(
        &path,
        &[
            json!({"command":"pending","after":null}),
            json!({"command":"advance","run":run}),
        ],
    )?;
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0]["status"], "pending");
    assert_eq!(frames[1]["status"], "completed");
    assert_eq!(frames[1]["client_id"], first["client_id"]);
    assert!(call(
        &path,
        "execute",
        Some(json!({"command":"advance","run":run,"approved":true}))
    )
    .is_err());
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
    let history = w.zoen("owner", &["read", "@reply_citizen"]);
    assert_eq!(history.matches(provider::ANSWER).count(), 1);
    assert!(!config.home.join("zoen.sqlite").exists());
    assert!(!vault.dir.join("zoen.identity.v1").exists());
    worker::run(&w, &path, &agent, &space, &provider).await?;
    println!("reply executable: actual sponsor/device pairing, canonical persona on dedicated leaf, owner-signed budget, closed clients, paid HTTP, owner MLS and repeated cold processes without duplicate inference/charge/output PASS");
    Ok(())
}

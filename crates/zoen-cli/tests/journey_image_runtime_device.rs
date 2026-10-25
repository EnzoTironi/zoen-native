//! Real sponsor enrollment and distinct Runtime/CLI MLS leaves. No provider call
//! or production admission capability is injected by this journey.
mod common;
#[path = "../src/image_host/config.rs"]
mod config;
#[path = "../src/file_vault.rs"]
mod file_vault;
#[path = "image_runtime/host_journey.rs"]
mod host_journey;
#[path = "../src/image_host/provision.rs"]
mod provision;

use client_core::{Client, SecretVault, TrustLevelDto};
use common::World;
use std::{path::Path, process::Command, sync::Arc};
use zoen_runtime::RuntimeAuthority;

fn account(w: &World, who: &str) -> serde_json::Value {
    let db = rusqlite::Connection::open(w.dir.join(who).join("zoen.sqlite")).unwrap();
    let json: String = db
        .query_row("SELECT value FROM meta WHERE key='account'", [], |r| {
            r.get(0)
        })
        .unwrap();
    serde_json::from_str(&json).unwrap()
}
fn write_json(path: &Path, value: &serde_json::Value) {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .unwrap()
        .write_all(&serde_json::to_vec(value).unwrap())
        .unwrap();
}
fn configuration(w: &World, agent: &str) -> serde_json::Value {
    serde_json::json!({
        "namespace":format!("image-device-{}", w.cell), "agent":agent,"device":"",
        "home":w.dir.join("image-runtime"),"continuity":w.dir.join("independent-anchor"),
        "postgres_key":"pg","evidence_key":"evidence","fdb_cluster_file":std::env::var("FDB_CLUSTER_FILE").ok(),
        "source_cell":w.cell,"relay_url":w.relay_url(),"relay_key":w.relay_name(),
        "key_version":1,"definition_version":"image-v1","image_credential_key":"image","chat_credential_key":"chat",
        "chat_price":{"version":"chat-v1","endpoint":"https://api.openai.com/v1/chat/completions","model":"model","credential_ref":"chat","currency":"USD","scale":6,"input_units_per_million":1,"output_units_per_million":1,"max_billable_input_tokens":1,"max_output_tokens":1,"max_request_bytes":4096,"max_response_bytes":4096},
        "image_price":{"version":"image-v1","endpoint":"https://api.openai.com/v1/images/generations","model":"gpt-image-fixture","credential_ref":"images","currency":"USD","scale":6,"max_request_bytes":16384,"max_response_bytes":100000,"reservation_units":10,"input_text_units_per_million":1,"input_image_units_per_million":1,"output_text_units_per_million":1,"output_image_units_per_million":1,"image":{"count":1,"format":"png","size":"1024x1024","quality":"low","background":"opaque","max_image_bytes":1000}}
    })
}
#[test]
fn dedicated_runtime_and_citizen_open_the_same_source_from_their_own_welcome(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // SAFETY: this test owns the sole FDB network in this test executable;
    // both the Runtime authority and Tokio executor end before this guard.
    let network = unsafe { foundationdb::boot() };
    let executor = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    executor.block_on(async {
        let mut w = World::new("image_dedicated_device").await;
        w.init("owner", "Owner");
        let agent = w.zoen("citizen", &["agent-keys"]).trim().to_owned();
        let proof = w.zoen("owner", &["authorize-agent", &agent]).trim().to_owned();
        let definition = w.dir.join("definition.json");
        write_json(&definition, &serde_json::json!({"version":1,"kind":"user","persona":"Answer the owner","rules":[],"state":{},"model_tier":"cheap"}));
        let owner = account(&w, "owner")["identity"].as_str().unwrap().to_owned();
        w.zoen("citizen", &["agent-init","--name","Image citizen","--handle","image_citizen","--owner",&owner,"--proof",&proof,"--definition",definition.to_str().unwrap()]);
        let citizen_device = account(&w, "citizen")["device"].as_str().unwrap().to_owned();
        let provider = host_journey::Provider::start().await;
        let mut value = configuration(&w, &agent);
        value["image_price"]["endpoint"] = provider.endpoint.clone().into();
        value["image_price"]["endpoint_policy"] = serde_json::to_value(zoen_runtime::ImageEndpointPolicy::LoopbackFixture).unwrap();
        value["image_price"]["image"]["max_image_bytes"] = 1_000_000.into();
        value["image_price"]["max_response_bytes"] = 2_000_000.into();
        let original_config: config::Config = serde_json::from_value(value.clone()).unwrap();
        std::fs::create_dir(&original_config.home).unwrap();
        let vault = Arc::new(file_vault::FileVault { dir: original_config.home.join("vault") });
        assert_eq!(original_config.host().unwrap().agent, agent);
        use std::io::IsTerminal;
        if !std::io::stdout().is_terminal() {
            assert!(provision::run(&original_config, &vault).is_err());
            assert!(!vault.dir.exists(), "non-interactive pairing wrote secrets");
        }
        let pairing = provision::prepare(&original_config, &vault).unwrap();
        assert_ne!(pairing.device, citizen_device);
        assert_eq!(provision::prepare(&original_config, &vault).unwrap(), pairing);
        // The actual certified sponsor enrolls the exact dedicated device via
        // unchanged native link code. No IdentityBox is installed at the host.
        w.zoen("citizen", &["link", &pairing.encode()]);
        value["device"] = pairing.device.clone().into();
        let installed: config::Config = serde_json::from_value(value.clone()).unwrap();
        provision::validate_installed(&installed, &vault).unwrap();
        let mut foreign_value = value.clone();
        foreign_value["device"] = citizen_device.clone().into();
        let foreign: config::Config = serde_json::from_value(foreign_value).unwrap();
        assert!(provision::validate_installed(&foreign, &vault).is_err());
        let path = w.dir.join("image-host.json"); write_json(&path, &value);
        assert!(vault.save("pg".into(), w.db_url.as_bytes().to_vec()));
        assert!(vault.save("evidence".into(), vec![73;32]));
        assert!(vault.save("chat".into(), b"unpaid-unused-chat-fixture".to_vec()));
        assert!(vault.save("image".into(), b"unpaid-local-image-fixture".to_vec()));
        let initialize = || Command::new(env!("CARGO_BIN_EXE_zoen-image-host")).arg("--config").arg(&path).arg("initialize").output().unwrap();
        let output = initialize();
        assert!(output.status.success(), "initialize: {}", String::from_utf8_lossy(&output.stderr));
        let retry = initialize();
        assert!(retry.status.success(), "initialize retry: {}", String::from_utf8_lossy(&retry.stderr));
        // This is trusted deployment configuration for the real relay process,
        // independent of restored SQL/FDB. Only this synthetic World's witness
        // is read; it never enters fixture logs or content requests.
        let operator = w.dir.join("operator");
        std::fs::create_dir(&operator).unwrap();
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&operator, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let registry = operator.join("managed-devices.json");
        let witness = w.scalar("SELECT witness FROM runtime_deployment_binding WHERE singleton").await;
        write_json(&registry, &serde_json::json!({"devices":[{"deployment":{"namespace":installed.namespace,"witness":witness,"source":w.cell,"agent":agent,"device":pairing.device},"anchor_directory":installed.continuity}]}));
        w.stop_relay();
        w.configure_relay_env("ZOEN_MANAGED_DEVICE_CONFIG", Some(registry.to_str().unwrap()));
        w.start_relay();
        // World intentionally gives a restarted relay a new Challenge name.
        // Bind the operator route to that observed actual host, never weaken
        // the Runtime's strict expected-relay check.
        value["relay_key"] = w.relay_name().into();
        std::fs::remove_file(&path).unwrap();
        write_json(&path, &value);
        // Both leaves exist before the first conversation invitation. Claiming
        // stock through normal MLS creation must include both active devices.
        let space = w.zoen("owner", &["dm","@image_citizen","The shared source is a calm sea"]).trim().to_owned();
        // The first CLI command may correctly return its locally queued space
        // while the retired relay node's finite partition lease expires. Reuse
        // the real owner's existing strict sync budget before asking the other
        // device to read; no lease row, TTL or MLS state is changed by setup.
        let owner_client = Client::open(w.dir.join("owner").join("zoen.sqlite").to_string_lossy().into_owned(), "en".into()).unwrap();
        assert!(owner_client.unlock(Arc::new(file_vault::FileVault { dir: w.dir.join("owner").join("vault") })).unwrap());
        owner_client.start_sync(None).unwrap();
        owner_client.wait_until_settled(20_000).await.unwrap();
        assert!(w.zoen("citizen", &["read", "@owner"]).contains("The shared source is a calm sea"));
        let config = config::Config::read(&path).unwrap();
        let secrets: Arc<dyn SecretVault> = vault.clone();
        let runtime = RuntimeAuthority::open_with_native_device(config.runtime(&vault).unwrap(), secrets, &agent, &pairing.device, 1).await.unwrap();
        host_journey::initial_account_context(&w, &path, &owner_client, &config, &space, &provider).await.expect("first native account context");
        let scan = runtime.sync_reply_runs(&agent, &pairing.device, &space).await.unwrap();
        assert!(scan.caught_up && scan.discovery_complete, "native source did not converge: {scan:?}");
        assert!(scan.runs.is_empty(), "a source without the owner's grant became executable");
        // Publish the permission through the actual owner Client and encrypted
        // conversation. The independent Runtime leaf must open this grant and
        // a subsequent source; test setup never writes authority rows directly.
        host_journey::install_policy(&w, &config, &runtime).await;
        owner_client.set_trust(agent.clone(), space.clone(), TrustLevelDto::Listen).unwrap();
        owner_client.send_message(space.clone(), "The authorized shared source is a calm sea".into()).unwrap();
        owner_client.wait_until_settled(20_000).await.unwrap();
        owner_client.stop_sync();
        drop(owner_client);
        assert!(w.zoen("citizen", &["read", "@owner"]).contains("The authorized shared source is a calm sea"));
        let granted = runtime.sync_reply_runs(&agent, &pairing.device, &space).await.unwrap();
        assert!(granted.caught_up && granted.discovery_complete, "authorized native source did not converge: {granted:?}");
        assert!(!granted.runs.is_empty(), "Runtime could not decrypt/discover the owner's actual granted source with its own MLS leaf");
        assert!(!original_config.home.join("zoen.sqlite").exists());
        assert!(!vault.dir.join("zoen.identity.v1").exists());
        assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
        host_journey::run(&w, &path, &config, runtime, &space, &provider).await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    drop(executor);
    drop(network);
    Ok(())
}

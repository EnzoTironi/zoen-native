//! Real sponsor enrollment and distinct Runtime/CLI MLS leaves. No provider call
//! or production admission capability is injected by this journey.
mod common;
#[path = "../src/image_host/config.rs"]
mod config;
#[path = "../src/file_vault.rs"]
mod file_vault;
#[path = "../src/image_host/provision.rs"]
mod provision;

use client_core::SecretVault;
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
fn dedicated_runtime_and_citizen_open_the_same_source_from_their_own_welcome() {
    // SAFETY: this test owns the sole FDB network in this test executable;
    // both the Runtime authority and Tokio executor end before this guard.
    let network = unsafe { foundationdb::boot() };
    let executor = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    executor.block_on(async {
        let w = World::new("image_dedicated_device").await;
        w.init("owner", "Owner");
        let agent = w.zoen("citizen", &["agent-keys"]).trim().to_owned();
        let proof = w.zoen("owner", &["authorize-agent", &agent]).trim().to_owned();
        let definition = w.dir.join("definition.json");
        write_json(&definition, &serde_json::json!({"version":1,"kind":"user","persona":"Answer the owner","rules":[],"state":{},"model_tier":"cheap"}));
        let owner = account(&w, "owner")["identity"].as_str().unwrap().to_owned();
        w.zoen("citizen", &["agent-init","--name","Image citizen","--handle","image_citizen","--owner",&owner,"--proof",&proof,"--definition",definition.to_str().unwrap()]);
        let citizen_device = account(&w, "citizen")["device"].as_str().unwrap().to_owned();
        let mut value = configuration(&w, &agent);
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
        let path = w.dir.join("image-host.json"); write_json(&path, &value);
        assert!(vault.save("pg".into(), w.db_url.as_bytes().to_vec()));
        assert!(vault.save("evidence".into(), vec![73;32]));
        assert!(vault.save("chat".into(), b"unpaid-unused-chat-fixture".to_vec()));
        let initialize = || Command::new(env!("CARGO_BIN_EXE_zoen-image-host")).arg("--config").arg(&path).arg("initialize").output().unwrap();
        let output = initialize();
        assert!(output.status.success(), "initialize: {}", String::from_utf8_lossy(&output.stderr));
        let retry = initialize();
        assert!(retry.status.success(), "initialize retry: {}", String::from_utf8_lossy(&retry.stderr));
        // Both leaves exist before the first conversation invitation. Claiming
        // stock through normal MLS creation must include both active devices.
        let space = w.zoen("owner", &["dm","@image_citizen","The shared source is a calm sea"]).trim().to_owned();
        assert!(w.zoen("citizen", &["read", "@owner"]).contains("The shared source is a calm sea"));
        let config = config::Config::read(&path).unwrap();
        let secrets: Arc<dyn SecretVault> = vault.clone();
        let runtime = RuntimeAuthority::open_with_native_device(config.runtime(&vault).unwrap(), secrets, &agent, &pairing.device, 1).await.unwrap();
        let scan = runtime.sync_reply_runs(&agent, &pairing.device, &space).await.unwrap();
        assert!(scan.caught_up && scan.discovery_complete, "native source did not converge: {scan:?}");
        assert!(!scan.runs.is_empty(), "Runtime could not decrypt/discover the owner's actual source with its own MLS leaf");
        assert!(!original_config.home.join("zoen.sqlite").exists());
        assert!(!vault.dir.join("zoen.identity.v1").exists());
        assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    });
    drop(executor);
    drop(network);
}

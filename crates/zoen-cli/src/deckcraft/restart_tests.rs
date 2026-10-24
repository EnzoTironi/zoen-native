// A process-boundary proof, not a relay daemon or real-model journey. Both
// children execute the actual Runtime/Gate/SpaceTools with ScriptedBrain and
// independently reload the operator-signed component through configured().
use super::*;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const HOME: &str = "ZOEN_TEST_DECKCRAFT_RESTART_HOME";
const PHASE: &str = "ZOEN_TEST_DECKCRAFT_RESTART_PHASE";
const CONFIG: [&str; 3] = [
    "ZOEN_DECKCRAFT_WORKER_COMPONENT",
    "ZOEN_DECKCRAFT_WORKER_MANIFEST",
    "ZOEN_DECKCRAFT_WORKER_PUBLISHER",
];
const CHILD: &str = "deckcraft::tests::restart::operator_restart_child";

fn write_new(path: &Path, value: &Value) {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).unwrap();
    file.write_all(&serde_json::to_vec_pretty(value).unwrap())
        .unwrap();
    file.sync_all().unwrap();
}

fn private_directory(path: &Path) {
    assert!(path.is_absolute());
    assert!(
        matches!(fs::symlink_metadata(path), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        "refuse existing restart profile"
    );
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).unwrap();
    assert_eq!(fs::canonicalize(path).unwrap(), path);
}

fn profile() -> PathBuf {
    PathBuf::from(std::env::var_os(HOME).expect("explicit private restart profile required"))
}

fn open_client(home: &Path) -> Arc<Client> {
    let client = Client::open(
        home.join("agent.sqlite").to_str().unwrap().into(),
        "en".into(),
    )
    .unwrap();
    assert!(client
        .unlock(Arc::new(crate::FileVault {
            dir: home.join("vault")
        }))
        .unwrap());
    client
}

fn snapshot(home: &Path, item: &str) -> Value {
    let client = open_client(home);
    let head = client.app_binary_read(item.into(), None, None).unwrap();
    let count = client.item(item.into()).unwrap().versions.len();
    let versions: Vec<_> = (1..=count as u32)
        .map(|version| {
            client
                .app_binary_read(item.into(), Some(version), None)
                .unwrap()
        })
        .collect();
    drop(client);
    // Read-only oracle against this test's synthetic SQLite only. Preserve opaque
    // production intent/fingerprint/receipt bytes; do not reimplement their hash.
    let store = store::Store::open(home.join("agent.sqlite").to_str().unwrap()).unwrap();
    let mut query = store.conn().prepare("SELECT key, value FROM meta WHERE key LIKE 'app_document_receipts:%' ORDER BY key LIMIT 65").unwrap();
    let receipts: Vec<(String, String)> = query
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(receipts.len() < 65);
    let events: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap();
    let outbox: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM outbox", [], |row| row.get(0))
        .unwrap();
    json!({"head":head,"versions":versions,"receipts":receipts,"events":events,"outbox":outbox})
}

fn launch(home: &Path, phase: &str) -> std::process::ExitStatus {
    let log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(home.join(format!("child-{phase}.log")))
        .unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .env_clear()
        .args([
            CHILD,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(HOME, home)
        .env(PHASE, phase)
        .env("RAYON_NUM_THREADS", "1")
        .env("TMPDIR", home.join("temp"))
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log));
    for name in CONFIG {
        command.env(
            name,
            std::env::var_os(name).expect("complete operator config required"),
        );
    }
    if phase == "a" {
        command.env("ZOEN_AGENT_FAULT", "before-journal").env(
            "ZOEN_AGENT_FAULT_MARKER",
            home.join("accepted-before-journal"),
        );
    }
    let mut child = command.spawn().unwrap();
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if started.elapsed() > Duration::from_secs(600) {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("bounded restart child {phase} exceeded 600 seconds");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicit operator-signed component config and a nonexistent private restart profile"]
async fn operator_restart_child() {
    let home = profile();
    let phase = std::env::var(PHASE).unwrap();
    assert!(matches!(phase.as_str(), "a" | "b"));
    assert!(std::env::var_os("ZOEN_AGENT_SCRIPT").is_none());
    if phase == "b" {
        assert!(std::env::var_os("ZOEN_AGENT_FAULT").is_none());
    }
    let request: Value =
        serde_json::from_slice(&fs::read(home.join("request.json")).unwrap()).unwrap();
    let item = request["item"].as_str().unwrap();
    let source = request["source"].as_str().unwrap();
    let serving = request["serving"].as_str().unwrap();
    let client = open_client(&home);
    let started = Instant::now();
    let worker = configured()
        .expect("operator configuration accepted")
        .expect("operator worker required; no fixture fallback");
    let load_ms = started.elapsed().as_millis();
    let component = bounded_file(&std::env::var(CONFIG[0]).unwrap(), 64 * 1024 * 1024).unwrap();
    let manifest = bounded_file(&std::env::var(CONFIG[1]).unwrap(), 64 * 1024).unwrap();
    write_new(
        &home.join(format!("child-{phase}-load.json")),
        &json!({"pid":std::process::id(),"coldConfiguredLoadMilliseconds":load_ms,"componentSha256":hash(&component),"manifestSha256":hash(&manifest),"publisher":std::env::var(CONFIG[2]).unwrap(),"scriptedBrainOnly":true}),
    );
    let arguments = if phase == "a" {
        let document: Value = serde_json::from_str(
            &call(
                &client,
                &worker,
                item,
                source,
                serving,
                "read_deckcraft",
                &json!({"artifactId":item}),
            )
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(document["head"]["version"], "1");
        let arguments = at(
            &document,
            json!({"slideId":document["slides"][0]["id"],"text":"Reviewed notes survive complete host process restart"}),
        );
        write_new(&home.join("arguments.json"), &arguments);
        arguments
    } else {
        assert!(
            !home.join("tool-journal.json").exists(),
            "replay must start without a host journal cache"
        );
        serde_json::from_slice(&fs::read(home.join("arguments.json")).unwrap()).unwrap()
    };
    assert_eq!(arguments["version"], 1);
    assert_eq!(arguments["sha256"], request["baseSha256"]);
    let started = Instant::now();
    let steps = crate::agent::exercise_deckcraft_operation(
        &client,
        worker,
        &home.join("tool-journal.json"),
        (item, source, serving),
        arguments,
    )
    .await;
    assert_eq!(
        phase, "b",
        "phase A must exit86 at the existing before-journal hook"
    );
    let receipt: Value = serde_json::from_str(&steps[0].result).unwrap();
    assert_eq!(receipt["replayed"], true);
    assert_eq!(receipt["version"], 2);
    assert_eq!(receipt["currentVersion"], 2);
    write_new(
        &home.join("child-b-result.json"),
        &json!({"pid":std::process::id(),"canonicalReplayMilliseconds":started.elapsed().as_millis(),"receipt":receipt,"steps":steps}),
    );
}

#[test]
#[ignore = "whole-process proof: explicit operator configuration and nonexistent private profile required; cold component loads can take minutes"]
fn operator_signed_worker_survives_whole_host_restart_after_acceptance(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for name in [
        "ZOEN_AGENT_FAULT",
        "ZOEN_AGENT_FAULT_MARKER",
        "ZOEN_AGENT_SCRIPT",
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GOOGLE_API_KEY",
        "GEMINI_API_KEY",
    ] {
        assert!(
            std::env::var_os(name).is_none(),
            "restart supervisor must have sanitized {name}"
        );
    }
    for name in CONFIG {
        assert!(
            std::env::var_os(name).is_some(),
            "complete explicit operator configuration required"
        );
    }
    let home = profile();
    private_directory(&home);
    private_directory(&home.join("vault"));
    private_directory(&home.join("temp"));
    let mut random_root = [0; 32];
    getrandom::getrandom(&mut random_root).unwrap();
    let fixture = fixture_at(home.clone(), random_root)?;
    let file_vault = crate::FileVault {
        dir: home.join("vault"),
    };
    for (key, bytes) in fixture.vault.0.lock().unwrap().iter() {
        assert!(file_vault.save(key.clone(), bytes.clone()));
    }
    let item = fixture.base.item_id.clone();
    let actor = fixture.base.actor_id.clone();
    write_new(
        &home.join("request.json"),
        &json!({"item":item,"source":fixture.source,"serving":fixture.owner,"baseSha256":fixture.base.sha256,"supervisorPid":std::process::id()}),
    );
    drop(fixture); // no Client/Worker Arc crosses either exec boundary
    let initial = snapshot(&home, &item);
    assert_eq!(initial["head"]["version"], 1);
    let first = launch(&home, "a");
    assert_eq!(first.code(), Some(86));
    assert_eq!(
        fs::read(home.join("accepted-before-journal")).unwrap(),
        b"before-journal"
    );
    assert!(!home.join("tool-journal.json").exists());
    let accepted = snapshot(&home, &item);
    assert_eq!(accepted["head"]["version"], 2);
    assert_eq!(accepted["head"]["author_id"], actor);
    assert_eq!(accepted["versions"].as_array().unwrap().len(), 2);
    assert_ne!(accepted["head"]["sha256"], initial["head"]["sha256"]);
    assert_ne!(accepted["receipts"], initial["receipts"]);
    write_new(&home.join("accepted-snapshot.json"), &accepted);
    assert!(launch(&home, "b").success());
    let after = snapshot(&home, &item);
    assert_eq!(after, accepted, "exact bytes, every version, opaque intent/receipt, signed events and outbox must remain unchanged");
    let load_a: Value =
        serde_json::from_slice(&fs::read(home.join("child-a-load.json")).unwrap()).unwrap();
    let load_b: Value =
        serde_json::from_slice(&fs::read(home.join("child-b-load.json")).unwrap()).unwrap();
    assert_ne!(load_a["pid"], load_b["pid"]);
    assert_ne!(load_a["pid"], std::process::id());
    for field in ["componentSha256", "manifestSha256", "publisher"] {
        assert_eq!(load_a[field], load_b[field]);
    }
    let result: Value =
        serde_json::from_slice(&fs::read(home.join("child-b-result.json")).unwrap()).unwrap();
    assert_eq!(result["receipt"]["sha256"], accepted["head"]["sha256"]);
    let journal: Vec<(String, Vec<zoen_agentd::ToolStep>)> =
        serde_json::from_slice(&fs::read(home.join("tool-journal.json")).unwrap()).unwrap();
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0].1.len(), 1);
    let operation_receipts: Vec<Value> = accepted["receipts"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| serde_json::from_str::<Vec<Value>>(row[1].as_str().unwrap()).unwrap())
        .filter(|receipt| receipt["id"].as_str().unwrap().starts_with("deckcmd_"))
        .collect();
    assert_eq!(operation_receipts.len(), 1);
    assert_eq!(operation_receipts[0]["version"], 2);
    assert_eq!(operation_receipts[0]["sha256"], result["receipt"]["sha256"]);
    assert_eq!(operation_receipts[0]["intent"].as_str().unwrap().len(), 64);
    write_new(
        &home.join("restart-proof.json"),
        &json!({"wholeHostedRuntimeProcessRestartQualified":true,"scriptedBrainOnly":true,"relayDaemonWakeQualified":false,"realProviderQualified":false,"supervisorPid":std::process::id(),"childA":load_a,"childB":load_b,"childAExitCode":86,"acceptedVersion":2,"versionCount":2,"exactDocumentAndVersionBytesUnchanged":true,"exactOpaqueIntentAndReceiptsUnchanged":true,"eventAndOutboxCountsUnchanged":true,"acceptedDocumentSha256":accepted["head"]["sha256"],"operationReceipt":operation_receipts[0],"toolJournalEntriesAfterReplay":1,"canonicalReplayMilliseconds":result["canonicalReplayMilliseconds"]}),
    );
    Ok(())
}

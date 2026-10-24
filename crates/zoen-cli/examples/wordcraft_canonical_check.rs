//! Offline qualification: real signed worker and ordinary canonical encrypted Item APIs.
//! Synthetic local account; no browser/catalogue, network, model, approval or iPhone claim.
#![deny(
    clippy::pedantic,
    clippy::perf,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
use base64::{engine::general_purpose::STANDARD, Engine as _};
use client_core::{AppBinaryDocumentDto, Client, PrivacyDto, SecretVault};
use ed25519_dalek::SigningKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};
use zoen_agentd::{deckcraft::DeckcraftWorker, Needs, ToolManifest};
type Result<T> = std::result::Result<T, String>;
const REVISION: &str = "d1b0bfc3f9dd83126b162a42b667facecb003b7f";
const LIMIT: usize = 512 * 1024;
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn require(ok: bool, code: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(code.into())
    }
}
fn read(path: &Path, max: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "input unavailable")?
        .take((max + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "input unreadable")?;
    require(!bytes.is_empty() && bytes.len() <= max, "input bound")?;
    Ok(bytes)
}
#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().ok()?.get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        let Ok(mut values) = self.0.lock() else {
            return false;
        };
        values.insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        if let Ok(mut values) = self.0.lock() {
            values.remove(&key);
        }
    }
}
fn load(component: &[u8]) -> Result<DeckcraftWorker> {
    let key = SigningKey::from_bytes(&[42; 32]); // Synthetic offline publisher only.
    let spec = ToolManifest {
        id: "wordcraft-canonical-v1".into(),
        version: REVISION.into(),
        summary: "Offline Word canonical qualification".into(),
        needs: Needs::Wasm,
        egress: vec![],
        secrets: vec![],
        component_sha256: Some(sha(component)),
    };
    let mut bad = spec.clone();
    bad.version = "foreign-engine".into();
    require(
        DeckcraftWorker::load_for(
            "wordcraft",
            vec![key.verifying_key()],
            &bad.sign(&key),
            component,
        )
        .is_err(),
        "foreign engine admitted",
    )?;
    DeckcraftWorker::load_for(
        "wordcraft",
        vec![key.verifying_key()],
        &spec.sign(&key),
        component,
    )
    .map_err(|_| "canonical worker load failed".into())
}
fn create(path: &Path, seed: &[u8]) -> Result<(Arc<Client>, Arc<Vault>, AppBinaryDocumentDto)> {
    let path = path.to_str().ok_or("invalid database path")?;
    let client = Client::open(path.into(), "en".into()).map_err(|_| "canonical open failed")?;
    let vault = Arc::new(Vault::default());
    client
        .create_account(
            "Synthetic Word qualification".into(),
            "word_canonical_fixture".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .map_err(|_| "account fixture failed")?;
    let space = client
        .create_group_with("Word qualification".into(), vec![], PrivacyDto::Closed)
        .map_err(|_| "private fixture failed")?;
    let base = client
        .app_binary_create_for(
            "wordcraft".into(),
            space.clone(),
            "Document".into(),
            STANDARD.encode(seed),
            "create-word-once".into(),
        )
        .map_err(|_| "canonical create failed")?;
    let retry = client
        .app_binary_create_for(
            "wordcraft".into(),
            space,
            "Document".into(),
            STANDARD.encode(seed),
            "create-word-once".into(),
        )
        .map_err(|_| "canonical creation observation failed")?;
    require(
        base == retry && base.kind == "wordcraft" && base.engine_revision == REVISION,
        "creation retry changed original",
    )?;
    Ok((client, vault, base))
}
fn request(base: &AppBinaryDocumentDto, text: &str) -> Value {
    let scope =
        json!({"artifactId":base.item_id,"actorId":base.actor_id,"actorKind":base.actor_kind});
    let head = json!({"version":base.version.to_string(),"sha256":base.sha256});
    json!({"schema":1,"open":{"scope":scope,"head":head},"command":{"mutation":{"scope":scope,"base":head,"mutationId":"word-canonical-edit"},"action":{"command":"text.replace","params":{"text":text}}}})
}
async fn proposal(
    worker: &DeckcraftWorker,
    base: &AppBinaryDocumentDto,
    seed: &[u8],
) -> Result<(Vec<u8>, String)> {
    require(
        worker.supports("wordcraft")
            && !worker.supports("deckcraft")
            && !worker.supports("gridcraft"),
        "worker format confusion",
    )?;
    let req = request(base, "Canonical DOCX\nOlá 日本語 👩🏽‍💻\tretained\n");
    let result = worker
        .run_for("wordcraft", &req, seed)
        .await
        .map_err(|_| "actual worker edit failed")?;
    require(
        result.metadata["sha256"] == sha(&result.bytes)
            && result.metadata["operation"]["action"] == req["command"]["action"]
            && result.metadata["acceptedHead"].is_null(),
        "proposal changed action or bytes",
    )?;
    let retry = worker
        .run_for("wordcraft", &req, seed)
        .await
        .map_err(|_| "actual worker retry failed")?;
    require(
        retry.bytes == result.bytes && retry.metadata == result.metadata,
        "worker retry changed original",
    )?;
    for text in ["before\u{fffe}after", "before\u{ffff}after"] {
        require(
            worker
                .run_for("wordcraft", &request(base, text), seed)
                .await
                .is_err(),
            "lossy XML input produced proposal",
        )?;
    }
    let intent = sha(&serde_json::to_vec(&req["command"]["action"]).map_err(|_| "invalid intent")?);
    Ok((result.bytes, intent))
}
fn commit(
    client: &Client,
    base: &AppBinaryDocumentDto,
    bytes: &[u8],
    intent: &str,
) -> Result<AppBinaryDocumentDto> {
    client
        .app_binary_save(
            base.item_id.clone(),
            base.version,
            base.sha256.clone(),
            STANDARD.encode(bytes),
            "word-canonical-edit".into(),
            intent.into(),
        )
        .map_err(|_| "canonical commit failed".into())
}
fn recover(
    path: &Path,
    vault: Arc<Vault>,
    base: &AppBinaryDocumentDto,
    bytes: &[u8],
    intent: &str,
) -> Result<AppBinaryDocumentDto> {
    let client = Client::open(path.to_str().ok_or("database path")?.into(), "en".into())
        .map_err(|_| "cold open failed")?;
    require(
        client.unlock(vault).map_err(|_| "cold vault failed")?,
        "cold vault refused",
    )?;
    let retry = commit(&client, base, bytes, intent)?;
    require(
        retry.version == 2
            && retry.current_version == 3
            && retry.sha256 == sha(bytes)
            && retry.data_base64 == STANDARD.encode(bytes),
        "cold ACK observation changed original",
    )?;
    let selected = client
        .app_binary_read(base.item_id.clone(), Some(1), Some(base.sha256.clone()))
        .map_err(|_| "selected source export failed")?;
    require(
        selected.version == 1
            && selected.current_version == 3
            && selected.data_base64 == base.data_base64,
        "selected source bytes changed",
    )?;
    require(
        client
            .app_binary_read(base.item_id.clone(), Some(1), Some(sha(bytes)))
            .is_err(),
        "foreign selected hash admitted",
    )?;
    require(
        client
            .app_binary_save(
                base.item_id.clone(),
                base.version,
                base.sha256.clone(),
                base.data_base64.clone(),
                "stale-new-id".into(),
                intent.into(),
            )
            .is_err(),
        "stale CAS admitted",
    )?;
    require(
        client
            .app_binary_read(base.item_id.clone(), None, None)
            .map_err(|_| "current read failed")?
            .version
            == 3,
        "recovery created another version",
    )?;
    Ok(retry)
}
async fn check(paths: &[PathBuf]) -> Result<()> {
    let component = read(&paths[0], 64 * 1024 * 1024)?;
    let seed = read(&paths[1], LIMIT)?;
    require(!paths[2].exists(), "output already exists")?;
    let started = Instant::now();
    let worker = load(&component)?;
    let compile_ms = started.elapsed().as_millis();
    std::fs::create_dir(&paths[2]).map_err(|_| "output unavailable")?;
    let path = paths[2].join("device.sqlite");
    let (client, vault, base) = create(&path, &seed)?;
    let (bytes, intent) = proposal(&worker, &base, &seed).await?;
    let accepted = commit(&client, &base, &bytes, &intent)?;
    require(accepted.version == 2, "wrong accepted version")?;
    client
        .app_binary_save(
            base.item_id.clone(),
            accepted.version,
            accepted.sha256,
            base.data_base64.clone(),
            "later-local-edit".into(),
            sha(b"later exact seed"),
        )
        .map_err(|_| "later canonical save failed")?;
    drop(client);
    let retry = recover(&path, vault, &base, &bytes, &intent)?;
    std::fs::write(paths[2].join("accepted.docx"), &bytes).map_err(|_| "output unavailable")?;
    let receipt = json!({"status":"PASS","scope":"Actual signed Word worker -> ordinary local canonical encrypted Item/FileDoc/CAS receipts -> later version -> cold original ACK observation and selected source bytes. Synthetic local account; not certified agent approval, browser/catalogue or native acceptance.","componentSha256":sha(&component),"seedSha256":sha(&seed),"acceptedSha256":sha(&bytes),"workerCompileMs":compile_ms,"acceptedVersion":retry.version,"currentVersion":retry.current_version,"xmlLossyInputsRefused":true,"foreignEngineRefused":true,"formatIsolation":true});
    std::fs::write(
        paths[2].join("result.json"),
        serde_json::to_vec_pretty(&receipt).map_err(|_| "receipt invalid")?,
    )
    .map_err(|_| "receipt unavailable")?;
    println!("WordCraft actual canonical worker + encrypted FileDoc + CAS + cold ACK/history PASS");
    Ok(())
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    require(
        paths.len() == 3 && paths.iter().all(|path| path.is_absolute()),
        "Usage: wordcraft_canonical_check ABS_COMPONENT ABS_REAL_SEED ABS_NEW_OUTPUT",
    )?;
    check(&paths).await
}

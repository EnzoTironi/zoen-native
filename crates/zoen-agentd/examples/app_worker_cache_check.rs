//! Opt-in offline measurement using the genuine signed WordCraft WASIp2 worker.
//! Measures actual disk hits in fresh Engines/processes; no account/provider/relay.
#![deny(
    clippy::pedantic,
    clippy::perf,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
use bytes::Bytes;
use ed25519_dalek::SigningKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
use zoen_agentd::{
    deckcraft::DeckcraftWorker,
    wasm::{CompilationCache, Stopped, WasmLimits, WasmTier},
    Needs, ToolManifest,
};

type Result<T> = std::result::Result<T, String>;
const REVISION: &str = "d1b0bfc3f9dd83126b162a42b667facecb003b7f";
const TEXT: &str = "Cache preserves actual Word bytes\nOlá 日本語 👩🏽‍💻\n";
const MEMORY: usize = 256 * 1024 * 1024;
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "input unavailable")?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "input unreadable")?;
    require(!bytes.is_empty() && bytes.len() <= limit, "input bound")?;
    Ok(bytes)
}
fn request(seed: &[u8]) -> Value {
    let scope = json!({"artifactId":"cache-proof","actorId":"synthetic-owner","actorKind":"agent"});
    let head = json!({"version":"1","sha256":sha(seed)});
    json!({"schema":1,"open":{"scope":scope,"head":head},"command":{"mutation":{"scope":scope,"base":head,"mutationId":"cache-proof-edit"},"action":{"command":"text.replace","params":{"text":TEXT}}}})
}
fn frame(metadata: &Value, document: &[u8]) -> Result<Bytes> {
    let metadata = serde_json::to_vec(metadata).map_err(|_| "metadata")?;
    let length = u32::try_from(metadata.len()).map_err(|_| "metadata bound")?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(&metadata);
    bytes.extend_from_slice(document);
    Ok(bytes.into())
}
async fn phase(
    component_path: &Path,
    seed_path: &Path,
    directory: &Path,
    warm: bool,
    limits: bool,
) -> Result<Value> {
    let component = read(component_path, 64 * 1024 * 1024)?;
    let seed = read(seed_path, 512 * 1024)?;
    let publisher = SigningKey::from_bytes(&[29; 32]); // Existing synthetic Word fixture publisher.
    let signed = ToolManifest {
        id: "wordcraft-canonical-v1".into(),
        version: REVISION.into(),
        summary: "Actual Word cache measurement".into(),
        needs: Needs::Wasm,
        egress: vec![],
        secrets: vec![],
        component_sha256: Some(sha(&component)),
    }
    .sign(&publisher);
    let cache = CompilationCache::private_directory(directory).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let worker = DeckcraftWorker::load_for_cached(
        "wordcraft",
        vec![publisher.verifying_key()],
        &signed,
        &component,
        &cache,
    )?;
    let load_ms = started.elapsed().as_millis();
    let stats = cache.stats();
    require(
        stats.hits == usize::from(warm) && stats.misses == usize::from(!warm),
        "actual cache hit/miss did not match phase",
    )?;
    let request = request(&seed);
    let started = Instant::now();
    let edited = worker.run_for("wordcraft", &request, &seed).await?;
    let edit_ms = started.elapsed().as_millis();
    require(
        !edited.bytes.is_empty() && edited.metadata["sha256"] == sha(&edited.bytes),
        "actual edit/hash",
    )?;
    require(
        edited.metadata["operation"]["action"] == request["command"]["action"],
        "actual upstream command",
    )?;
    let mut reopen = request.clone();
    reopen["command"] = Value::Null;
    reopen["open"]["head"] = json!({"version":"2","sha256":sha(&edited.bytes)});
    let opened = worker.run_for("wordcraft", &reopen, &edited.bytes).await?;
    require(
        opened.bytes.is_empty()
            && opened.metadata["text"] == TEXT
            && opened.metadata["head"] == reopen["open"]["head"],
        "actual exact reopen",
    )?;
    drop(worker);
    if limits {
        let tier =
            WasmTier::with_compilation_cache(vec![publisher.verifying_key()], 1, MEMORY, &cache)
                .map_err(|e| e.to_string())?;
        let tool = tier.load(&signed, &component).map_err(|e| e.to_string())?;
        let input = frame(&request, &seed)?;
        let fuel = tier
            .run(
                &tool,
                input.clone(),
                WasmLimits {
                    fuel: 1,
                    wall: Duration::from_secs(5),
                    memory_bytes: MEMORY,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        require(
            fuel.stopped == Some(Stopped::OutOfFuel),
            "cached code bypassed caller fuel",
        )?;
        let memory = tier
            .run(
                &tool,
                input,
                WasmLimits {
                    fuel: 2_000_000_000,
                    wall: Duration::from_secs(5),
                    memory_bytes: 1,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        require(
            memory.stopped == Some(Stopped::OutOfMemory),
            "cached code bypassed caller memory",
        )?;
    }
    Ok(
        json!({"loadMs":load_ms,"editMs":edit_ms,"hits":stats.hits,"misses":stats.misses,"componentSha256":sha(&component),"seedSha256":sha(&seed),"resultSha256":sha(&edited.bytes),"resultBytes":edited.bytes.len(),"exactReopen":true,"callerFuelAndMemoryRefused":limits}),
    )
}
fn write(path: &Path, value: &Value) -> Result<()> {
    std::fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|_| "receipt encoding")?,
    )
    .map_err(|_| "receipt unavailable".into())
}
async fn check() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mode = args.next().ok_or("mode required")?;
    let paths: Vec<PathBuf> = args.map(PathBuf::from).collect();
    require(
        paths.len() == 4 && paths.iter().all(|p| p.is_absolute()),
        "usage: measure|warm-process ABS_COMPONENT ABS_SEED ABS_PRIVATE_CACHE ABS_OUTPUT",
    )?;
    let [component, seed, directory, output] = paths.as_slice() else {
        return Err("arguments".into());
    };
    require(!output.exists(), "output already exists")?;
    if mode == "warm-process" {
        return write(
            output,
            &phase(component, seed, directory, true, false).await?,
        );
    }
    require(mode == "measure", "unknown mode")?;
    require(
        std::fs::read_dir(directory)
            .map_err(|_| "cache unavailable")?
            .next()
            .is_none(),
        "cold cache must be empty",
    )?;
    let cold_output = output.with_extension("cold.json");
    let warm_output = output.with_extension("new-engine.json");
    let child_output = output.with_extension("new-process.json");
    require(
        [&cold_output, &warm_output, &child_output]
            .iter()
            .all(|path| !path.exists()),
        "phase output already exists",
    )?;
    eprintln!("cache measurement: cold miss");
    let cold = phase(component, seed, directory, false, false).await?;
    write(&cold_output, &cold)?;
    eprintln!("cache measurement: new Engine");
    let warm = phase(component, seed, directory, true, true).await?;
    write(&warm_output, &warm)?;
    eprintln!("cache measurement: new process");
    let status = Command::new(std::env::current_exe().map_err(|_| "executable unavailable")?)
        .arg("warm-process")
        .arg(component)
        .arg(seed)
        .arg(directory)
        .arg(&child_output)
        .env_clear()
        .status()
        .map_err(|_| "new process unavailable")?;
    require(status.success(), "new process failed")?;
    let process: Value = serde_json::from_slice(&read(&child_output, 16 * 1024)?)
        .map_err(|_| "new process receipt")?;
    require(
        cold["resultSha256"] == warm["resultSha256"]
            && warm["resultSha256"] == process["resultSha256"],
        "cold/warm/new-process changed actual result",
    )?;
    write(
        output,
        &json!({"status":"PASS","scope":"Actual signed Word WASIp2 worker and operator cache; no canonical account/provider/native acceptance","cold":cold,"newEngine":warm,"newProcess":process,"limits":{"maxConcurrent":1,"wallMs":5000,"fuel":2_000_000_000_u64,"memoryBytes":MEMORY},"cacheTrust":"private operator native-code state; soft disk limits, no hostile decompression bound"}),
    )
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    if let Err(error) = check().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

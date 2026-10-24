//! Explicit offline qualification of actual pinned components in the existing WASI tier.
//! Synthetic supplied Scope is not certified-agent or canonical persistence acceptance.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    clippy::perf
)]
use bytes::Bytes;
use ed25519_dalek::SigningKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zoen_agentd::wasm::{Stopped, WasmLimits, WasmOutput, WasmTier, WasmTool};
use zoen_agentd::{Needs, ToolManifest};

type Result<T> = std::result::Result<T, String>;
const REVISION: &str = "d1b0bfc3f9dd83126b162a42b667facecb003b7f";
const DOCUMENT: usize = 512 * 1024;
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
fn read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "input unavailable")?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "input unreadable")?;
    require(
        !bytes.is_empty() && bytes.len() <= limit,
        "input exceeds bound",
    )?;
    Ok(bytes)
}
fn manifest(id: &str, bytes: &[u8]) -> ToolManifest {
    ToolManifest {
        id: id.into(),
        version: REVISION.into(),
        summary: "Synthetic WordCraft component qualification".into(),
        needs: Needs::Wasm,
        egress: vec![],
        secrets: vec![],
        component_sha256: Some(sha(bytes)),
    }
}
fn frame(metadata: &Value, bytes: &[u8]) -> Result<Bytes> {
    let meta = serde_json::to_vec(metadata).map_err(|_| "invalid metadata")?;
    require(
        meta.len() <= 64 * 1024 && bytes.len() <= DOCUMENT,
        "frame bound",
    )?;
    let length = u32::try_from(meta.len()).map_err(|_| "metadata bound")?;
    let mut result = length.to_le_bytes().to_vec();
    result.extend(meta);
    result.extend_from_slice(bytes);
    Ok(Bytes::from(result))
}
fn decode(bytes: &[u8]) -> Result<(Value, Vec<u8>)> {
    require(bytes.len() <= 4 + 64 * 1024 + DOCUMENT, "response bound")?;
    let prefix = bytes.get(..4).ok_or("missing frame")?;
    let length = usize::try_from(u32::from_le_bytes(
        prefix.try_into().map_err(|_| "invalid prefix")?,
    ))
    .map_err(|_| "invalid metadata length")?;
    require(length > 0 && length <= 64 * 1024, "metadata size")?;
    let metadata = serde_json::from_slice(bytes.get(4..4 + length).ok_or("truncated frame")?)
        .map_err(|_| "invalid response")?;
    Ok((
        metadata,
        bytes.get(4 + length..).ok_or("truncated body")?.to_vec(),
    ))
}
fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| "system clock unavailable".into())
}
fn limits() -> WasmLimits {
    WasmLimits {
        fuel: 2_000_000_000,
        wall: Duration::from_secs(5),
        memory_bytes: 256 * 1024 * 1024,
    }
}
fn completed(output: &WasmOutput) -> Result<()> {
    match output.stopped.as_ref() {
        None => Ok(()),
        Some(Stopped::OutOfFuel) => Err("component exhausted fuel".into()),
        Some(Stopped::OutOfTime) => Err("component exceeded execution deadline".into()),
        Some(Stopped::OutOfMemory) => Err("component exceeded memory limit".into()),
        Some(Stopped::Failed(_)) => Err("component failed during execution".into()),
    }
}
async fn call(
    tier: &WasmTier,
    tool: &WasmTool,
    request: &Value,
    bytes: &[u8],
    cases: &mut Vec<Value>,
    name: &str,
) -> Result<(Value, Vec<u8>)> {
    let output = tier
        .run(tool, frame(request, bytes)?, limits())
        .await
        .map_err(|_| "runtime error")?;
    completed(&output)?;
    let (envelope, body) = decode(&output.stdout)?;
    require(
        envelope.as_object().is_some_and(|o| o.len() == 2),
        "envelope fields",
    )?;
    if envelope["ok"] == true {
        require(
            envelope["metadata"]["engineRevision"] == REVISION,
            "engine revision",
        )?;
    } else {
        require(
            envelope["ok"] == false && envelope["error"].is_string() && body.is_empty(),
            "invalid refusal",
        )?;
    }
    cases.push(json!({"case":name,"fuel":output.fuel_used,"elapsedMs":output.elapsed.as_millis(),"ok":envelope["ok"],"byteLength":body.len(),"sha256":sha(&body)}));
    Ok((envelope, body))
}

struct Loaded {
    tier: WasmTier,
    worker: WasmTool,
    clock: WasmTool,
    compile_ms: u128,
}
fn load(component: &[u8], probe: &[u8]) -> Result<Loaded> {
    let key = SigningKey::from_bytes(&[42; 32]); // Synthetic fixture publisher, never a deployment key.
    let tier = WasmTier::new(vec![key.verifying_key()], 1, 256 * 1024 * 1024)
        .map_err(|_| "tier unavailable")?;
    let signed = manifest("wordcraft-qualification", component).sign(&key);
    let mut swapped = component.to_vec();
    *swapped.first_mut().ok_or("empty component")? ^= 1;
    require(
        tier.load(&signed, &swapped).is_err(),
        "wrong component accepted",
    )?;
    require(
        tier.load(
            &manifest("wordcraft-qualification", component)
                .sign(&SigningKey::from_bytes(&[43; 32])),
            component,
        )
        .is_err(),
        "foreign publisher accepted",
    )?;
    let started = Instant::now();
    let tool = tier
        .load(&signed, component)
        .map_err(|_| "actual worker cannot load")?;
    let worker_compile_ms = started.elapsed().as_millis();
    let clock_tool = tier
        .load(&manifest("wordcraft-clock-probe", probe).sign(&key), probe)
        .map_err(|_| "actual clock probe cannot load")?;
    Ok(Loaded {
        tier,
        worker: tool,
        clock: clock_tool,
        compile_ms: worker_compile_ms,
    })
}
async fn run_clock(tier: &WasmTier, tool: &WasmTool) -> Result<(Value, u64)> {
    let before = now()?;
    let clock_output = tier
        .run(tool, Bytes::new(), limits())
        .await
        .map_err(|_| "clock probe runtime error")?;
    completed(&clock_output)?;
    let after = now()?;
    let clock: Value =
        serde_json::from_slice(&clock_output.stdout).map_err(|_| "invalid clock output")?;
    let layout_before = clock["layoutBeforeMs"]
        .as_f64()
        .ok_or("missing actual layout clock")?;
    let layout_after = clock["layoutAfterMs"]
        .as_f64()
        .ok_or("missing actual layout clock after wait")?;
    require(
        layout_before.is_finite()
            && layout_after.is_finite()
            && layout_after >= layout_before + 15.0,
        "layout monotonic clock did not progress",
    )?;
    let seconds = clock["engineUnixSeconds"]
        .as_u64()
        .ok_or("missing engine clock")?;
    require(
        before <= seconds && seconds <= after,
        "engine clock is not the actual WASI wall clock",
    )?;
    let format_date = clock["formatChangeDate"]
        .as_str()
        .ok_or("missing formats clock")?;
    // The probe also executes the ODT writer's blank-revision-date branch. Compare
    // its actual timestamp to host UTC, rather than accepting any nonzero/fixed date.
    let seconds_after = clock["engineUnixAfter"]
        .as_u64()
        .ok_or("missing engine clock after export")?;
    require(
        seconds <= seconds_after && seconds_after <= after,
        "clock after export left actual wall-clock range",
    )?;
    let start_date = clock["engineIsoBefore"]
        .as_str()
        .ok_or("missing engine ISO before")?;
    let end_date = clock["engineIsoAfter"]
        .as_str()
        .ok_or("missing engine ISO after")?;
    require(
        start_date.len() == 20
            && end_date.len() == 20
            && start_date <= format_date
            && format_date <= end_date,
        "formats clock is not the actual WASI wall clock",
    )?;
    Ok((clock, clock_output.fuel_used))
}
async fn negatives(
    tier: &WasmTier,
    tool: &WasmTool,
    request: &Value,
    seed: &[u8],
    cases: &mut Vec<Value>,
) -> Result<()> {
    let open = &request["open"];
    let head = &open["head"];
    let scope = &open["scope"];
    let command = &request["command"];
    let action = &command["action"];
    for (name, request) in [
        (
            "unknown-command",
            json!({"schema":1,"open":open,"command":{"mutation":command["mutation"],"action":{"command":"file.open","params":{"path":"forbidden"}}}}),
        ),
        (
            "foreign-scope",
            json!({"schema":1,"open":open,"command":{"mutation":{"scope":{"artifactId":"foreign","actorId":"synthetic-owner","actorKind":"agent"},"base":head,"mutationId":"foreign"},"action":action}}),
        ),
        (
            "stale-base",
            json!({"schema":1,"open":open,"command":{"mutation":{"scope":scope,"base":{"version":"0","sha256":sha(seed)},"mutationId":"stale"},"action":action}}),
        ),
        (
            "bad-hash",
            json!({"schema":1,"open":{"scope":scope,"head":{"version":"1","sha256":"0".repeat(64)}},"command":null}),
        ),
    ] {
        let (refused, body) = call(tier, tool, &request, seed, cases, name).await?;
        require(
            refused["ok"] == false && body.is_empty(),
            "negative case produced a proposal",
        )?;
    }
    Ok(())
}
async fn edits(tier: &WasmTier, tool: &WasmTool, seed: &[u8]) -> Result<(Vec<Value>, Vec<u8>)> {
    let scope = json!({"artifactId":"synthetic-wordcraft-component","actorId":"synthetic-owner","actorKind":"agent"});
    let head = json!({"version":"1","sha256":sha(seed)});
    let open = json!({"scope":scope,"head":head});
    let mut cases = Vec::new();
    let (initial, body) = call(
        tier,
        tool,
        &json!({"schema":1,"open":open,"command":null}),
        seed,
        &mut cases,
        "inspect",
    )
    .await?;
    require(
        initial["ok"] == true && initial["metadata"]["text"] == "" && body.is_empty(),
        "seed inspection failed",
    )?;
    let text = "WASI real\nOlá 日本語 👩🏽‍💻\ttexto\n";
    let action = json!({"command":"text.replace","params":{"text":text}});
    let command = json!({"mutation":{"scope":scope,"base":head,"mutationId":"actual-wasi-edit"},"action":action});
    let request = json!({"schema":1,"open":open,"command":command});
    let (proposal, bytes) = call(tier, tool, &request, seed, &mut cases, "edit").await?;
    require(
        proposal["ok"] == true
            && !bytes.is_empty()
            && proposal["metadata"]["sha256"] == sha(&bytes)
            && proposal["metadata"]["operation"]["action"] == action
            && proposal["metadata"]["acceptedHead"].is_null(),
        "invalid actual proposal",
    )?;
    let (retry, retried) = call(tier, tool, &request, seed, &mut cases, "retry").await?;
    require(
        retry == proposal && retried == bytes,
        "retry changed original proposal",
    )?;
    let cold_head = json!({"version":"2","sha256":sha(&bytes)});
    let (cold, body) = call(
        tier,
        tool,
        &json!({"schema":1,"open":{"scope":scope,"head":cold_head},"command":null}),
        &bytes,
        &mut cases,
        "cold-reopen",
    )
    .await?;
    require(
        cold["ok"] == true
            && cold["metadata"]["text"] == text
            && cold["metadata"]["head"] == cold_head
            && body.is_empty(),
        "cold bytes changed",
    )?;
    negatives(tier, tool, &request, seed, &mut cases).await?;
    Ok((cases, bytes))
}
async fn check(paths: &[PathBuf]) -> Result<()> {
    let component = read(&paths[0], 64 * 1024 * 1024)?;
    let probe = read(&paths[1], 64 * 1024 * 1024)?;
    let seed = read(&paths[2], DOCUMENT)?;
    require(!paths[3].exists(), "output already exists")?;
    let loaded = load(&component, &probe)?;
    let (clock, clock_fuel) = run_clock(&loaded.tier, &loaded.clock).await?;
    let (cases, bytes) = edits(&loaded.tier, &loaded.worker, &seed).await?;
    std::fs::create_dir(&paths[3]).map_err(|_| "output directory unavailable")?;
    std::fs::write(paths[3].join("proposal.docx"), &bytes).map_err(|_| "output unavailable")?;
    let result = json!({"status":"PASS","scope":"Actual pinned WordCraft components in the existing signed WasmTier, supplied synthetic Scope. No canonical authorization/persistence or browser/iPhone claim.","componentSha256":sha(&component),"probeSha256":sha(&probe),"seedSha256":sha(&seed),"proposalSha256":sha(&bytes),"workerCompileMs":loaded.compile_ms,"clock":clock,"clockFuel":clock_fuel,"cases":cases,"publisherAndHashNegatives":true,"limits":{"fuel":2_000_000_000u64,"wallMs":5000,"memoryBytes":256*1024*1024}});
    std::fs::write(
        paths[3].join("result.json"),
        serde_json::to_vec_pretty(&result).map_err(|_| "receipt invalid")?,
    )
    .map_err(|_| "receipt unavailable")?;
    println!("WordCraft actual WASI component + three real clocks + 8 framed calls PASS");
    Ok(())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    require(paths.len()==4 && paths.iter().all(|path|path.is_absolute()),"Usage: wordcraft_component_check ABS_COMPONENT ABS_CLOCK_PROBE ABS_ACTUAL_SEED_DOCX ABS_NEW_OUTPUT")?;
    check(&paths).await
}

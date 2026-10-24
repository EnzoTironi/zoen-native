//! Actual offline Photo component through the production signed worker dispatch.
//! Synthetic scope is not certified-agent or canonical-storage acceptance.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use ed25519_dalek::SigningKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
use zoen_agentd::{deckcraft::DeckcraftWorker, Needs, ToolManifest};

type R<T = ()> = Result<T, Box<dyn std::error::Error>>;
const REVISION: &str = "9b9643764f3fb77d9f0e48d7510983ae50621e11";
fn require(value: bool, reason: &str) -> R {
    if value {
        Ok(())
    } else {
        Err(reason.into())
    }
}
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn read(path: &Path, max: usize) -> R<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(u64::try_from(max)? + 1)
        .read_to_end(&mut bytes)?;
    require(
        !bytes.is_empty() && bytes.len() <= max,
        "input exceeds limit",
    )?;
    Ok(bytes)
}
fn manifest(component: &[u8]) -> ToolManifest {
    ToolManifest {
        id: "photocraft-canonical-v1".into(),
        version: REVISION.into(),
        summary: "Offline Photo component qualification with synthetic scope".into(),
        needs: Needs::Wasm,
        egress: vec![],
        secrets: vec![],
        component_sha256: Some(sha(component)),
    }
}
async fn qualify(component: &[u8], source: &[u8], expected: &[u8], output: &Path) -> R<Value> {
    let key = SigningKey::from_bytes(&[42; 32]);
    let signed = manifest(component).sign(&key);
    let worker =
        DeckcraftWorker::load_for("photocraft", vec![key.verifying_key()], &signed, component)?;
    require(
        worker.supports("photocraft") && !worker.supports("wordcraft"),
        "worker dispatch",
    )?;
    require(
        DeckcraftWorker::load_for("wordcraft", vec![key.verifying_key()], &signed, component)
            .is_err(),
        "cross-engine manifest accepted",
    )?;
    let other = SigningKey::from_bytes(&[43; 32]);
    require(
        DeckcraftWorker::load_for(
            "photocraft",
            vec![other.verifying_key()],
            &signed,
            component,
        )
        .is_err(),
        "foreign publisher accepted",
    )?;
    let mut foreign = manifest(component);
    foreign.version = "foreign".into();
    require(
        DeckcraftWorker::load_for(
            "photocraft",
            vec![key.verifying_key()],
            &foreign.sign(&key),
            component,
        )
        .is_err(),
        "foreign engine accepted",
    )?;
    let mut corrupted = component.to_vec();
    let last = corrupted.last_mut().ok_or("empty component")?;
    *last ^= 1;
    require(
        DeckcraftWorker::load_for("photocraft", vec![key.verifying_key()], &signed, &corrupted)
            .is_err(),
        "modified code accepted",
    )?;
    let scope = json!({"artifactId":"synthetic-photo-component","actorId":"synthetic-owner","actorKind":"agent"});
    let head = json!({"version":"1","sha256":sha(source)});
    let request = json!({"schema":1,"open":{"scope":scope,"head":head},"command":null});
    let inspected = worker.run_for("photocraft", &request, source).await?;
    require(
        inspected.bytes.is_empty()
            && inspected.metadata["scope"] == scope
            && inspected.metadata["head"] == head
            && inspected.metadata["width"] == 640
            && inspected.metadata["height"] == 480
            && inspected.metadata["layers"] == 1,
        "actual native source inspection mismatch",
    )?;
    let mut changed = request.clone();
    changed["command"] = json!({"mutation":{"scope":scope,"base":head,"mutationId":"rotate_real"},"action":"rotate_clockwise"});
    let result = worker.run_for("photocraft", &changed, source).await?;
    require(
        result.bytes == expected
            && result.metadata["sha256"] == sha(expected)
            && result.metadata["base"] == head
            && result.metadata["scope"] == scope
            && result.metadata["mutationId"] == "rotate_real",
        "WASI and native transform differ",
    )?;
    let repeated = worker.run_for("photocraft", &changed, source).await?;
    require(
        result.bytes == repeated.bytes && result.metadata == repeated.metadata,
        "fresh worker retry changed proposal",
    )?;
    let mut corrupt_head = request.clone();
    corrupt_head["open"]["head"]["sha256"] = json!("0".repeat(64));
    require(
        worker
            .run_for("photocraft", &corrupt_head, source)
            .await
            .is_err(),
        "wrong source hash accepted",
    )?;
    let mut foreign_scope = changed.clone();
    foreign_scope["command"]["mutation"]["scope"]["actorId"] = json!("another-owner");
    require(
        worker
            .run_for("photocraft", &foreign_scope, source)
            .await
            .is_err(),
        "scope substitution accepted",
    )?;
    let mut unknown = changed;
    unknown["command"]["action"] = json!("image.load");
    require(
        worker
            .run_for("photocraft", &unknown, source)
            .await
            .is_err(),
        "arbitrary command accepted",
    )?;
    let reopen = json!({"schema":1,"open":{"scope":scope,"head":{"version":"2","sha256":sha(expected)}},"command":null});
    let reopened = worker.run_for("photocraft", &reopen, &result.bytes).await?;
    require(
        reopened.metadata["width"] == 480 && reopened.metadata["height"] == 640,
        "fresh component did not reopen saved output",
    )?;
    std::fs::write(output.join("actual-wasi.pcraft"), &result.bytes)?;
    Ok(
        json!({"engineRevision":REVISION,"componentSha256":sha(component),"sourceSha256":sha(source),
        "outputSha256":sha(&result.bytes),"outputBytes":result.bytes.len(),
        "passed":["production-dispatch","cross-engine-refused","foreign-publisher-refused",
            "foreign-revision-refused","modified-code-refused","actual-native-source-inspected",
            "native-wasi-transform-byte-equality","fresh-worker-retry-equal","wrong-source-refused",
            "scope-substitution-refused","arbitrary-command-refused","fresh-component-reopen"],
        "scope":"Actual signed WASIp2 worker with synthetic scope. No canonical authorization, persistence, browser or iPhone claim."}),
    )
}
#[tokio::main]
async fn main() -> R {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    require(args.len()==4 && args.iter().all(|p| p.is_absolute()),
        "Usage: photocraft_component_check ABS_COMPONENT ABS_SOURCE ABS_NATIVE_EXPECTED ABS_NEW_OUTPUT")?;
    let component = read(&args[0], 64 << 20)?;
    let source = read(&args[1], 512 << 10)?;
    let expected = read(&args[2], 512 << 10)?;
    std::fs::create_dir(&args[3])?;
    let result = qualify(&component, &source, &expected, &args[3]).await?;
    let json = serde_json::to_string_pretty(&result)?;
    std::fs::write(args[3].join("result.json"), &json)?;
    println!("{json}");
    Ok(())
}

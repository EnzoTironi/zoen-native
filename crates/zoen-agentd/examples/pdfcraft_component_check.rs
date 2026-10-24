//! Actual pinned PDF component in the production WASI tier, with a synthetic scope.
//! This does not certify a Core agent, grant access or prove canonical persistence.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use bytes::Bytes;
use ed25519_dalek::SigningKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use zoen_agentd::{
    wasm::{WasmLimits, WasmTier, WasmTool},
    Needs, ToolManifest,
};

type R<T> = Result<T, Box<dyn std::error::Error>>;
const REVISION: &str = "d445410807e7eefbe5b96f9662ab963ae8dd2a3a";
const DOCUMENT: usize = 512 << 10;
const METADATA: usize = 64 << 10;
fn require(ok: bool, message: &str) -> R<()> {
    if ok {
        Ok(())
    } else {
        Err(message.to_owned().into())
    }
}
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn read(path: &Path, bound: usize) -> R<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(u64::try_from(bound)? + 1)
        .read_to_end(&mut bytes)?;
    require(!bytes.is_empty() && bytes.len() <= bound, "input bound")?;
    Ok(bytes)
}
fn frame(metadata: &Value, document: &[u8]) -> R<Bytes> {
    let metadata = serde_json::to_vec(metadata)?;
    require(
        !metadata.is_empty() && metadata.len() <= METADATA && document.len() <= DOCUMENT,
        "request frame bound",
    )?;
    let mut bytes = u32::try_from(metadata.len())?.to_le_bytes().to_vec();
    bytes.extend(metadata);
    bytes.extend_from_slice(document);
    Ok(Bytes::from(bytes))
}
fn decode(bytes: &[u8]) -> R<(Value, Vec<u8>)> {
    require(bytes.len() <= 1 << 20, "response frame bound")?;
    let prefix = bytes.get(..4).ok_or("missing response prefix")?;
    let length = usize::try_from(u32::from_le_bytes(prefix.try_into()?))?;
    require(length > 0 && length <= METADATA, "response metadata bound")?;
    let metadata = serde_json::from_slice(bytes.get(4..4 + length).ok_or("truncated metadata")?)?;
    Ok((
        metadata,
        bytes.get(4 + length..).ok_or("truncated body")?.to_vec(),
    ))
}
fn manifest(component: &[u8]) -> ToolManifest {
    ToolManifest {
        id: "pdfcraft-canonical-v1".into(),
        version: REVISION.into(),
        summary: "Actual offline PDF component qualification with synthetic scope".into(),
        needs: Needs::Wasm,
        egress: vec![],
        secrets: vec![],
        component_sha256: Some(sha(component)),
    }
}
async fn call(
    tier: &WasmTier,
    tool: &WasmTool,
    metadata: &Value,
    document: &[u8],
    output: &Path,
    cases: &mut Vec<Value>,
    name: &str,
) -> R<(Value, Vec<u8>)> {
    let response = tier
        .run(
            tool,
            frame(metadata, document)?,
            WasmLimits {
                fuel: 2_000_000_000,
                wall: Duration::from_secs(5),
                memory_bytes: 256 << 20,
            },
        )
        .await?;
    if let Some(stopped) = response.stopped {
        return Err(format!("actual component stopped: {stopped:?}").into());
    }
    let (envelope, body) = decode(&response.stdout)?;
    if envelope["ok"] == true {
        require(
            envelope["metadata"]["engineRevision"] == REVISION,
            "engine revision changed",
        )?;
    } else {
        require(
            envelope["ok"] == false && envelope["error"].is_string() && body.is_empty(),
            "invalid refusal",
        )?;
    }
    cases.push(
        json!({"case":name,"ok":envelope["ok"],"fuel":response.fuel_used,
        "elapsedMs":response.elapsed.as_millis(),"bytes":body.len(),"sha256":sha(&body)}),
    );
    std::fs::write(output.join("cases.json"), serde_json::to_vec_pretty(cases)?)?;
    Ok((envelope, body))
}
async fn qualify(component: &[u8], document: &[u8], output: &Path) -> R<Value> {
    let key = SigningKey::from_bytes(&[42; 32]); // Synthetic test publisher, not a deployment key.
    let tier = WasmTier::new(vec![key.verifying_key()], 1, 256 << 20)?;
    let signed = manifest(component).sign(&key);
    let mut corrupted = component.to_vec();
    *corrupted.first_mut().ok_or("empty component")? ^= 1;
    require(
        tier.load(&signed, &corrupted).is_err(),
        "modified component accepted",
    )?;
    let start = Instant::now();
    let tool = tier.load(&signed, component)?;
    let compile_ms = start.elapsed().as_millis();
    let scope = json!({"artifactId":"synthetic-pdf-component","actorId":"synthetic-reviewer","actorKind":"agent"});
    let head = json!({"version":"1","sha256":sha(document)});
    let open = json!({"scope":scope,"head":head});
    let request = json!({"schema":1,"open":open,"command":null});
    let mut cases = vec![];
    let (inspected, body) = call(
        &tier,
        &tool,
        &request,
        document,
        output,
        &mut cases,
        "read-real-pdf",
    )
    .await?;
    require(
        inspected["ok"] == true
            && inspected["metadata"]["scope"] == scope
            && inspected["metadata"]["head"] == head
            && inspected["metadata"]["pageCount"] == 2
            && body.is_empty(),
        "actual PDF inspection mismatch",
    )?;
    let action = json!({"op":"rotate_pages","pages":[1],"degrees":90});
    let command =
        json!({"mutation":{"scope":scope,"base":head,"mutationId":"rotate-real"},"action":action});
    let changed = json!({"schema":1,"open":open,"command":command});
    let (proposal, bytes) = call(
        &tier,
        &tool,
        &changed,
        document,
        output,
        &mut cases,
        "rotate-real-page",
    )
    .await?;
    std::fs::write(output.join("proposal.pdf"), &bytes)?;
    std::fs::write(
        output.join("proposal-metadata.json"),
        serde_json::to_vec_pretty(&proposal)?,
    )?;
    require(
        proposal["ok"] == true
            && !bytes.is_empty()
            && proposal["metadata"]["scope"] == scope
            && proposal["metadata"]["base"] == head
            && proposal["metadata"]["mutationId"] == "rotate-real"
            && proposal["metadata"]["sha256"] == sha(&bytes)
            && proposal["metadata"]["operation"] == json!({"kind":"command","action":action}),
        "actual proposal does not meet the approved-command consumer contract",
    )?;
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let (retry, repeated) = call(
        &tier,
        &tool,
        &changed,
        document,
        output,
        &mut cases,
        "fresh-store-retry",
    )
    .await?;
    require(
        retry == proposal && repeated == bytes,
        "fresh worker changed retained source intent",
    )?;
    let cold_head = json!({"version":"2","sha256":sha(&bytes)});
    let cold = json!({"schema":1,"open":{"scope":scope,"head":cold_head},"command":null});
    let (reopened, body) = call(
        &tier,
        &tool,
        &cold,
        &bytes,
        output,
        &mut cases,
        "cold-reopen",
    )
    .await?;
    require(
        reopened["ok"] == true && reopened["metadata"]["head"] == cold_head && body.is_empty(),
        "cold recovery mismatch",
    )?;
    for (name, request) in [
        (
            "wrong-hash",
            json!({"schema":1,"open":{"scope":scope,"head":{"version":"1","sha256":"0".repeat(64)}},"command":null}),
        ),
        (
            "foreign-mutation-scope",
            json!({"schema":1,"open":open,"command":{"mutation":{"scope":{"artifactId":"foreign","actorId":"synthetic-reviewer","actorKind":"agent"},"base":head,"mutationId":"foreign"},"action":action}}),
        ),
        (
            "unknown-operation",
            json!({"schema":1,"open":open,"command":{"mutation":{"scope":scope,"base":head,"mutationId":"unknown"},"action":{"op":"file_read","path":"unavailable"}}}),
        ),
    ] {
        let (refusal, body) =
            call(&tier, &tool, &request, document, output, &mut cases, name).await?;
        require(
            refusal["ok"] == false && body.is_empty(),
            "negative case produced document bytes",
        )?;
    }
    Ok(
        json!({"status":"PASS","engineRevision":REVISION,"componentSha256":sha(component),
        "componentBytes":component.len(),"sourceSha256":sha(document),"outputSha256":sha(&bytes),
        "compileMillis":compile_ms,"cases":cases,"scope":"Actual signed WasmTier component with synthetic scope; no Core authorization, persistence, browser or iPhone claim."}),
    )
}
#[tokio::main]
async fn main() -> R<()> {
    let args = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    require(args.len() == 4 && args.iter().all(|path| path.is_absolute()),
        "Usage: pdfcraft_component_check ABS_COMPONENT ABS_SOURCE ABS_SOURCE_GUARD_RECEIPT ABS_NEW_OUTPUT")?;
    let component = read(&args[0], 64 << 20)?;
    let document = read(&args[1], DOCUMENT)?;
    let receipt: Value = serde_json::from_slice(&read(&args[2], 16 << 20)?)?;
    let path = args[0].canonicalize()?;
    require(
        receipt["status"] == "PASS"
            && receipt["artifacts"].as_array().is_some_and(|artifacts| {
                artifacts.iter().any(|artifact| {
                    artifact["path"].as_str() == path.to_str()
                        && artifact["sha256"] == sha(&component)
                })
            }),
        "component is not bound to the successful source receipt",
    )?;
    std::fs::create_dir(&args[3])?;
    let result = qualify(&component, &document, &args[3]).await?;
    std::fs::write(
        args[3].join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{result}");
    Ok(())
}

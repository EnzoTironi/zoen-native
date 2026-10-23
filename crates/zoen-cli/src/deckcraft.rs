//! One existing agent-host tool binding. The model selects an offered artifact
//! and command; only the signed core supplies the actor/source and accepts bytes.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use client_core::{AppBinaryDocumentDto, Client, AGENT_BINARY_DOCUMENT_LIMIT};
use ed25519_dalek::VerifyingKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{io::Read, sync::Arc};
use zoen_agentd::{
    deckcraft::{DeckcraftWorker, ENGINE_REVISION},
    SignedManifest, ToolDef,
};

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn bounded_file(path: &str, max: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "Worker configuration file unavailable")?
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Worker configuration file unavailable")?;
    if bytes.len() > max {
        return Err("Worker configuration file exceeds its limit".into());
    }
    Ok(bytes)
}
/// Fixed host deployment configuration. No automatic server import or model path.
pub fn configured() -> Result<Option<Arc<DeckcraftWorker>>, String> {
    let component = std::env::var("ZOEN_DECKCRAFT_WORKER_COMPONENT").ok();
    let manifest = std::env::var("ZOEN_DECKCRAFT_WORKER_MANIFEST").ok();
    let publisher = std::env::var("ZOEN_DECKCRAFT_WORKER_PUBLISHER").ok();
    let (Some(component), Some(manifest), Some(publisher)) =
        (component.clone(), manifest.clone(), publisher.clone())
    else {
        return if component.is_none() && manifest.is_none() && publisher.is_none() {
            Ok(None)
        } else {
            Err("DeckCraft worker requires component, signed manifest and trusted publisher".into())
        };
    };
    let bytes: [u8; 32] = hex::decode(publisher)
        .map_err(|_| "Invalid worker publisher")?
        .try_into()
        .map_err(|_| "Invalid worker publisher")?;
    let trusted = VerifyingKey::from_bytes(&bytes).map_err(|_| "Invalid worker publisher")?;
    let manifest: SignedManifest = serde_json::from_slice(&bounded_file(&manifest, 64 * 1024)?)
        .map_err(|_| "Invalid signed worker manifest")?;
    Ok(Some(Arc::new(DeckcraftWorker::load(
        vec![trusted],
        &manifest,
        &bounded_file(&component, 64 * 1024 * 1024)?,
    )?)))
}
pub fn tools() -> Vec<ToolDef> {
    client_core::deckcraft_agent_tools::tools()
        .into_iter()
        .map(|definition| ToolDef {
            name: definition.name,
            description: definition.description,
            parameters: definition.parameters,
        })
        .collect()
}
use client_core::deckcraft_agent_tools::command;
pub use client_core::deckcraft_agent_tools::{is_model_name, model_name, recognizes};
/// Read-only classification of exact canonical references, not permission to run
/// a command. `call` revalidates authority/CAS after Runtime's unchanged Gate.
pub(crate) fn gate_public_references(
    client: &Client,
    item: &str,
    source: &str,
    serving: &str,
    tool: &str,
    args: &Value,
) -> Option<zoen_agentd::PublicArtifactReferences> {
    if args.get("artifactId").and_then(Value::as_str) != Some(item) {
        return None;
    }
    let (version, sha, _) = command(tool, args).ok()?;
    let turn = client
        .agent_binary_turn(item, source, serving, version, sha.as_deref())
        .ok()?;
    let document = turn.document();
    if document.item_id != item
        || document.actor_kind != "agent"
        || document.engine_revision != ENGINE_REVISION
    {
        return None;
    }
    client.agent_binary_finish_read(&turn).ok()?;
    zoen_agentd::PublicArtifactReferences::new(tool.into(), item.into(), sha)
}
fn receipt(document: &AppBinaryDocumentDto, replay: bool) -> String {
    json!({"itemId":document.item_id,"version":document.version,"currentVersion":document.current_version,"sha256":document.sha256,"authorId":document.author_id,"engineRevision":document.engine_revision,"replayed":replay}).to_string()
}
pub async fn call(
    client: &Client,
    worker: &DeckcraftWorker,
    item: &str,
    source: &str,
    serving: &str,
    tool: &str,
    args: &Value,
) -> Result<String, String> {
    if args.get("artifactId").and_then(Value::as_str) != Some(item) {
        return Err("The offered tool belongs to another presentation".into());
    }
    let (version, sha, action) = command(tool, args)?;
    let turn = client
        .agent_binary_turn(item, source, serving, version, sha.as_deref())
        .map_err(|e| e.to_string())?;
    execute_turn(client, worker, &turn, item, source, serving, tool, action).await
}
/// Execute only the frozen, natively approved call using its pinned source/head.
pub async fn call_approved(
    client: &Client,
    worker: &DeckcraftWorker,
    approved: &client_core::ApprovedToolTurn,
) -> Result<String, String> {
    let p = approved.proposal();
    if worker.binding_sha256() != p.worker_binding {
        return Err("The reviewed executor changed".into());
    }
    let args: Value =
        serde_json::from_str(&p.arguments_json).map_err(|_| "Invalid reviewed arguments")?;
    let (_, _, action) = command(&p.canonical_tool, &args)?;
    execute_turn(
        client,
        worker,
        approved.turn(),
        &p.artifact,
        &p.source,
        &p.serving,
        &p.canonical_tool,
        action,
    )
    .await
}
#[expect(
    clippy::too_many_arguments,
    reason = "the native turn carries the unchanged existing command intent fields"
)]
async fn execute_turn(
    client: &Client,
    worker: &DeckcraftWorker,
    turn: &client_core::AgentBinaryTurn,
    item: &str,
    source: &str,
    serving: &str,
    tool: &str,
    action: Option<Value>,
) -> Result<String, String> {
    let doc = turn.document();
    let scope = json!({"artifactId":doc.item_id,"actorId":doc.actor_id,"actorKind":"agent"});
    let head = json!({"version":doc.version.to_string(),"sha256":doc.sha256});
    let intent = hash(
        &serde_json::to_vec(&(
            item,
            source,
            serving,
            turn.source_sha256(),
            doc.version,
            &doc.sha256,
            ENGINE_REVISION,
            tool,
            &action,
        ))
        .map_err(|_| "Invalid command intent")?,
    );
    let mutation = format!("deckcmd_{intent}");
    if action.is_some() {
        if let Some(accepted) = client
            .agent_binary_result(turn, &mutation, &intent)
            .map_err(|e| e.to_string())?
        {
            return Ok(receipt(&accepted, true));
        }
    }
    let bytes = STANDARD
        .decode(&doc.data_base64)
        .map_err(|_| "Invalid canonical bytes")?;
    if bytes.len() > AGENT_BINARY_DOCUMENT_LIMIT
        || hash(&bytes) != doc.sha256
        || doc.actor_kind != "agent"
        || doc.engine_revision != ENGINE_REVISION
    {
        return Err("The canonical presentation binding changed".into());
    }
    let command=action.as_ref().map(|action|json!({"mutation":{"scope":scope,"base":head,"mutationId":mutation},"action":action}));
    let reply = worker
        .run(
            &json!({"schema":1,"open":{"scope":scope,"head":head},"command":command}),
            &bytes,
        )
        .await?;
    if reply.metadata["scope"] != scope {
        return Err("The worker returned another artifact or actor".into());
    }
    if let Some(action) = action {
        if reply.metadata["mutationId"] != mutation
            || reply.metadata["base"] != head
            || reply.metadata["sha256"] != hash(&reply.bytes)
            || reply.metadata["byteLength"] != reply.bytes.len()
            || reply.metadata["operation"]["kind"] != "command"
            || reply.metadata["operation"]["action"] != action
        {
            return Err("The worker proposal does not match the exact command".into());
        }
        let accepted = client
            .agent_binary_save(turn, &mutation, &intent, &STANDARD.encode(reply.bytes))
            .map_err(|e| e.to_string())?;
        Ok(receipt(&accepted, false))
    } else {
        if reply.metadata["head"] != head || !reply.bytes.is_empty() {
            return Err("The worker inspected another source".into());
        }
        client
            .agent_binary_finish_read(turn)
            .map_err(|e| e.to_string())?;
        Ok(reply.metadata.to_string())
    }
}

#[cfg(test)]
#[path = "deckcraft_tests.rs"]
pub(crate) mod tests;

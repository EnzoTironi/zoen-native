//! One existing agent-host tool binding. The model selects an offered artifact
//! and command; only the signed core supplies the actor/source and accepts bytes.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use client_core::{AppBinaryDocumentDto, Client, AGENT_BINARY_DOCUMENT_LIMIT};
use ed25519_dalek::VerifyingKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{io::Read, sync::Arc};
use zoen_agentd::{deckcraft::DeckcraftWorker, SignedManifest, ToolDef};

#[cfg(test)]
use zoen_agentd::deckcraft::ENGINE_REVISION;

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
    let deck = configured_for(
        "deckcraft",
        [
            "ZOEN_DECKCRAFT_WORKER_COMPONENT",
            "ZOEN_DECKCRAFT_WORKER_MANIFEST",
            "ZOEN_DECKCRAFT_WORKER_PUBLISHER",
        ],
    )?;
    let grid = configured_for(
        "gridcraft",
        [
            "ZOEN_GRIDCRAFT_WORKER_COMPONENT",
            "ZOEN_GRIDCRAFT_WORKER_MANIFEST",
            "ZOEN_GRIDCRAFT_WORKER_PUBLISHER",
        ],
    )?;
    let word = configured_for(
        "wordcraft",
        [
            "ZOEN_WORDCRAFT_WORKER_COMPONENT",
            "ZOEN_WORDCRAFT_WORKER_MANIFEST",
            "ZOEN_WORDCRAFT_WORKER_PUBLISHER",
        ],
    )?;
    let mut configured: Option<DeckcraftWorker> = None;
    for worker in [deck, grid, word].into_iter().flatten() {
        configured = Some(match configured {
            None => worker,
            Some(current) => current.combine(worker)?,
        });
    }
    Ok(configured.map(Arc::new))
}
fn configured_for(app: &str, names: [&str; 3]) -> Result<Option<DeckcraftWorker>, String> {
    let component = std::env::var(names[0]).ok();
    let manifest = std::env::var(names[1]).ok();
    let publisher = std::env::var(names[2]).ok();
    let (Some(component), Some(manifest), Some(publisher)) =
        (component.clone(), manifest.clone(), publisher.clone())
    else {
        return if component.is_none() && manifest.is_none() && publisher.is_none() {
            Ok(None)
        } else {
            Err(format!(
                "{app} worker requires component, signed manifest and trusted publisher"
            ))
        };
    };
    let bytes: [u8; 32] = hex::decode(publisher)
        .map_err(|_| "Invalid worker publisher")?
        .try_into()
        .map_err(|_| "Invalid worker publisher")?;
    let trusted = VerifyingKey::from_bytes(&bytes).map_err(|_| "Invalid worker publisher")?;
    let manifest: SignedManifest = serde_json::from_slice(&bounded_file(&manifest, 64 * 1024)?)
        .map_err(|_| "Invalid signed worker manifest")?;
    Ok(Some(DeckcraftWorker::load_for(
        app,
        vec![trusted],
        &manifest,
        &bounded_file(&component, 64 * 1024 * 1024)?,
    )?))
}
pub fn tools() -> Vec<ToolDef> {
    tools_for_format("deckcraft")
}
pub fn tools_for(app: &str) -> Vec<ToolDef> {
    if app == "deckcraft" {
        tools()
    } else {
        tools_for_format(app)
    }
}
fn tools_for_format(app: &str) -> Vec<ToolDef> {
    client_core::binary_agent_tools::tools()
        .into_iter()
        .filter(|tool| {
            client_core::binary_agent_tools::format(&tool.name)
                .is_some_and(|format| format.id() == app)
        })
        .map(|definition| ToolDef {
            name: definition.name,
            description: definition.description,
            parameters: definition.parameters,
        })
        .collect()
}
use client_core::binary_agent_tools::command;
pub use client_core::binary_agent_tools::{is_model_name, model_name, recognizes};
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
    let format = client_core::binary_agent_tools::format(tool)?;
    if document.item_id != item
        || document.actor_kind != "agent"
        || document.kind != format.id()
        || document.engine_revision != format.revision()
    {
        return None;
    }
    client.agent_binary_finish_read(&turn).ok()?;
    zoen_agentd::PublicArtifactReferences::new(tool.into(), item.into(), sha)
}
fn grid_read(tool: &str, args: &Value) -> Option<Value> {
    (tool == "read_gridcraft" && args.get("sheet").is_some())
        .then(|| json!({"sheet":args["sheet"],"cell":args["cell"]}))
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
    execute_turn(
        client,
        worker,
        &turn,
        item,
        source,
        serving,
        tool,
        action,
        grid_read(tool, args),
    )
    .await
}
/// Execute only the frozen, natively approved call using its pinned source/head.
pub async fn call_approved(
    client: &Client,
    worker: &DeckcraftWorker,
    approved: &client_core::ApprovedToolTurn,
) -> Result<String, String> {
    let p = approved.proposal();
    if worker.binding_for(&approved.turn().document().kind)? != p.worker_binding {
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
        grid_read(&p.canonical_tool, &args),
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
    read: Option<Value>,
) -> Result<String, String> {
    let doc = turn.document();
    let format =
        client_core::binary_agent_tools::format(tool).ok_or("Unsupported binary App tool")?;
    if doc.kind != format.id() || !worker.supports(format.id()) {
        return Err("The canonical App executor changed".into());
    }
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
            doc.engine_revision.as_str(),
            tool,
            &action,
        ))
        .map_err(|_| "Invalid command intent")?,
    );
    let prefix = match format.id() {
        "deckcraft" => "deckcmd",
        "gridcraft" => "gridcmd",
        "wordcraft" => "wordcmd",
        _ => return Err("Unsupported binary App executor".into()),
    };
    let mutation = format!("{prefix}_{intent}");
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
        || doc.engine_revision != format.revision()
    {
        return Err("The canonical presentation binding changed".into());
    }
    let command=action.as_ref().map(|action|json!({"mutation":{"scope":scope,"base":head,"mutationId":mutation},"action":action}));
    let mut envelope = json!({"schema":1,"open":{"scope":scope,"head":head},"command":command});
    if let Some(read) = &read {
        envelope["read"] = read.clone();
    }
    let reply = worker.run_for(format.id(), &envelope, &bytes).await?;
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
        if reply.metadata["head"] != head
            || !reply.bytes.is_empty()
            || read
                .as_ref()
                .is_some_and(|query| reply.metadata["query"] != *query)
        {
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

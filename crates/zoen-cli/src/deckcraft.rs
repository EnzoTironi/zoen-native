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
    let base = json!({"artifactId":{"type":"string","minLength":1,"maxLength":128},"version":{"type":"integer","minimum":1,"maximum":4294967294_u32},"sha256":{"type":"string","pattern":"^[a-f0-9]{64}$"}});
    let mut out = vec![ToolDef{name:"read_deckcraft".into(),description:"Read this presentation's accepted version/hash, slide and shape IDs before editing. Isolated executor supports at most 512 KiB.".into(),parameters:json!({"type":"object","properties":{"artifactId":{"type":"string","minLength":1,"maxLength":128}},"required":["artifactId"],"additionalProperties":false})}];
    for (name, description, properties, required) in [
        (
            "deckcraft_text_set",
            "Replace one shape's text",
            json!({"slideId":{"type":"integer","minimum":0},"id":{"type":"integer","minimum":0},"text":{"type":"string","maxLength":8192}}),
            vec!["slideId", "id", "text"],
        ),
        (
            "deckcraft_slide_notes",
            "Replace one slide's notes",
            json!({"slideId":{"type":"integer","minimum":0},"text":{"type":"string","maxLength":8192}}),
            vec!["slideId", "text"],
        ),
        (
            "deckcraft_shape_insert",
            "Insert one reviewed shape",
            json!({"slideId":{"type":"integer","minimum":0},"preset":{"enum":["rect","ellipse","roundRect"]},"rect":{"type":"array","minItems":4,"maxItems":4,"items":{"type":"number"}},"text":{"type":"string","maxLength":8192}}),
            vec!["slideId", "preset", "rect", "text"],
        ),
        (
            "deckcraft_slide_new",
            "Insert one slide after an existing slide",
            json!({"afterSlideId":{"type":"integer","minimum":0},"layout":{"enum":["blank","title","titleAndContent"]},"title":{"type":"string","maxLength":8192},"body":{"type":"string","maxLength":8192}}),
            vec!["afterSlideId", "layout", "title", "body"],
        ),
    ] {
        let mut properties = properties.as_object().cloned().unwrap_or_default();
        properties.extend(base.as_object().cloned().unwrap_or_default());
        let required: Vec<&str> = [vec!["artifactId", "version", "sha256"], required].concat();
        out.push(ToolDef{name:name.into(),description:format!("{description}; exact observed version/hash required. 512 KiB executor capacity; four explicit reversible commands only."),parameters:json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})});
    }
    out
}
pub fn recognizes(tool: &str) -> bool {
    tools().iter().any(|t| t.name == tool)
}
/// Compact stable alias; the server mapping and required artifactId still
/// retain/check the entire canonical ID, including its 72-character form.
pub fn model_name(tool: &str, item: &str) -> Option<String> {
    let digest = hash(item.as_bytes());
    let name = format!("{tool}__{}", &digest[..40]);
    (recognizes(tool)
        && !item.is_empty()
        && item.len() <= 128
        && item
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        && name.len() <= 64)
        .then_some(name)
}
pub fn is_model_name(name: &str) -> bool {
    name.rsplit_once("__").is_some_and(|(tool, digest)| {
        recognizes(tool)
            && digest.len() == 40
            && digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
type ParsedCommand = (Option<u32>, Option<String>, Option<Value>);

fn command(tool: &str, args: &Value) -> Result<ParsedCommand, String> {
    let definition = tools()
        .into_iter()
        .find(|t| t.name == tool)
        .ok_or("Unknown presentation tool")?;
    let args = args
        .as_object()
        .ok_or("Presentation arguments must be an object")?;
    let allowed = definition.parameters["properties"]
        .as_object()
        .ok_or("Invalid presentation schema")?;
    if args.keys().any(|key| !allowed.contains_key(key)) {
        return Err("Unexpected presentation argument".into());
    }
    if args
        .get("artifactId")
        .and_then(Value::as_str)
        .is_none_or(|id| {
            id.is_empty()
                || id.len() > 128
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        })
    {
        return Err("An exact canonical artifactId is required".into());
    }
    if tool == "read_deckcraft" {
        return Ok((None, None, None));
    }
    let version = args
        .get("version")
        .and_then(Value::as_u64)
        .filter(|n| *n >= 1 && *n < u32::MAX as u64)
        .ok_or("An observed version is required")? as u32;
    let sha = args
        .get("sha256")
        .and_then(Value::as_str)
        .filter(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or("An observed hash is required")?
        .to_owned();
    for field in ["slideId", "id", "afterSlideId"] {
        if allowed.contains_key(field)
            && args
                .get(field)
                .and_then(Value::as_u64)
                .is_none_or(|id| id > u32::MAX as u64)
        {
            return Err("A native slide/shape ID is required".into());
        }
    }
    for field in ["text", "title", "body"] {
        if allowed.contains_key(field)
            && args
                .get(field)
                .and_then(Value::as_str)
                .is_none_or(|text| text.len() > 8192)
        {
            return Err("Presentation text must be a string up to 8192 UTF-8 bytes".into());
        }
    }
    for (field, values) in [
        ("preset", &["rect", "ellipse", "roundRect"][..]),
        ("layout", &["blank", "title", "titleAndContent"][..]),
    ] {
        if allowed.contains_key(field)
            && args
                .get(field)
                .and_then(Value::as_str)
                .is_none_or(|value| !values.contains(&value))
        {
            return Err("Unsupported presentation command variant".into());
        }
    }
    let mut params = args.clone();
    params.remove("artifactId");
    params.remove("version");
    params.remove("sha256");
    if tool == "deckcraft_shape_insert" {
        let rect = params
            .get_mut("rect")
            .and_then(Value::as_array_mut)
            .filter(|rect| rect.len() == 4)
            .ok_or("A four-number rectangle is required")?;
        // Match the pinned bridge's f64 DTO. JSON integer coordinates and float
        // coordinates are the same geometric command and must hash/verify alike.
        for coordinate in rect {
            let number = coordinate
                .as_f64()
                .filter(|n| n.is_finite())
                .ok_or("Invalid rectangle coordinate")?;
            *coordinate = json!(number);
        }
    }
    let command = match tool {
        "deckcraft_text_set" => "text.set",
        "deckcraft_slide_notes" => "slide.notes",
        "deckcraft_shape_insert" => "shape.insert",
        "deckcraft_slide_new" => "slide.new",
        _ => return Err("Unknown presentation tool".into()),
    };
    Ok((
        Some(version),
        Some(sha),
        Some(json!({"command":command,"params":params})),
    ))
}
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
            .agent_binary_result(&turn, &mutation, &intent)
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
            .agent_binary_save(&turn, &mutation, &intent, &STANDARD.encode(reply.bytes))
            .map_err(|e| e.to_string())?;
        Ok(receipt(&accepted, false))
    } else {
        if reply.metadata["head"] != head || !reply.bytes.is_empty() {
            return Err("The worker inspected another source".into());
        }
        client
            .agent_binary_finish_read(&turn)
            .map_err(|e| e.to_string())?;
        Ok(reply.metadata.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use client_core::SecretVault;
    use ed25519_dalek::SigningKey;
    use std::{collections::HashMap, sync::Mutex};
    use zoen_agentd::{Needs, ToolManifest};
    const ORIGINAL: &[u8] =
        include_bytes!("../../client-core/src/app_binary_documents/fixtures/original.deckcraft");
    #[derive(Default)]
    struct Vault(Mutex<HashMap<String, Vec<u8>>>);
    impl SecretVault for Vault {
        fn load(&self, key: String) -> Option<Vec<u8>> {
            self.0.lock().unwrap().get(&key).cloned()
        }
        fn save(&self, key: String, value: Vec<u8>) -> bool {
            self.0.lock().unwrap().insert(key, value);
            true
        }
        fn delete(&self, key: String) {
            self.0.lock().unwrap().remove(&key);
        }
    }
    struct Fixture {
        path: std::path::PathBuf,
        client: Arc<Client>,
        vault: Arc<Vault>,
        owner: String,
        source: String,
        base: AppBinaryDocumentDto,
    }
    fn fixture() -> Fixture {
        let path = std::env::temp_dir().join(types::new_id("deckcraft-worker-test"));
        std::fs::create_dir_all(&path).unwrap();
        let owner_path = path.join("owner.sqlite");
        let owner_vault = Arc::new(Vault::default());
        let human = Client::open(owner_path.to_str().unwrap().into(), "en".into()).unwrap();
        let owner = human
            .create_account(
                "Owner".into(),
                "worker_owner".into(),
                "http://127.0.0.1:1".into(),
                owner_vault.clone(),
            )
            .unwrap()
            .identity_id;
        let root = [72; 32];
        let id = event_log::Signer::from_secret(&root).id();
        let proof = human.authorize_agent_json(id).unwrap();
        drop(human);
        let owner_store = store::Store::open(owner_path.to_str().unwrap()).unwrap();
        let profile = owner_store
            .identities()
            .unwrap()
            .into_iter()
            .find(|(profile, _)| profile.id == owner)
            .unwrap()
            .0;
        drop(owner_store);
        let db = path.join("agent.sqlite");
        let vault = Arc::new(Vault::default());
        let agent = Client::open(db.to_str().unwrap().into(), "en".into()).unwrap();
        agent
            .create_agent_account(
                "Worker".into(),
                "worker_agent".into(),
                "http://127.0.0.1:1".into(),
                root.to_vec(),
                owner.clone(),
                proof,
                vault.clone(),
            )
            .unwrap();
        drop(agent);
        // Offline fixture only. All source events still pass the real signature
        // and projection code; no relay/service, mocked core or browser actor.
        let key: [u8; 32] = owner_vault
            .load("zoen.identity.v1".into())
            .unwrap()
            .try_into()
            .unwrap();
        let db_store = store::Store::open(db.to_str().unwrap()).unwrap();
        db_store.put_identity(&profile, Some(&key)).unwrap();
        let space = types::new_id("worker_space");
        let mut log = event_log::SpaceLog::new(&space);
        let agent_signer = event_log::Signer::from_secret(&root);
        let human_signer = event_log::Signer::from_secret(&key);
        for (time, body) in [
            (
                1_792_510_809_000,
                types::EventBody::SpaceCreated {
                    title: "Worker test".into(),
                    kind: types::SpaceKind::Group,
                    privacy: types::Privacy::Closed,
                },
            ),
            (
                1_792_510_809_001,
                types::EventBody::MemberAdded {
                    identity: owner.clone(),
                    role: types::Role::Member,
                },
            ),
        ] {
            db_store
                .append_event(log.append(&agent_signer, time, body))
                .unwrap();
        }
        let source_event = log
            .append(
                &human_signer,
                1_792_510_809_002,
                types::EventBody::MessagePosted {
                    message: types::new_id("source"),
                    text: "Update this presentation with the reviewed edits".into(),
                    attaches: None,
                    reply: None,
                    media: None,
                    forwarded: false,
                },
            )
            .clone();
        db_store.append_event(&source_event).unwrap();
        let source = source_event.client_id;
        drop(db_store);
        let client = Client::open(db.to_str().unwrap().into(), "en".into()).unwrap();
        assert!(client.unlock(vault.clone()).unwrap());
        let base = client
            .app_binary_create(
                space.clone(),
                "Agent deck".into(),
                STANDARD.encode(ORIGINAL),
                "worker_create".into(),
            )
            .unwrap();
        Fixture {
            path,
            client,
            vault,
            owner,
            source,
            base,
        }
    }
    fn worker() -> DeckcraftWorker {
        let path = std::env::var("ZOEN_TEST_DECKCRAFT_COMPONENT")
            .expect("explicit real WASI-p2 component path required; see v160 build receipt");
        let bytes = bounded_file(&path, 64 * 1024 * 1024).unwrap();
        let key = SigningKey::from_bytes(&[19; 32]);
        let signed = ToolManifest {
            id: zoen_agentd::deckcraft::WORKER_ID.into(),
            version: ENGINE_REVISION.into(),
            summary: "Pinned isolated presentation test worker".into(),
            needs: Needs::Wasm,
            egress: vec![],
            secrets: vec![],
            component_sha256: Some(hash(&bytes)),
        }
        .sign(&key);
        let mut tampered = bytes.clone();
        tampered.push(0);
        assert!(DeckcraftWorker::load(vec![key.verifying_key()], &signed, &tampered).is_err());
        DeckcraftWorker::load(vec![key.verifying_key()], &signed, &bytes).unwrap()
    }
    async fn inspect(f: &Fixture, worker: &DeckcraftWorker) -> Value {
        serde_json::from_str(
            &call(
                &f.client,
                worker,
                &f.base.item_id,
                &f.source,
                &f.owner,
                "read_deckcraft",
                &json!({"artifactId":f.base.item_id}),
            )
            .await
            .unwrap(),
        )
        .unwrap()
    }
    fn at(document: &Value, params: Value) -> Value {
        let mut params = params.as_object().unwrap().clone();
        params.insert("artifactId".into(), document["scope"]["artifactId"].clone());
        params.insert(
            "version".into(),
            document["head"]["version"]
                .as_str()
                .unwrap()
                .parse::<u32>()
                .unwrap()
                .into(),
        );
        params.insert("sha256".into(), document["head"]["sha256"].clone());
        Value::Object(params)
    }
    #[test]
    fn model_schema_does_not_accept_actor_authority_paths_or_unobserved_versions() {
        let item = format!("it_deck_{}", "ab".repeat(32));
        assert_eq!(item.len(), 72);
        assert!(model_name("deckcraft_shape_insert", &item).unwrap().len() <= 64);
        assert_eq!(
            model_name("read_deckcraft", &item),
            model_name("read_deckcraft", &item)
        );
        assert_ne!(
            model_name("read_deckcraft", &item),
            model_name("read_deckcraft", &format!("it_deck_{}", "cd".repeat(32)))
        );
        assert!(command("read_deckcraft", &json!({"artifactId":item})).is_ok());
        assert!(command(
            "read_deckcraft",
            &json!({"artifactId":item,"actorId":"forged"})
        )
        .is_err());
        assert!(command(
            "deckcraft_slide_notes",
            &json!({"artifactId":item,"slideId":257,"text":"x"})
        )
        .is_err());
        assert!(command("deckcraft_slide_notes", &json!({"artifactId":item,"version":1,"sha256":"00".repeat(32),"slideId":257,"text":"x","path":"/tmp/other"})).is_err());
        assert!(command("deckcraft_slide_notes", &json!({"artifactId":item,"version":1,"sha256":"00".repeat(32),"slideId":257,"text":"x".repeat(8193)})).is_err());
        let normalized=command("deckcraft_shape_insert", &json!({"artifactId":item,"version":1,"sha256":"00".repeat(32),"slideId":257,"preset":"rect","rect":[20,24,120,48],"text":"x"})).unwrap().2.unwrap();
        assert_eq!(
            normalized["params"]["rect"],
            json!([20.0, 24.0, 120.0, 48.0])
        );
        assert!(normalized["params"].get("artifactId").is_none());
        assert_eq!(tools().len(), 5);
        assert_eq!(
            tools()
                .iter()
                .filter(|tool| tool.parameters["properties"].get("version").is_some())
                .count(),
            4
        );
    }
    #[test]
    fn native_public_reference_classification_requires_real_binding_head_and_closed_dto() {
        let f = fixture();
        let args = json!({"artifactId":f.base.item_id,"version":f.base.version,"sha256":f.base.sha256,"slideId":257,"text":"Reviewed notes"});
        let classify = |item: &str, source: &str, args: &Value| {
            gate_public_references(
                &f.client,
                item,
                source,
                &f.owner,
                "deckcraft_slide_notes",
                args,
            )
        };
        assert!(classify(&f.base.item_id, &f.source, &args).is_some());
        for changed in [
            {
                let mut a = args.clone();
                a["artifactId"] = "different_item".into();
                a
            },
            {
                let mut a = args.clone();
                a["sha256"] = "00".repeat(32).into();
                a
            },
            {
                let mut a = args.clone();
                a["version"] = 999.into();
                a
            },
            {
                let mut a = args.clone();
                a["public"] = true.into();
                a
            },
            {
                let mut a = args.clone();
                a["path"] = "/tmp/external".into();
                a
            },
            {
                let mut a = args.clone();
                a.as_object_mut().unwrap().remove("text");
                a
            },
            {
                let mut a = args.clone();
                a["text"] = json!({"public":true});
                a
            },
            {
                let mut a = args.clone();
                a["slideId"] = json!(u64::MAX);
                a
            },
        ] {
            assert!(classify(&f.base.item_id, &f.source, &changed).is_none());
        }
        assert!(classify(&f.base.item_id, "missing-source", &args).is_none());
        let mut credential = args.clone();
        credential["text"] = format!("sk-test-{}", "a1".repeat(24)).into();
        // Classification covers references only. Runtime's Gate must still reject this text.
        assert!(classify(&f.base.item_id, &f.source, &credential).is_some());
        assert_eq!(
            f.client
                .app_binary_read(f.base.item_id.clone(), None, None)
                .unwrap(),
            f.base
        );
        assert_eq!(
            f.client
                .item(f.base.item_id.clone())
                .unwrap()
                .versions
                .len(),
            1
        );
        drop(f.client);
        std::fs::remove_dir_all(f.path).unwrap();
    }
    // Explicitly ignored in generic `cargo test`: this test requires an actual
    // separately pinned upstream build, not a substitute WAT component or mock.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires ZOEN_TEST_DECKCRAFT_COMPONENT built from pinned upstream + headless supplement"]
    async fn actual_pinned_component_edits_as_certified_agent_and_cold_replay_does_not_duplicate() {
        let translation_started = std::time::Instant::now();
        let worker = Arc::new(worker());
        eprintln!(
            "actual pinned Component::new/load elapsed_ms={}",
            translation_started.elapsed().as_millis()
        );
        let execution_started = std::time::Instant::now();
        let f = fixture();
        let initial = inspect(&f, &worker).await;
        assert_eq!(initial["scope"]["actorId"], f.base.actor_id);
        assert_eq!(initial["scope"]["actorKind"], "agent");
        assert_eq!(initial["slides"].as_array().unwrap().len(), 1);
        let slide = initial["slides"][0]["id"].as_u64().unwrap();
        let shape = initial["slides"][0]["shapes"][0]["id"].as_u64().unwrap();
        let first_args = at(
            &initial,
            json!({"slideId":slide,"id":shape,"text":"Certified agent — exact canonical text"}),
        );
        let first: Value = serde_json::from_str(
            &call(
                &f.client,
                &worker,
                &f.base.item_id,
                &f.source,
                &f.owner,
                "deckcraft_text_set",
                &first_args,
            )
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(first["version"], 2);
        assert_eq!(first["authorId"], f.base.actor_id);
        assert_eq!(first["replayed"], false);
        let after_text = inspect(&f, &worker).await;
        assert_eq!(
            after_text["slides"][0]["shapes"][0]["text"]["paragraphs"][0]["runs"][0]["text"],
            "Certified agent — exact canonical text"
        );
        let notes = at(
            &after_text,
            json!({"slideId":slide,"text":"Reviewed notes from the isolated engine"}),
        );
        let notes_receipt: Value = serde_json::from_str(
            &call(
                &f.client,
                &worker,
                &f.base.item_id,
                &f.source,
                &f.owner,
                "deckcraft_slide_notes",
                &notes,
            )
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(notes_receipt["version"], 3);
        let after_notes = inspect(&f, &worker).await;
        assert_eq!(
            after_notes["slides"][0]["notes"]["paragraphs"][0]["runs"][0]["text"],
            "Reviewed notes from the isolated engine"
        );
        let before_shapes = after_notes["slides"][0]["shapes"].as_array().unwrap().len();
        let insert = at(
            &after_notes,
            json!({"slideId":slide,"preset":"roundRect","rect":[20,24,120,48],"text":"A reviewed shape"}),
        );
        let inserted: Value = serde_json::from_str(
            &call(
                &f.client,
                &worker,
                &f.base.item_id,
                &f.source,
                &f.owner,
                "deckcraft_shape_insert",
                &insert,
            )
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(inserted["version"], 4);
        let after_shape = inspect(&f, &worker).await;
        assert_eq!(
            after_shape["slides"][0]["shapes"].as_array().unwrap().len(),
            before_shapes + 1
        );
        let add_slide = at(
            &after_shape,
            json!({"afterSlideId":slide,"layout":"title","title":"Second accepted slide","body":"Exact source, exact actor"}),
        );
        let added: Value = serde_json::from_str(
            &call(
                &f.client,
                &worker,
                &f.base.item_id,
                &f.source,
                &f.owner,
                "deckcraft_slide_new",
                &add_slide,
            )
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(added["version"], 5);
        let final_view = inspect(&f, &worker).await;
        assert_eq!(final_view["slides"].as_array().unwrap().len(), 2);
        assert_eq!(final_view["slides"][0]["id"], slide);
        assert_ne!(final_view["slides"][1]["id"], slide);
        crate::agent::exercise_deckcraft_dispatch(
            &f.client,
            worker.clone(),
            &f.path.join("agent-tool-steps.json"),
            (&f.base.item_id, &f.source, &f.owner),
            at(
                &final_view,
                json!({"slideId":slide,"text":"Runtime dispatch — gated and signed"}),
            ),
        )
        .await;
        let after_dispatch = inspect(&f, &worker).await;
        assert_eq!(
            after_dispatch["slides"][0]["notes"]["paragraphs"][0]["runs"][0]["text"],
            "Runtime dispatch — gated and signed"
        );
        let text_b = at(
            &after_dispatch,
            json!({"slideId":slide,"id":shape,"text":"Intervening reviewed text B"}),
        );
        let changed_b: Value = serde_json::from_str(
            &call(
                &f.client,
                &worker,
                &f.base.item_id,
                &f.source,
                &f.owner,
                "deckcraft_text_set",
                &text_b,
            )
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(changed_b["version"], 7);
        let after_b = inspect(&f, &worker).await;
        assert_eq!(
            after_b["slides"][0]["shapes"][0]["text"]["paragraphs"][0]["runs"][0]["text"],
            "Intervening reviewed text B"
        );
        // The same action under the same source, deliberately re-applied against
        // a new observed head, is a new intent rather than the old receipt.
        let text_a_again = at(
            &after_b,
            json!({"slideId":slide,"id":shape,"text":"Certified agent — exact canonical text"}),
        );
        let changed_a: Value = serde_json::from_str(
            &call(
                &f.client,
                &worker,
                &f.base.item_id,
                &f.source,
                &f.owner,
                "deckcraft_text_set",
                &text_a_again,
            )
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(changed_a["version"], 8);
        assert_eq!(changed_a["replayed"], false);
        let after_a = inspect(&f, &worker).await;
        assert_eq!(
            after_a["slides"][0]["shapes"][0]["text"]["paragraphs"][0]["runs"][0]["text"],
            "Certified agent — exact canonical text"
        );
        let accepted = f
            .client
            .app_binary_read(f.base.item_id.clone(), None, None)
            .unwrap();
        assert_eq!(accepted.version, 8);
        assert_eq!(accepted.author_id, f.base.actor_id);
        assert_ne!(accepted.sha256, f.base.sha256);
        let stale = call(
            &f.client,
            &worker,
            &f.base.item_id,
            &f.source,
            &f.owner,
            "deckcraft_slide_notes",
            &at(
                &initial,
                json!({"slideId":slide,"text":"Stale concurrent write"}),
            ),
        )
        .await;
        assert!(stale.is_err());
        assert!(call(
            &f.client,
            &worker,
            &f.base.item_id,
            "forged-source",
            &f.owner,
            "read_deckcraft",
            &json!({"artifactId":f.base.item_id})
        )
        .await
        .is_err());
        assert!(worker
            .run(&json!({}), &vec![0; AGENT_BINARY_DOCUMENT_LIMIT + 1])
            .await
            .is_err());
        let Fixture {
            path,
            client,
            vault,
            owner,
            source,
            base,
        } = f;
        drop(client);
        let restored = Client::open(
            path.join("agent.sqlite").to_str().unwrap().into(),
            "en".into(),
        )
        .unwrap();
        assert!(restored.unlock(vault).unwrap());
        // Simulate a lost response/journal: replay exactly the original observed
        // command, after Client/SQLite reopen and several newer accepted versions.
        // The compiled worker Arc is retained in this same test process.
        let replay: Value = serde_json::from_str(
            &call(
                &restored,
                &worker,
                &base.item_id,
                &source,
                &owner,
                "deckcraft_text_set",
                &first_args,
            )
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(replay["replayed"], true);
        assert_eq!(replay["version"], 2);
        assert_eq!(replay["currentVersion"], 8);
        assert_eq!(
            restored
                .app_binary_read(base.item_id.clone(), None, None)
                .unwrap(),
            accepted
        );
        assert_eq!(restored.item(base.item_id).unwrap().versions.len(), 8);
        drop(restored);
        std::fs::remove_dir_all(path).unwrap();
        eprintln!(
            "actual signed Runtime/core/worker journey elapsed_ms={}",
            execution_started.elapsed().as_millis()
        );
    }
}

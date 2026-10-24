use super::*;
use client_core::SecretVault;
use ed25519_dalek::SigningKey;
use std::{collections::HashMap, sync::Mutex};
use zoen_agentd::{Needs, ToolManifest};
const ORIGINAL: &[u8] =
    include_bytes!("../../client-core/src/app_binary_documents/fixtures/original.deckcraft");
#[derive(Default)]
pub(crate) struct Vault(Mutex<HashMap<String, Vec<u8>>>);
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
pub(crate) struct Fixture {
    pub(crate) path: std::path::PathBuf,
    pub(crate) client: Arc<Client>,
    pub(crate) vault: Arc<Vault>,
    pub(crate) owner_vault: Arc<Vault>,
    pub(crate) owner: String,
    pub(crate) source: String,
    pub(crate) base: AppBinaryDocumentDto,
}
pub(crate) fn fixture() -> std::result::Result<Fixture, Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(types::new_id("deckcraft-worker-test")?);
    std::fs::create_dir_all(&path).unwrap();
    Ok(fixture_at(path, [72; 32])?)
}
#[path = "deckcraft/fixture.rs"]
mod fixture_construction;
pub(crate) use fixture_construction::{fixture_at, fixture_with_privacy_at};

pub(crate) fn worker() -> Arc<DeckcraftWorker> {
    if [
        "ZOEN_DECKCRAFT_WORKER_COMPONENT",
        "ZOEN_DECKCRAFT_WORKER_MANIFEST",
        "ZOEN_DECKCRAFT_WORKER_PUBLISHER",
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some())
    {
        // Explicit operator QA exercises the production loader and trusted
        // publisher. Partial/invalid configuration must never use the fixture.
        eprintln!("actual worker factory: production configured operator manifest");
        return configured()
            .expect("valid explicit worker deployment configuration required")
            .expect("an explicitly configured worker is required");
    }
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
    Arc::new(DeckcraftWorker::load(vec![key.verifying_key()], &signed, &bytes).unwrap())
}
mod restart {
    include!("deckcraft/restart_tests.rs");
}
mod codex {
    include!("deckcraft/codex_tests.rs");
}
pub(crate) async fn inspect(f: &Fixture, worker: &DeckcraftWorker) -> Value {
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
fn native_public_reference_classification_requires_real_binding_head_and_closed_dto(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture()?;
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
    Ok(())
}
// Explicitly ignored in generic `cargo test`: this test requires an actual
// separately pinned upstream build, not a substitute WAT component or mock.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a pinned component via ZOEN_TEST_DECKCRAFT_COMPONENT or complete worker deployment configuration"]
async fn actual_pinned_component_edits_as_certified_agent_and_cold_replay_does_not_duplicate(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let translation_started = std::time::Instant::now();
    let worker = worker();
    eprintln!(
        "actual pinned Component::new/load elapsed_ms={}",
        translation_started.elapsed().as_millis()
    );
    let execution_started = std::time::Instant::now();
    let f = fixture()?;
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
        ..
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
    Ok(())
}

//! Real signed component + certified native agent + controlled Gate/brain; no provider login.
use super::*;
use crate::deckcraft::tests::{fixture, Fixture};
use ed25519_dalek::SigningKey;
use sha2::Digest;
use std::sync::atomic::{AtomicUsize, Ordering};
use zoen_agentd::{Needs, ToolManifest};
fn worker() -> Arc<zoen_agentd::deckcraft::DeckcraftWorker> {
    let path = std::env::var("ZOEN_TEST_GRIDCRAFT_COMPONENT")
        .expect("explicit genuine GridCraft WASI-p2 component required");
    let bytes = std::fs::read(path).expect("actual component");
    assert!(!bytes.is_empty() && bytes.len() <= 64 * 1024 * 1024);
    let key = SigningKey::from_bytes(&[29; 32]);
    let signed = ToolManifest {
        id: "gridcraft-canonical-v1".into(),
        version: "b8771d51fb4da60bd48c13cf313d6d6014dc5e66".into(),
        summary: "Pinned actual GridCraft offline fixture".into(),
        needs: Needs::Wasm,
        egress: vec![],
        secrets: vec![],
        component_sha256: Some(hex::encode(sha2::Sha256::digest(&bytes))),
    }
    .sign(&key);
    let mut tampered = bytes.clone();
    tampered.push(0);
    assert!(zoen_agentd::deckcraft::DeckcraftWorker::load_for(
        "gridcraft",
        vec![key.verifying_key()],
        &signed,
        &tampered
    )
    .is_err());
    Arc::new(
        zoen_agentd::deckcraft::DeckcraftWorker::load_for(
            "gridcraft",
            vec![key.verifying_key()],
            &signed,
            &bytes,
        )
        .expect("signed genuine worker"),
    )
}
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "explicit actual GridCraft WASI-p2 component, synthetic Gate/brain, no provider or services"]
async fn actual_grid_owner_review_worker_cas_and_lost_receipt_cold_recovery(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let worker = worker();
    let mut f = fixture()?;
    let space = f
        .client
        .item(f.base.item_id.clone())
        .expect("space")
        .space_id;
    f.base = f
        .client
        .app_binary_create_for(
            "gridcraft".into(),
            space.clone(),
            "Shared spreadsheet".into(),
            base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                include_bytes!("../../../miniapps/resources/gridcraft-empty.xlsx"),
            ),
            "grid-real-worker".into(),
        )
        .expect("real canonical seed");
    let inspected: serde_json::Value = serde_json::from_str(
        &crate::deckcraft::call(
            &f.client,
            &worker,
            &f.base.item_id,
            &f.source,
            &f.owner,
            "read_gridcraft",
            &json!({"artifactId":f.base.item_id,"sheet":"Sheet1","cell":"A1"}),
        )
        .await
        .expect("actual WASI read"),
    )
    .expect("inspection");
    assert_eq!(inspected["scope"]["actorId"], f.base.actor_id);
    assert_eq!(inspected["scope"]["actorKind"], "agent");
    assert_eq!(inspected["cell"]["type"], "empty");
    let call = ToolCall {
        requires_review: false,
        id: "reviewed-grid-cell".into(),
        name: crate::deckcraft::model_name("gridcraft_cell_set", &f.base.item_id).expect("alias"),
        arguments: json!({"artifactId":f.base.item_id,"version":f.base.version,"sha256":f.base.sha256,"sheet":"Sheet1","cell":"A1","input":"7"}),
    };
    let initial = Arc::new(AtomicUsize::new(0));
    let completed = Arc::new(AtomicUsize::new(0));
    let decisions = Arc::new(AtomicUsize::new(0));
    let journal = f.path.join("grid-tool-steps.json");
    let cx = canonical_tool_approval_tests::context(
        &f.client,
        worker.clone(),
        &journal,
        &call,
        &initial,
        &completed,
        &decisions,
        false,
    );
    canonical_tool_approval_tests::handle(&f.client, &cx, &space, &f.source).await;
    assert_eq!(initial.load(Ordering::SeqCst), 1);
    assert_eq!(completed.load(Ordering::SeqCst), 0);
    let requests = f.client.tool_approval_requests(&space, &f.source);
    assert_eq!(requests.len(), 1);
    let request = requests[0].clone();
    assert_eq!(
        request
            .tool_proposal
            .as_ref()
            .expect("proposal")
            .canonical_tool,
        "gridcraft_cell_set"
    );
    assert_eq!(
        f.client
            .app_binary_read(f.base.item_id.clone(), None, None)
            .expect("pending unchanged"),
        f.base
    );
    let Fixture {
        path,
        client,
        vault,
        owner,
        source,
        base,
        ..
    } = f;
    drop(cx);
    drop(client);
    canonical_tool_approval_tests::signed_owner_resolution(&path, &space, &request, &owner)?;
    let client = canonical_tool_approval_tests::reopen(&path, vault.clone());
    let cx = canonical_tool_approval_tests::context(
        &client,
        worker.clone(),
        &journal,
        &call,
        &initial,
        &completed,
        &decisions,
        true,
    );
    let tools = SpaceTools::new(&client, &cx, &space, &owner, &source);
    assert!(tools
        .begin_resume(&request.id, &request.content_hash)
        .expect("durable attempt"));
    drop(tools);
    drop(cx);
    let permit = client
        .approved_tool_turn(
            &request.id,
            worker.binding_for("gridcraft").expect("grid binding"),
        )
        .expect("current native owner approval");
    let accepted: serde_json::Value = serde_json::from_str(
        &crate::deckcraft::call_approved(&client, &worker, &permit)
            .await
            .expect("actual WASI edit + Core CAS"),
    )
    .expect("receipt");
    assert_eq!(accepted["version"], 2);
    assert_eq!(accepted["authorId"], base.actor_id);
    assert_eq!(accepted["replayed"], false);
    let doc = client
        .app_binary_read(base.item_id.clone(), None, None)
        .expect("accepted bytes");
    assert_eq!(doc.kind, "gridcraft");
    assert_eq!(doc.actor_kind, "agent");
    assert!(JournalState::load(&journal)
        .expect("journal")
        .steps
        .is_empty());
    drop(permit);
    drop(client);
    let client = canonical_tool_approval_tests::reopen(&path, vault);
    let cx = canonical_tool_approval_tests::context(
        &client,
        worker.clone(),
        &journal,
        &call,
        &initial,
        &completed,
        &decisions,
        true,
    );
    canonical_tool_approval_tests::handle(&client, &cx, &space, &source).await;
    assert_eq!(initial.load(Ordering::SeqCst), 1);
    assert_eq!(completed.load(Ordering::SeqCst), 1);
    assert_eq!(
        client
            .app_binary_read(base.item_id.clone(), None, None)
            .expect("cold bytes"),
        doc
    );
    assert_eq!(
        client
            .item(base.item_id.clone())
            .expect("versions")
            .versions
            .len(),
        2
    );
    let read: serde_json::Value = serde_json::from_str(
        &crate::deckcraft::call(
            &client,
            &worker,
            &base.item_id,
            &source,
            &owner,
            "read_gridcraft",
            &json!({"artifactId":base.item_id,"sheet":"Sheet1","cell":"A1"}),
        )
        .await
        .expect("cold actual WASI read"),
    )
    .expect("cell");
    assert_eq!(read["cell"]["value"].as_f64(), Some(7.0));
    assert!(crate::deckcraft::call(
        &client,
        &worker,
        &base.item_id,
        "forged-source",
        &owner,
        "read_gridcraft",
        &json!({"artifactId":base.item_id})
    )
    .await
    .is_err());
    drop(cx);
    drop(client);
    std::fs::remove_dir_all(path).expect("owned fixture cleanup");
    Ok(())
}

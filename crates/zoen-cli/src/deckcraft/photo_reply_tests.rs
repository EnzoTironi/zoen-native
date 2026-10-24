//! Real signed component through the unchanged CLI reply guard and Core CAS.
//! Synthetic offline source/actor; no model, provider, relay or owner UI dispatch.
use super::*;

const PHOTO: &[u8] =
    include_bytes!("../../../miniapps/src/photocraft/fixtures/native-asymmetric.pcraft");
const ROTATED: &[u8] =
    include_bytes!("../../../miniapps/src/photocraft/fixtures/native-rotated.pcraft");

fn photo_worker() -> DeckcraftWorker {
    let path = std::env::var("ZOEN_TEST_PHOTOCRAFT_COMPONENT")
        .expect("explicit real Photo WASIp2 component required");
    let bytes = bounded_file(&path, 64 * 1024 * 1024).unwrap();
    let key = SigningKey::from_bytes(&[23; 32]);
    let signed = ToolManifest {
        id: "photocraft-canonical-v1".into(),
        version: "9b9643764f3fb77d9f0e48d7510983ae50621e11".into(),
        summary: "Synthetic exact Photo CLI reply qualification".into(),
        needs: Needs::Wasm,
        egress: vec![],
        secrets: vec![],
        component_sha256: Some(hash(&bytes)),
    }
    .sign(&key);
    DeckcraftWorker::load_for("photocraft", vec![key.verifying_key()], &signed, &bytes).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a pinned actual Photo WASIp2 component via ZOEN_TEST_PHOTOCRAFT_COMPONENT"]
async fn actual_photo_reply_survives_cli_guard_and_scoped_core_cas() {
    let path = std::env::temp_dir().join(types::new_id("photo-cli-reply"));
    std::fs::create_dir(&path).unwrap();
    let f = fixture_construction::fixture_for_format_at(
        path,
        [74; 32],
        types::Privacy::Closed,
        "photocraft",
        PHOTO,
    );
    let worker = photo_worker();
    let args = json!({"artifactId":f.base.item_id,"version":f.base.version,
        "sha256":f.base.sha256,"action":"rotate_clockwise"});
    // This is the same call/execute_turn/reply validation/save consumer as normal
    // CLI artifact dispatch. The test does not recreate its operation check.
    let accepted: Value = serde_json::from_str(
        &call(
            &f.client,
            &worker,
            &f.base.item_id,
            &f.source,
            &f.owner,
            "photocraft_transform",
            &args,
        )
        .await
        .expect("actual Photo proposal must match the exact CLI command"),
    )
    .unwrap();
    assert_eq!(accepted["version"], 2);
    assert_eq!(accepted["authorId"], f.base.actor_id);
    assert_eq!(accepted["replayed"], false);
    let head = f
        .client
        .app_binary_read(f.base.item_id.clone(), None, None)
        .unwrap();
    assert_eq!(STANDARD.decode(&head.data_base64).unwrap(), ROTATED);
    assert_eq!(head.sha256, hash(ROTATED));
    let replay: Value = serde_json::from_str(
        &call(
            &f.client,
            &worker,
            &f.base.item_id,
            &f.source,
            &f.owner,
            "photocraft_transform",
            &args,
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(replay["version"], 2);
    assert_eq!(replay["replayed"], true);
    for (source, altered) in [
        ("foreign-source", args.clone()),
        (f.source.as_str(), {
            let mut a = args.clone();
            a["action"] = "flip_horizontal".into();
            a
        }),
        (f.source.as_str(), {
            let mut a = args.clone();
            a["artifactId"] = "foreign-artifact".into();
            a
        }),
        (f.source.as_str(), {
            let mut a = args.clone();
            a["action"] = "file.open".into();
            a
        }),
    ] {
        assert!(call(
            &f.client,
            &worker,
            &f.base.item_id,
            source,
            &f.owner,
            "photocraft_transform",
            &altered
        )
        .await
        .is_err());
        assert_eq!(
            f.client
                .app_binary_read(f.base.item_id.clone(), None, None)
                .unwrap(),
            head
        );
    }
    assert_eq!(
        f.client
            .item(f.base.item_id.clone())
            .unwrap()
            .versions
            .len(),
        2
    );
    drop(f.client);
    std::fs::remove_dir_all(f.path).unwrap();
}

//! Complete selection scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn private_selection_reopens_encrypted_without_a_message_file_or_upload(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = temp()?;
    let path = dir.join("device.sqlite");
    let f = fixture(path.to_str().unwrap());
    let before = counts(&f.engine);
    let draft = save(
        &f,
        "0",
        &attachments(&["photo", "document"], None),
        vec![input("photo"), input("document")],
    );
    assert_eq!(
        counts(&f.engine),
        before,
        "selecting a file must not publish an Item or upload"
    );
    assert_eq!(asset(&f, &draft, "photo"), input("photo").bytes);
    assert!(!draft.attempted);
    assert!(rows(&f)
        .iter()
        .all(|(_, value)| value.starts_with("v1:") && !value.contains("Private")));
    let root = f
        .engine
        .send_message(f.scope.space_id.clone(), "Thread root".into())
        .unwrap();
    let mut thread = f.scope.clone();
    thread.thread_root_id = Some(root.id);
    let thread_draft = f
        .engine
        .save_media_draft(
            thread.clone(),
            "0".into(),
            attachments(&["thread"], None),
            vec![input("thread")],
        )
        .unwrap()
        .draft;
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let engine = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(
        engine.media_draft(scope.clone()).is_err(),
        "locked device cannot expose private media"
    );
    assert!(engine.unlock(vault).unwrap());
    assert_eq!(engine.media_draft(scope.clone()).unwrap(), draft);
    assert_eq!(engine.media_draft(thread).unwrap(), thread_draft);
    assert_eq!(
        engine
            .media_draft_asset(scope, draft.revision, "document".into())
            .unwrap(),
        input("document").bytes
    );
    drop(engine);
    for entry in std::fs::read_dir(&dir).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        assert!(
            !bytes
                .windows(b"Private".len())
                .any(|part| part == b"Private"),
            "SQLite/WAL leaked plaintext content or bytes"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();

    Ok(())
}

pub(super) fn compare_and_swap_preserves_immutable_assets_and_prunes_only_removed_references() {
    let f = fixture(":memory:");
    let first = save(
        &f,
        "0",
        &attachments(&["a", "b"], None),
        vec![input("a"), input("b")],
    );
    let before = rows(&f);
    let mut drift = input("a");
    drift.bytes.push(9);
    assert!(f
        .engine
        .save_media_draft(
            f.scope.clone(),
            first.revision.clone(),
            attachments(&["a"], None),
            vec![drift]
        )
        .is_err());
    assert_eq!(rows(&f), before);
    assert!(matches!(
        f.engine
            .save_media_draft(f.scope.clone(), "0".into(), "null".into(), vec![]),
        Err(CoreError::Stale { .. })
    ));
    let next = save(
        &f,
        &first.revision,
        &attachments(&["b", "c"], None),
        vec![input("c")],
    );
    assert_eq!(
        next.assets
            .iter()
            .map(|a| a.id.as_str())
            .collect::<Vec<_>>(),
        ["b", "c"]
    );
    assert_eq!(asset(&f, &next, "b"), input("b").bytes);
    assert_eq!(rows(&f).len(), 3);
    assert!(f
        .engine
        .media_draft_asset(f.scope.clone(), next.revision.clone(), "a".into())
        .is_err());
    assert!(matches!(
        f.engine
            .media_draft_asset(f.scope.clone(), first.revision, "b".into()),
        Err(CoreError::Stale { .. })
    ));
    let clear = save(&f, &next.revision, "null", vec![]);
    assert_eq!(clear.content_json, "null");
    assert!(clear.assets.is_empty());
    assert_ne!(clear.revision, "0");
    assert_eq!(rows(&f).len(), 1);
}

pub(super) fn owner_membership_reply_and_thread_admission_precede_attempted_or_publication() {
    let f = fixture(":memory:");
    let root = f
        .engine
        .send_message(f.scope.space_id.clone(), "Root".into())
        .unwrap();
    let draft = save(
        &f,
        "0",
        &attachments(&["a"], Some(&root.id)),
        vec![input("a")],
    );
    f.engine
        .delete_message(f.scope.space_id.clone(), root.id)
        .unwrap();
    let before = counts(&f.engine);
    assert!(matches!(
        f.engine
            .send_media_draft(f.scope.clone(), draft.revision.clone()),
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(counts(&f.engine), before);
    assert!(!read(&f).attempted);
    let mut wrong = f.scope.clone();
    wrong.expected_device_id.push('x');
    assert!(f.engine.media_draft(wrong.clone()).is_err());
    assert!(f
        .engine
        .media_draft_asset(wrong.clone(), draft.revision.clone(), "a".into())
        .is_err());
    assert!(f.engine.send_media_draft(wrong, draft.revision).is_err());
    let clear = save(&f, &read(&f).revision, "null", vec![]);
    f.engine
        .lock()
        .append(
            &f.scope.space_id,
            &f.scope.expected_identity_id,
            types::EventBody::MemberAdded {
                identity: f.scope.expected_identity_id.clone(),
                role: types::Role::Reader,
            },
        )
        .unwrap();
    let draft = save(
        &f,
        &clear.revision,
        &attachments(&["b"], None),
        vec![input("b")],
    );
    let before = counts(&f.engine);
    assert!(matches!(
        f.engine.send_media_draft(f.scope.clone(), draft.revision),
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(counts(&f.engine), before);
    assert!(!read(&f).attempted);
}

pub(super) fn ciphertext_corruption_or_asset_substitution_never_publishes_or_overwrites_original_bytes(
) {
    let f = fixture(":memory:");
    let draft = save(
        &f,
        "0",
        &attachments(&["a", "b"], None),
        vec![input("a"), input("b")],
    );
    let storage = f.engine.lock().media_draft_scope(&f.scope).unwrap();
    let originals = rows(&f);
    let a = storage.asset_key("a");
    let b = storage.asset_key("b");
    let a_cipher = originals
        .iter()
        .find(|(key, _)| key == &a)
        .unwrap()
        .1
        .clone();
    f.engine.lock().store.set_meta(&b, &a_cipher).unwrap();
    let before = counts(&f.engine);
    assert!(matches!(
        f.engine
            .media_draft_asset(f.scope.clone(), draft.revision.clone(), "b".into()),
        Err(CoreError::Storage { .. })
    ));
    assert!(f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert!(!read(&f).attempted);
    let record_cipher = originals
        .iter()
        .find(|(key, _)| key == &storage.id)
        .unwrap()
        .1
        .clone();
    f.engine
        .lock()
        .store
        .set_meta(&storage.id, "v1:corrupt")
        .unwrap();
    assert!(matches!(
        f.engine.media_draft(f.scope.clone()),
        Err(CoreError::Storage { .. })
    ));
    assert!(f
        .engine
        .save_media_draft(
            f.scope.clone(),
            draft.revision.clone(),
            "null".into(),
            vec![]
        )
        .is_err());
    assert_eq!(
        f.engine.lock().store.meta(&storage.id).unwrap().as_deref(),
        Some("v1:corrupt")
    );
    f.engine
        .lock()
        .store
        .set_meta(&storage.id, &record_cipher)
        .unwrap();
    for (key, value) in originals {
        f.engine.lock().store.set_meta(&key, &value).unwrap();
    }
    assert_eq!(asset(&f, &draft, "b"), input("b").bytes);
}

pub(super) fn malformed_content_duplicate_ids_and_invalid_voice_metadata_fail_without_private_or_public_writes(
) {
    let f = fixture(":memory:");
    let cases = [
        serde_json::json!({"kind":"attachments","reply_to":null,"items":[]}),
        serde_json::json!({"kind":"attachments","reply_to":null,"items":[{"asset_id":"a","caption":"x","mutation_id":"x"},{"asset_id":"a","caption":"y","mutation_id":"y"}]}),
        serde_json::json!({"kind":"attachments","reply_to":null,"items":[{"asset_id":"a","caption":"x\u{0}","mutation_id":"x"}]}),
        serde_json::json!({"kind":"attachments","reply_to":null,"items":[{"asset_id":"a","caption":"x","mutation_id":"x"}],"unknown":true}),
    ];
    let before = counts(&f.engine);
    for json in cases {
        assert!(f
            .engine
            .save_media_draft(
                f.scope.clone(),
                "0".into(),
                json.to_string(),
                vec![input("a")]
            )
            .is_err());
        assert!(rows(&f).is_empty());
    }
    let mut bad_voice: serde_json::Value = serde_json::from_str(&voice(true)).unwrap();
    bad_voice["render"]["duration_ms"] = 249.0.into();
    assert!(f
        .engine
        .save_media_draft(
            f.scope.clone(),
            "0".into(),
            bad_voice.to_string(),
            vec![audio("original"), audio("edited")]
        )
        .is_err());
    bad_voice["render"]["duration_ms"] = 900.0.into();
    bad_voice["render"]["levels"] = serde_json::json!([1.1]);
    assert!(f
        .engine
        .save_media_draft(
            f.scope.clone(),
            "0".into(),
            bad_voice.to_string(),
            vec![audio("original"), audio("edited")]
        )
        .is_err());
    assert!(f
        .engine
        .save_media_draft(
            f.scope.clone(),
            "0".into(),
            " ".repeat(MAX_CONTENT_BYTES + 1),
            vec![]
        )
        .is_err());
    let mut empty = input("a");
    empty.bytes.clear();
    assert!(f
        .engine
        .save_media_draft(
            f.scope.clone(),
            "0".into(),
            attachments(&["a"], None),
            vec![empty]
        )
        .is_err());
    assert!(rows(&f).is_empty());
    assert_eq!(counts(&f.engine), before);
}

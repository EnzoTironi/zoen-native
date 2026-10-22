//! Whole failure recovery cases on the same original parent fixture.

use super::*;

pub(super) fn corrupted_ciphertexts_fail_closed_and_preserve_every_captured_byte_and_canonical_version(
) {
    let f = fixture(":memory:");
    let a = stage(&f, "0", "editor", "a");
    let storage = f.engine.lock().file_edit_draft_scope(&f.scope).unwrap();
    let key = storage.asset_key("a");
    let saved = f.engine.lock().store.meta(&key).unwrap().unwrap();
    f.engine.lock().store.set_meta(&key, "v1:corrupt").unwrap();
    let before = counts(&f.engine);
    let retained = rows(&f);
    assert!(matches!(
        export(&f, &a.draft.revision, "a"),
        Err(CoreError::Storage { .. })
    ));
    assert!(f
        .engine
        .commit_file_edit_draft(
            f.scope.clone(),
            "editor".into(),
            "a".into(),
            "Annotations".into()
        )
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert_eq!(rows(&f), retained);
    f.engine.lock().store.set_meta(&key, &saved).unwrap();
    f.engine
        .lock()
        .store
        .set_meta(&storage.id, "v1:corrupt")
        .unwrap();
    let retained = rows(&f);
    assert!(f.engine.file_edit_draft(f.scope.clone()).is_err());
    assert!(f
        .engine
        .discard_file_edit_draft(f.scope.clone(), a.draft.revision)
        .is_err());
    assert_eq!(rows(&f), retained);
}

pub(super) fn failed_private_writes_and_canonical_or_receipt_writes_rollback_and_retry_exactly_once(
) {
    let f = fixture(":memory:");
    for kind in ["asset", "record"] {
        let sql = format!("CREATE TRIGGER fail_private BEFORE INSERT ON meta WHEN NEW.key LIKE 'file.edit.draft.{kind}.%' BEGIN SELECT RAISE(ABORT,'private failed'); END;");
        f.engine.lock().store.conn().execute_batch(&sql).unwrap();
        let before = counts(&f.engine);
        assert!(f
            .engine
            .stage_file_edit_draft(
                f.scope.clone(),
                "0".into(),
                "editor".into(),
                "a".into(),
                1,
                f.origin.sha256.clone(),
                pdf("a")
            )
            .is_err());
        assert!(rows(&f).is_empty());
        assert_eq!(counts(&f.engine), before);
        f.engine
            .lock()
            .store
            .conn()
            .execute_batch("DROP TRIGGER fail_private")
            .unwrap();
    }
    let a = stage(&f, "0", "editor", "a");
    for sql in [
        "CREATE TRIGGER fail_commit BEFORE INSERT ON outbox WHEN NEW.json LIKE '%ItemVersioned%' BEGIN SELECT RAISE(ABORT,'append failed'); END;",
        "CREATE TRIGGER fail_commit BEFORE INSERT ON meta WHEN NEW.key LIKE 'file.edit.draft.record.%' BEGIN SELECT RAISE(ABORT,'receipt failed'); END;",
    ] {
        f.engine.lock().store.conn().execute_batch(sql).unwrap(); let before = counts(&f.engine); let retained = rows(&f);
        assert!(f.engine.commit_file_edit_draft(f.scope.clone(), "editor".into(), "a".into(), "Annotations".into()).is_err());
        assert_eq!(counts(&f.engine), before); assert_eq!(rows(&f), retained); assert_eq!(read(&f), a.draft);
        assert_eq!(f.engine.file_bytes(f.scope.item_id.clone(), None).unwrap().unwrap(), pdf("original"));
        f.engine.lock().store.conn().execute_batch("DROP TRIGGER fail_commit").unwrap();
    }
    let accepted = commit(&f, "editor", "a");
    assert_eq!(accepted.version, 2);
    let before = counts(&f.engine);
    assert_eq!(commit(&f, "editor", "a"), accepted);
    assert_eq!(counts(&f.engine), before);
}

pub(super) fn validation_queue_bounds_and_same_bytes_commit_preserve_existing_file_semantics() {
    let f = fixture(":memory:");
    let before = counts(&f.engine);
    for bytes in [
        Vec::new(),
        b"not a PDF".to_vec(),
        vec![b'x'; MAX_ASSET_BYTES + 1],
    ] {
        assert!(f
            .engine
            .stage_file_edit_draft(
                f.scope.clone(),
                "0".into(),
                "editor".into(),
                "bad".into(),
                1,
                f.origin.sha256.clone(),
                bytes
            )
            .is_err());
    }
    assert_eq!(counts(&f.engine), before);
    assert!(rows(&f).is_empty());
    let same = f
        .engine
        .stage_file_edit_draft(
            f.scope.clone(),
            "0".into(),
            "editor".into(),
            "same".into(),
            1,
            f.origin.sha256.clone(),
            pdf("original"),
        )
        .unwrap();
    let before = counts(&f.engine);
    let result = commit(&f, "editor", "same");
    assert_eq!(result, f.origin);
    assert_eq!(counts(&f.engine), before);
    assert!(read(&f).pending.is_empty());
    let mut current = read(&f);
    for i in 0..MAX_PENDING {
        current = stage(&f, &current.revision, "editor", &format!("capture-{i}")).draft;
    }
    let retained = rows(&f);
    assert!(f
        .engine
        .stage_file_edit_draft(
            f.scope.clone(),
            current.revision,
            "editor".into(),
            "overflow".into(),
            1,
            f.origin.sha256.clone(),
            pdf("overflow")
        )
        .is_err());
    assert_eq!(rows(&f), retained);
    assert_ne!(same.accepted_revision, "0");
}

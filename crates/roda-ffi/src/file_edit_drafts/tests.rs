use super::*;
use crate::{PrivacyDto, SecretVault};
use roda_types::{EventBody, Role};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

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
    engine: Arc<RodaEngine>,
    vault: Arc<Vault>,
    scope: FileEditDraftScope,
    origin: FileEditDraftCommitResult,
}
fn pdf(id: &str) -> Vec<u8> {
    format!("%PDF-1.7\n% Private edited PDF snapshot {id}\n%%EOF\n").into_bytes()
}
fn fixture(path: &str) -> Fixture {
    let engine = RodaEngine::open(path.into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    engine
        .create_account(
            "PDF editor".into(),
            "pdf_editor".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    let space = engine
        .create_group_with("PDF edits".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let item = engine
        .file_add(
            space.clone(),
            "document.pdf".into(),
            "document.pdf".into(),
            "application/pdf".into(),
            pdf("original"),
            None,
        )
        .unwrap();
    let account = engine.account().unwrap();
    let origin = FileEditDraftCommitResult {
        version: item.version,
        sha256: item.file.unwrap().sha256,
    };
    Fixture {
        engine,
        vault,
        scope: FileEditDraftScope {
            expected_identity_id: account.identity_id,
            expected_device_id: account.device_id,
            space_id: space,
            item_id: item.id,
        },
        origin,
    }
}
fn read(f: &Fixture) -> FileEditDraftDto {
    f.engine.file_edit_draft(f.scope.clone()).unwrap()
}
fn stage(f: &Fixture, expected: &str, editor: &str, capture: &str) -> FileEditDraftSaveResult {
    f.engine
        .stage_file_edit_draft(
            f.scope.clone(),
            expected.into(),
            editor.into(),
            capture.into(),
            f.origin.version,
            f.origin.sha256.clone(),
            pdf(capture),
        )
        .unwrap()
}
fn commit(f: &Fixture, editor: &str, capture: &str) -> FileEditDraftCommitResult {
    f.engine
        .commit_file_edit_draft(
            f.scope.clone(),
            editor.into(),
            capture.into(),
            "Annotations".into(),
        )
        .unwrap()
}
fn export(f: &Fixture, revision: &str, capture: &str) -> R<Vec<u8>> {
    f.engine
        .file_edit_draft_bytes(f.scope.clone(), revision.into(), capture.into())
}
fn counts(e: &RodaEngine) -> Vec<i64> {
    let e = e.lock();
    let mut result = ["events", "media", "media_keys", "blob_uploads", "outbox"]
        .map(|table| {
            e.store
                .conn()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        })
        .to_vec();
    result.push(
        e.store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key NOT LIKE 'file.edit.draft.%'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
    );
    result
}
fn rows(f: &Fixture) -> Vec<(String, String)> {
    let e = f.engine.lock();
    let mut statement = e
        .store
        .conn()
        .prepare("SELECT key,value FROM meta WHERE key LIKE 'file.edit.draft.%' ORDER BY key")
        .unwrap();
    statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}
fn temp() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(roda_types::new_id("file-edit-draft-test"));
    std::fs::create_dir_all(&path).unwrap();
    path
}
fn role(f: &Fixture, role: Role) {
    f.engine
        .lock()
        .append(
            &f.scope.space_id,
            &f.scope.expected_identity_id,
            EventBody::MemberAdded {
                identity: f.scope.expected_identity_id.clone(),
                role,
            },
        )
        .unwrap();
}

#[test]
fn complete_captures_are_private_encrypted_and_survive_a_locked_cold_reopen() {
    let dir = temp();
    let path = dir.join("device.sqlite");
    let f = fixture(path.to_str().unwrap());
    let before = counts(&f.engine);
    let a = stage(&f, "0", "editor", "a");
    let b = stage(&f, &a.draft.revision, "editor", "b");
    assert_eq!(
        counts(&f.engine),
        before,
        "private capture must create no canonical version, media or upload"
    );
    assert_eq!(export(&f, &b.draft.revision, "a").unwrap(), pdf("a"));
    assert!(
        export(&f, &a.draft.revision, "a").is_err(),
        "byte export is exact revision CAS"
    );
    assert!(rows(&f).iter().all(|(_, value)| value.starts_with("v1:")
        && !value.contains("Private")
        && !value.contains("editor")));
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let engine = RodaEngine::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(engine.file_edit_draft(scope.clone()).is_err());
    assert!(engine.unlock(vault).unwrap());
    assert_eq!(engine.file_edit_draft(scope.clone()).unwrap(), b.draft);
    assert_eq!(
        engine
            .file_edit_draft_bytes(scope.clone(), b.draft.revision, "b".into())
            .unwrap(),
        pdf("b")
    );
    assert_eq!(
        engine.file_bytes(scope.item_id, None).unwrap().unwrap(),
        pdf("original")
    );
    drop(engine);
    for file in std::fs::read_dir(&dir).unwrap() {
        let bytes = std::fs::read(file.unwrap().path()).unwrap();
        assert!(!bytes
            .windows(b"Private edited PDF snapshot a".len())
            .any(|v| v == b"Private edited PDF snapshot a"));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn fifo_keeps_all_snapshots_and_advances_only_its_own_base_while_the_origin_stays_pinned() {
    let f = fixture(":memory:");
    let a = stage(&f, "0", "editor", "a");
    let b = stage(&f, &a.draft.revision, "editor", "b");
    assert!(matches!(
        f.engine.commit_file_edit_draft(
            f.scope.clone(),
            "editor".into(),
            "b".into(),
            "Annotations".into()
        ),
        Err(CoreError::Stale { .. })
    ));
    let first = commit(&f, "editor", "a");
    assert_eq!(first.version, 2);
    let after_a = read(&f);
    assert_eq!(
        after_a
            .pending
            .iter()
            .map(|c| c.capture_id.as_str())
            .collect::<Vec<_>>(),
        ["b"]
    );
    assert_eq!(after_a.base_version, Some(2));
    assert_eq!(after_a.origin_version, Some(1));
    // The same QuickLook editor captures C while its A commit ACK is still unseen.
    let c = stage(&f, &after_a.revision, "editor", "c");
    assert_eq!(
        c.draft.origin_sha256.as_deref(),
        Some(f.origin.sha256.as_str())
    );
    assert_eq!(
        stage(&f, "0", "editor", "a").accepted_revision,
        a.accepted_revision
    );
    assert_eq!(stage(&f, "0", "editor", "a").draft, c.draft);
    assert!(
        export(&f, &c.draft.revision, "a").is_err(),
        "accepted replay must never resurrect consumed bytes"
    );
    assert_eq!(commit(&f, "editor", "b").version, 3);
    assert_eq!(commit(&f, "editor", "c").version, 4);
    for (version, capture) in [(1, "original"), (2, "a"), (3, "b"), (4, "c")] {
        assert_eq!(
            f.engine
                .file_bytes(f.scope.item_id.clone(), Some(version))
                .unwrap()
                .unwrap(),
            pdf(capture)
        );
    }
    let empty = read(&f);
    assert!(empty.pending.is_empty());
    assert_eq!(empty.origin_version, Some(1));
    assert_eq!(empty.base_version, Some(4));
    let d = stage(&f, &empty.revision, "editor", "d");
    assert_eq!(d.draft.origin_version, Some(1));
    assert_eq!(commit(&f, "editor", "d").version, 5);
    assert_eq!(b.draft.pending.len(), 2);
}

#[test]
fn lost_commit_ack_reopens_and_returns_its_exact_receipt_without_touching_newer_edits_or_versions()
{
    let dir = temp();
    let path = dir.join("device.sqlite");
    let f = fixture(path.to_str().unwrap());
    let a = stage(&f, "0", "editor", "a");
    let result = commit(&f, "editor", "a");
    let b = stage(&f, &read(&f).revision, "editor", "b");
    // A separate writer changes the canonical file. A retry of accepted A is still safe.
    f.engine
        .file_new_version(
            f.scope.item_id.clone(),
            pdf("remote"),
            None,
            "Remote".into(),
        )
        .unwrap();
    role(&f, Role::Reader);
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let engine = RodaEngine::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(engine.unlock(vault).unwrap());
    let before = counts(&engine);
    assert_eq!(
        engine
            .commit_file_edit_draft(
                scope.clone(),
                "editor".into(),
                "a".into(),
                "Different retry note".into()
            )
            .unwrap(),
        result
    );
    assert_eq!(engine.file_edit_draft(scope.clone()).unwrap(), b.draft);
    assert_eq!(counts(&engine), before);
    let replay = engine
        .stage_file_edit_draft(
            scope.clone(),
            "0".into(),
            "editor".into(),
            "a".into(),
            1,
            a.draft.origin_sha256.unwrap(),
            pdf("a"),
        )
        .unwrap();
    assert_eq!(replay.accepted_revision, a.accepted_revision);
    assert_eq!(replay.draft, b.draft);
    assert!(engine
        .commit_file_edit_draft(scope, "editor".into(), "b".into(), "Annotations".into())
        .is_err());
    assert_eq!(counts(&engine), before);
    drop(engine);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn remote_change_preserves_late_full_capture_but_never_rebases_or_publishes_it() {
    let f = fixture(":memory:");
    let a = stage(&f, "0", "editor", "a");
    f.engine
        .file_new_version(
            f.scope.item_id.clone(),
            pdf("remote"),
            None,
            "Remote".into(),
        )
        .unwrap();
    let b = stage(&f, &a.draft.revision, "editor", "b");
    let before = counts(&f.engine);
    let retained = rows(&f);
    assert!(matches!(
        f.engine.commit_file_edit_draft(
            f.scope.clone(),
            "editor".into(),
            "a".into(),
            "Annotations".into()
        ),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(counts(&f.engine), before);
    assert_eq!(rows(&f), retained);
    assert_eq!(export(&f, &b.draft.revision, "b").unwrap(), pdf("b"));
    assert_eq!(
        f.engine
            .file_bytes(f.scope.item_id.clone(), None)
            .unwrap()
            .unwrap(),
        pdf("remote")
    );
}

#[test]
fn stage_replay_authenticates_all_bytes_and_binding_inputs_and_discard_is_explicit_cas() {
    let f = fixture(":memory:");
    let a = stage(&f, "0", "editor", "a");
    for (editor, capture, version, sha, bytes) in [
        ("other", "a", 1, f.origin.sha256.clone(), pdf("a")),
        ("editor", "other", 1, f.origin.sha256.clone(), pdf("a")),
        ("editor", "a", 2, f.origin.sha256.clone(), pdf("a")),
        ("editor", "a", 1, "0".repeat(64), pdf("a")),
        (
            "editor",
            "a",
            1,
            f.origin.sha256.clone(),
            pdf("changed bytes"),
        ),
    ] {
        assert!(f
            .engine
            .stage_file_edit_draft(
                f.scope.clone(),
                "0".into(),
                editor.into(),
                capture.into(),
                version,
                sha,
                bytes
            )
            .is_err());
    }
    assert_eq!(read(&f), a.draft);
    assert!(f
        .engine
        .discard_file_edit_draft(f.scope.clone(), "0".into())
        .is_err());
    let cleared = f
        .engine
        .discard_file_edit_draft(f.scope.clone(), a.draft.revision.clone())
        .unwrap();
    assert!(cleared.pending.is_empty());
    assert_ne!(cleared.revision, "0");
    let replay = stage(&f, "0", "editor", "a");
    assert_eq!(replay.accepted_revision, a.accepted_revision);
    assert_eq!(replay.draft, cleared);
    assert!(export(&f, &cleared.revision, "a").is_err());
    assert!(f
        .engine
        .commit_file_edit_draft(
            f.scope.clone(),
            "editor".into(),
            "a".into(),
            "Annotations".into()
        )
        .is_err());
    assert_eq!(f.engine.item(f.scope.item_id.clone()).unwrap().version, 1);
    let empty = f
        .engine
        .discard_file_edit_draft(f.scope.clone(), cleared.revision)
        .unwrap();
    assert!(f
        .engine
        .stage_file_edit_draft(
            f.scope.clone(),
            "0".into(),
            "new".into(),
            "new".into(),
            1,
            f.origin.sha256.clone(),
            pdf("new")
        )
        .is_err());
    let claimed = stage(&f, &empty.revision, "new", "new");
    assert_eq!(claimed.draft.editor_id.as_deref(), Some("new"));
}

#[test]
fn editor_claims_are_exclusive_until_empty_and_preserve_historical_origin_without_rebasing() {
    let f = fixture(":memory:");
    let a = stage(&f, "0", "editor", "a");
    assert!(f
        .engine
        .stage_file_edit_draft(
            f.scope.clone(),
            a.draft.revision.clone(),
            "other".into(),
            "other".into(),
            1,
            f.origin.sha256.clone(),
            pdf("other")
        )
        .is_err());
    commit(&f, "editor", "a");
    let empty = read(&f);
    let other = f
        .engine
        .stage_file_edit_draft(
            f.scope.clone(),
            empty.revision,
            "other".into(),
            "other".into(),
            1,
            f.origin.sha256.clone(),
            pdf("other"),
        )
        .unwrap();
    assert_eq!(other.draft.origin_version, Some(1));
    assert_eq!(other.draft.base_version, Some(1));
    assert!(matches!(
        f.engine.commit_file_edit_draft(
            f.scope.clone(),
            "other".into(),
            "other".into(),
            "Annotations".into()
        ),
        Err(CoreError::Stale { .. })
    ));
    assert!(f
        .engine
        .stage_file_edit_draft(
            f.scope.clone(),
            other.draft.revision,
            "editor".into(),
            "b".into(),
            1,
            f.origin.sha256.clone(),
            pdf("b")
        )
        .is_err());
}

#[test]
fn first_capture_after_a_remote_change_is_durable_but_can_never_overwrite_that_remote_version() {
    let f = fixture(":memory:");
    f.engine
        .file_new_version(
            f.scope.item_id.clone(),
            pdf("remote-before-first-capture"),
            None,
            "Remote".into(),
        )
        .unwrap();
    let before = counts(&f.engine);
    let captured = stage(&f, "0", "opened-before-remote", "late-first");
    assert_eq!(captured.draft.base_version, Some(1));
    assert_eq!(captured.draft.origin_version, Some(1));
    assert_eq!(counts(&f.engine), before);
    assert_eq!(
        export(&f, &captured.draft.revision, "late-first").unwrap(),
        pdf("late-first")
    );
    assert!(matches!(
        f.engine.commit_file_edit_draft(
            f.scope.clone(),
            "opened-before-remote".into(),
            "late-first".into(),
            "Annotations".into()
        ),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(read(&f), captured.draft);
    assert_eq!(
        f.engine
            .file_bytes(f.scope.item_id.clone(), None)
            .unwrap()
            .unwrap(),
        pdf("remote-before-first-capture")
    );
    assert_eq!(counts(&f.engine), before);
}

#[test]
fn exact_owner_device_item_space_membership_and_writer_checks_guard_every_boundary() {
    let f = fixture(":memory:");
    let a = stage(&f, "0", "editor", "a");
    let other_space = f
        .engine
        .create_group_with("Other PDF".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    for scope in [
        FileEditDraftScope {
            expected_identity_id: "foreign".into(),
            ..f.scope.clone()
        },
        FileEditDraftScope {
            expected_device_id: "foreign".into(),
            ..f.scope.clone()
        },
        FileEditDraftScope {
            space_id: other_space,
            ..f.scope.clone()
        },
    ] {
        assert!(f.engine.file_edit_draft(scope.clone()).is_err());
        assert!(f
            .engine
            .file_edit_draft_bytes(scope.clone(), a.draft.revision.clone(), "a".into())
            .is_err());
        assert!(f
            .engine
            .commit_file_edit_draft(
                scope.clone(),
                "editor".into(),
                "a".into(),
                "Annotations".into()
            )
            .is_err());
        assert!(f
            .engine
            .discard_file_edit_draft(scope, a.draft.revision.clone())
            .is_err());
    }
    role(&f, Role::Reader);
    let before = counts(&f.engine);
    assert!(f
        .engine
        .stage_file_edit_draft(
            f.scope.clone(),
            a.draft.revision.clone(),
            "editor".into(),
            "b".into(),
            1,
            f.origin.sha256.clone(),
            pdf("b")
        )
        .is_err());
    assert!(f
        .engine
        .commit_file_edit_draft(
            f.scope.clone(),
            "editor".into(),
            "a".into(),
            "Annotations".into()
        )
        .is_err());
    assert_eq!(export(&f, &a.draft.revision, "a").unwrap(), pdf("a"));
    assert_eq!(counts(&f.engine), before);
    let removed = fixture(":memory:");
    let captured = stage(&removed, "0", "editor", "a");
    removed
        .engine
        .lock()
        .append(
            &removed.scope.space_id,
            &removed.scope.expected_identity_id,
            EventBody::MemberRemoved {
                identity: removed.scope.expected_identity_id.clone(),
            },
        )
        .unwrap();
    assert!(removed
        .engine
        .file_edit_draft(removed.scope.clone())
        .is_err());
    assert!(export(&removed, &captured.draft.revision, "a").is_err());
    assert!(removed
        .engine
        .discard_file_edit_draft(removed.scope.clone(), captured.draft.revision)
        .is_err());
}

#[test]
fn origin_hash_and_current_pdf_type_are_canonical_and_cannot_be_supplied_by_the_caller() {
    let f = fixture(":memory:");
    let before = counts(&f.engine);
    for (version, sha) in [(1, "0".repeat(64)), (2, f.origin.sha256.clone())] {
        assert!(f
            .engine
            .stage_file_edit_draft(
                f.scope.clone(),
                "0".into(),
                "editor".into(),
                "a".into(),
                version,
                sha,
                pdf("a")
            )
            .is_err());
    }
    assert_eq!(counts(&f.engine), before);
    assert!(rows(&f).is_empty());
    let a = stage(&f, "0", "editor", "a");
    f.engine
        .file_new_version_typed(
            f.scope.item_id.clone(),
            b"synthetic image bytes".to_vec(),
            None,
            "Changed type".into(),
            "renamed.pdf".into(),
            "image/png".into(),
        )
        .unwrap();
    let before = counts(&f.engine);
    assert!(f
        .engine
        .stage_file_edit_draft(
            f.scope.clone(),
            a.draft.revision.clone(),
            "editor".into(),
            "b".into(),
            1,
            f.origin.sha256.clone(),
            pdf("b")
        )
        .is_err());
    assert!(f
        .engine
        .commit_file_edit_draft(
            f.scope.clone(),
            "editor".into(),
            "a".into(),
            "Annotations".into()
        )
        .is_err());
    assert_eq!(
        export(&f, &a.draft.revision, "a").unwrap(),
        pdf("a"),
        "private recovery remains possible after type changes"
    );
    assert_eq!(counts(&f.engine), before);
}

#[test]
fn corrupted_ciphertexts_fail_closed_and_preserve_every_captured_byte_and_canonical_version() {
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

#[test]
fn failed_private_writes_and_canonical_or_receipt_writes_rollback_and_retry_exactly_once() {
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

#[test]
fn validation_queue_bounds_and_same_bytes_commit_preserve_existing_file_semantics() {
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

#[test]
fn backup_restore_and_device_erase_exclude_private_pdf_journals_even_from_retained_snapshots() {
    let f = fixture(":memory:");
    let a = stage(&f, "0", "editor", "a");
    // Confirm real membership/file history, so the restored device can access the same PDF.
    let pending = f.engine.lock().store.outbox().unwrap();
    let mut relay = roda_log::SpaceLog::new(f.scope.space_id.clone());
    for pending in pending
        .into_iter()
        .filter(|p| p.event.space == f.scope.space_id)
    {
        let event = relay.sequence(pending.event);
        assert!(matches!(
            f.engine.lock().ingest(roda_proto::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: roda_proto::Envelope::plain(event)
            }),
            crate::sync::Ingest::Confirmed
        ));
    }
    let private = rows(&f);
    let snapshot = f.engine.lock().backup_snapshot().unwrap();
    let dir = temp();
    let path = dir.join("backup.sqlite");
    std::fs::write(&path, snapshot).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM meta WHERE key LIKE 'file.edit.draft.%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    for (key, value) in &private {
        conn.execute(
            "INSERT INTO meta(key,value) VALUES (?1,?2)",
            rusqlite::params![key, value],
        )
        .unwrap();
    }
    drop(conn);
    let header = crate::backup::Header {
        version: 1,
        identity: f.scope.expected_identity_id.clone(),
        relay_url: "http://127.0.0.1:1".into(),
        identity_secret: hex::encode(f.vault.load(crate::sync::VAULT_IDENTITY.into()).unwrap()),
        agreement_secret: hex::encode(
            f.vault
                .load(crate::profile::VAULT_AGREEMENT.into())
                .unwrap(),
        ),
        created_ms: 1,
    };
    let restored = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    restored
        .lock()
        .restore_snapshot(
            &header,
            &std::fs::read(&path).unwrap(),
            roda_log::Signer::generate(),
        )
        .unwrap();
    assert_eq!(
        restored
            .lock()
            .store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key LIKE 'file.edit.draft.%'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let scope = FileEditDraftScope {
        expected_device_id: restored.account().unwrap().device_id,
        ..f.scope.clone()
    };
    assert_eq!(restored.file_edit_draft(scope).unwrap().revision, "0");
    assert_eq!(rows(&f), private);
    assert_eq!(export(&f, &a.draft.revision, "a").unwrap(), pdf("a"));
    f.engine.erase_device(f.vault.clone()).unwrap();
    assert!(rows(&f).is_empty());
    assert!(f.engine.file_edit_draft(f.scope.clone()).is_err());
    drop(restored);
    std::fs::remove_dir_all(dir).unwrap();
}

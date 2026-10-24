//! Checked native file edits bind the write to one observed canonical base.
use super::*;
use rusqlite::types::Value;
use std::collections::BTreeMap;
use types::{EventBody, Role};

const CAPTION: &str = "Signed attachment caption 🇧🇷\nKeep this text";
const ORIGINAL: &[u8] = b"%PDF-1.7\nsynthetic original checked file\n";
const EDITED: &[u8] = b"%PDF-1.7\nsynthetic marked-up checked file\n";

struct Fixture {
    space: String,
    item: String,
    message: String,
}

fn fixture(e: &Client) -> Fixture {
    let space = find_space(e, "Zoen").id;
    let file = e
        .file_add(
            space.clone(),
            "Shared/reports/original.pdf".into(),
            "original.pdf".into(),
            "application/pdf".into(),
            ORIGINAL.to_vec(),
            Some(vec![9; 32]),
        )
        .unwrap();
    let message = {
        let mut engine = e.lock();
        let me = engine.me_id().unwrap();
        engine
            .post_as(&space, &me, CAPTION, Some(file.id.clone()))
            .unwrap()
    };
    Fixture {
        space,
        item: file.id,
        message: message.id,
    }
}

#[derive(Debug, PartialEq)]
struct StorageSnapshot {
    rows: Vec<(String, Vec<Vec<Value>>)>,
    pending: BTreeMap<String, String>,
    items: Vec<ItemDetail>,
    item: ItemDetail,
    timeline: Vec<TimelineEntry>,
}

fn snapshot(e: &Client, f: &Fixture) -> StorageSnapshot {
    let (rows, pending) = {
        let engine = e.lock();
        let mut rows = Vec::new();
        for (table, order) in [
            ("events", "space, seq"),
            ("meta", "key"),
            ("media", "sha256"),
            ("media_keys", "plain_sha"),
            ("blob_uploads", "blob_sha"),
            ("outbox", "client_id"),
        ] {
            let mut query = engine
                .store
                .conn()
                .prepare(&format!("SELECT * FROM {table} ORDER BY {order}"))
                .unwrap();
            let columns = query.column_count();
            let values = query
                .query_map([], |row| {
                    (0..columns)
                        .map(|column| row.get::<_, Value>(column))
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            rows.push((table.to_string(), values));
        }
        let pending = engine
            .net
            .pending
            .iter()
            .map(|(id, space)| (id.clone(), space.clone()))
            .collect();
        (rows, pending)
    };
    let mut items = e.items();
    items.sort_by(|left, right| left.id.cmp(&right.id));
    StorageSnapshot {
        rows,
        pending,
        items,
        item: e.item(f.item.clone()).unwrap(),
        timeline: e.timeline(f.space.clone()).unwrap(),
    }
}

fn base(e: &Client, item: &str) -> (u32, String) {
    let item = e.item(item.into()).unwrap();
    (item.version, item.file.unwrap().sha256)
}

fn checked(e: &Client, item: &str, version: u32, hash: &str) -> Result<ItemDetail, CoreError> {
    e.file_new_version_checked(
        item.into(),
        version,
        hash.into(),
        EDITED.to_vec(),
        Some(vec![5; 48]),
        "Marked up".into(),
    )
}

fn caption(e: &Client, f: &Fixture) -> String {
    let message = e
        .timeline(f.space.clone())
        .unwrap()
        .into_iter()
        .find(|message| message.id == f.message)
        .unwrap();
    match message.kind {
        EntryKind::Message { text, .. } => text,
        other => panic!("expected the original attachment message, got {other:?}"),
    }
}

#[test]
fn checked_file_edit_preserves_original_metadata_caption_history_and_ignores_identical_saves() {
    let e = seeded();
    let f = fixture(&e);
    let original = e.item(f.item.clone()).unwrap();
    let original_info = original.file.clone().unwrap();
    let (version, hash) = base(&e, &f.item);
    let before_preflight = snapshot(&e, &f);
    assert!(e
        .file_edit_allowed(f.item.clone(), version, hash.clone())
        .unwrap());
    assert_eq!(snapshot(&e, &f), before_preflight, "preflight is read-only");

    let edited = checked(&e, &f.item, version, &hash).unwrap();
    assert_eq!(edited.version, version + 1);
    assert_eq!(edited.versions.len(), original.versions.len() + 1);
    assert_eq!(edited.versions[0].note, "Marked up");
    assert_eq!(edited.versions[1], original.versions[0]);
    assert_eq!(edited.created_by, original.created_by);
    assert_eq!(edited.path, original.path);
    let info = edited.file.unwrap();
    assert_eq!(
        (&info.name, &info.path, &info.mime),
        (
            &original_info.name,
            &original_info.path,
            &original_info.mime
        )
    );
    assert_ne!(info.sha256, hash);
    assert_eq!(
        e.file_bytes(f.item.clone(), Some(version)).unwrap(),
        Some(ORIGINAL.to_vec())
    );
    assert_eq!(
        e.file_info_at(f.item.clone(), Some(version)).unwrap(),
        original_info
    );
    assert_eq!(
        e.file_bytes(f.item.clone(), None).unwrap(),
        Some(EDITED.to_vec())
    );
    assert_eq!(e.file_thumbnail(f.item.clone()).unwrap(), Some(vec![5; 48]));
    assert_eq!(caption(&e, &f), CAPTION);
    assert_eq!(e.item_at(f.item.clone(), version).unwrap().version, version);
    assert!(e.verify_log(f.space).valid);
    identical_checked_file_and_thumbnail_do_not_create_a_duplicate_version_or_upload();
}

#[test]
fn stale_version_expected_hash_and_actual_current_hash_refuse_without_any_writes() {
    for drift in ["version", "expected_hash", "current_hash"] {
        let e = seeded();
        let f = fixture(&e);
        let (original_version, original_hash) = base(&e, &f.item);
        if drift != "expected_hash" {
            e.file_new_version(
                f.item.clone(),
                b"Another participant's real new file bytes".to_vec(),
                Some(vec![3; 29]),
                "Concurrent edit".into(),
            )
            .unwrap();
        }
        let version = if drift == "current_hash" {
            base(&e, &f.item).0
        } else {
            original_version
        };
        let expected_hash = if drift == "expected_hash" {
            "00".repeat(32)
        } else {
            original_hash
        };
        let before = snapshot(&e, &f);
        assert!(
            !e.file_edit_allowed(f.item.clone(), version, expected_hash.clone())
                .unwrap(),
            "{drift}"
        );
        assert!(
            matches!(
                checked(&e, &f.item, version, &expected_hash),
                Err(CoreError::Stale { .. })
            ),
            "{drift}"
        );
        assert_eq!(snapshot(&e, &f), before, "{drift}");
    }
}

#[test]
fn checked_file_edit_keeps_existing_errors_without_writes_and_uses_the_session_language() {
    let e = seeded();
    let f = fixture(&e);
    let plan = paraty_plan(&e).id;
    let before = snapshot(&e, &f);
    for item in ["missing-file", plan.as_str()] {
        let allowed = e.file_edit_allowed(item.into(), 1, "00".repeat(32));
        let mutation = checked(&e, item, 1, &"00".repeat(32));
        if item == "missing-file" {
            assert!(matches!(allowed, Err(CoreError::NotFound { .. })));
            assert!(matches!(mutation, Err(CoreError::NotFound { .. })));
        } else {
            assert!(matches!(allowed, Err(CoreError::Invalid { .. })));
            assert!(matches!(mutation, Err(CoreError::Invalid { .. })));
        }
        assert_eq!(snapshot(&e, &f), before);
    }
    checked_file_errors_use_session_language_and_restore_the_ambient_thread_language();
}

#[test]
fn reader_and_removed_member_cannot_edit_even_with_the_exact_base() {
    for removed in [false, true] {
        let e = seeded();
        let f = fixture(&e);
        let (version, hash) = base(&e, &f.item);
        let me = e.me().unwrap().id;
        let body = if removed {
            EventBody::MemberRemoved {
                identity: me.clone(),
            }
        } else {
            EventBody::MemberAdded {
                identity: me.clone(),
                role: Role::Reader,
            }
        };
        e.lock().append(&f.space, &me, body).unwrap();
        let before = snapshot(&e, &f);
        assert!(!e
            .file_edit_allowed(f.item.clone(), version, hash.clone())
            .unwrap());
        assert!(matches!(
            checked(&e, &f.item, version, &hash),
            Err(CoreError::Forbidden { .. })
        ));
        assert_eq!(snapshot(&e, &f), before, "removed={removed}");
    }
}

#[test]
fn foreign_voice_owner_is_denied_even_after_the_owner_changes_its_visible_file_type() {
    for hidden in [false, true] {
        let e = seeded();
        let space = find_space(&e, "Paraty com a Marina");
        let sent = e
            .send_voice_note(
                space.id.clone(),
                vec![7; 900],
                "audio/mp4".into(),
                1500,
                vec![0.2],
                "Synthetic voice transcript".into(),
                None,
                false,
            )
            .unwrap();
        let item = sent.voice.unwrap().item_id;
        if hidden {
            e.file_new_version_typed(
                item.clone(),
                vec![8; 500],
                None,
                "Owner changed format".into(),
                "renamed.pdf".into(),
                "application/pdf".into(),
            )
            .unwrap();
        }
        let other = space
            .members
            .iter()
            .find(|person| !person.is_me)
            .unwrap()
            .id
            .clone();
        {
            let mut engine = e.lock();
            assert!(engine.state.writer(&space.id, &other));
            engine.me = Some(other);
        }
        let f = Fixture {
            space: space.id,
            item,
            message: sent.id,
        };
        let (version, hash) = base(&e, &f.item);
        let before = snapshot(&e, &f);
        assert!(!e
            .file_edit_allowed(f.item.clone(), version, hash.clone())
            .unwrap());
        assert!(matches!(
            checked(&e, &f.item, version, &hash),
            Err(CoreError::Forbidden { .. })
        ));
        assert_eq!(snapshot(&e, &f), before, "hidden={hidden}");
    }
}

fn identical_checked_file_and_thumbnail_do_not_create_a_duplicate_version_or_upload() {
    for synced in [false, true] {
        let e = seeded();
        let f = fixture(&e);
        if synced {
            let mut engine = e.lock();
            engine.store.set_synced(&f.space).unwrap();
            engine.net.synced.insert(f.space.clone());
        }
        let (version, hash) = base(&e, &f.item);
        let before = snapshot(&e, &f);
        let unchanged = e
            .file_new_version_checked(
                f.item.clone(),
                version,
                hash,
                ORIGINAL.to_vec(),
                Some(vec![9; 32]),
                "This note must not manufacture another version".into(),
            )
            .unwrap();
        assert_eq!(unchanged.version, version);
        assert_eq!(snapshot(&e, &f), before, "synced={synced}");
    }
}

#[test]
fn rejected_item_version_append_restores_every_sql_row_projection_and_upload_then_retries() {
    for synced in [false, true] {
        let e = seeded();
        let f = fixture(&e);
        {
            let mut engine = e.lock();
            if synced {
                engine.store.set_synced(&f.space).unwrap();
                engine.net.synced.insert(f.space.clone());
            }
            // A receiver may know per-chunk keys without a locally cached file
            // master key. Failure must roll back a newly generated key as well.
            engine
                .store
                .conn()
                .execute(
                    "DELETE FROM meta WHERE key = ?1",
                    [format!("file_key:{}", f.item)],
                )
                .unwrap();
            let table = if synced { "outbox" } else { "events" };
            engine.store.conn().execute_batch(&format!(
                "CREATE TRIGGER fail_checked_file BEFORE INSERT ON {table} WHEN NEW.json LIKE '%ItemVersioned%' BEGIN SELECT RAISE(ABORT,'checked file append failed'); END;"
            )).unwrap();
        }
        let (version, hash) = base(&e, &f.item);
        let before = snapshot(&e, &f);
        assert!(checked(&e, &f.item, version, &hash).is_err());
        assert_eq!(snapshot(&e, &f), before, "synced={synced}");
        assert_eq!(
            e.file_bytes(f.item.clone(), None).unwrap(),
            Some(ORIGINAL.to_vec())
        );
        e.lock()
            .store
            .conn()
            .execute_batch("DROP TRIGGER fail_checked_file")
            .unwrap();
        let saved = checked(&e, &f.item, version, &hash).unwrap();
        assert_eq!(saved.version, version + 1);
        assert_eq!(
            e.file_bytes(f.item.clone(), None).unwrap(),
            Some(EDITED.to_vec())
        );
        assert_eq!(e.file_thumbnail(f.item.clone()).unwrap(), Some(vec![5; 48]));
        assert_eq!(caption(&e, &f), CAPTION);
        assert_eq!(e.lock().net.pending.len(), usize::from(synced));
        assert!(e.verify_log(f.space).valid);
    }
}

#[test]
fn checked_file_edit_and_original_history_survive_a_cold_database_reopen() {
    let directory = std::env::temp_dir().join(types::new_id("checked-file-reopen").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("core.sqlite").to_string_lossy().to_string();
    let (f, saved) = {
        let e = Client::open(path.clone(), "pt-BR".into()).unwrap();
        e.seed_demo_if_empty().unwrap();
        let f = fixture(&e);
        let (version, hash) = base(&e, &f.item);
        let saved = checked(&e, &f.item, version, &hash).unwrap();
        (f, saved)
    };
    {
        let reopened = Client::open(path, "pt-BR".into()).unwrap();
        assert_eq!(reopened.item(f.item.clone()).unwrap(), saved);
        assert_eq!(
            reopened.file_bytes(f.item.clone(), Some(1)).unwrap(),
            Some(ORIGINAL.to_vec())
        );
        assert_eq!(
            reopened.file_bytes(f.item.clone(), None).unwrap(),
            Some(EDITED.to_vec())
        );
        assert_eq!(
            reopened.file_thumbnail(f.item.clone()).unwrap(),
            Some(vec![5; 48])
        );
        assert_eq!(caption(&reopened, &f), CAPTION);
        assert!(!reopened
            .file_edit_allowed(f.item.clone(), 1, base(&reopened, &f.item).1)
            .unwrap());
        assert!(reopened.verify_all().iter().all(|log| log.valid));
    }
    std::fs::remove_dir_all(directory).unwrap();
}

fn checked_file_errors_use_session_language_and_restore_the_ambient_thread_language() {
    for (tag, ambient, stale, forbidden) in [
        (
            "en",
            crate::i18n::Lang::Pt,
            "the file changed; reopen the current version before saving",
            "unauthorized file edit",
        ),
        (
            "pt-BR",
            crate::i18n::Lang::En,
            "o arquivo mudou; reabra a versão atual antes de salvar",
            "edição de arquivo não autorizada",
        ),
    ] {
        let e = Client::open(":memory:".into(), tag.into()).unwrap();
        e.seed_demo_if_empty().unwrap();
        let f = fixture(&e);
        let (version, hash) = base(&e, &f.item);
        {
            let _ambient = crate::i18n::enter(ambient);
            assert!(!e
                .file_edit_allowed(f.item.clone(), version, "00".repeat(32))
                .unwrap());
            assert_eq!(crate::i18n::lang(), ambient);
            match checked(&e, &f.item, version, &"00".repeat(32)) {
                Err(CoreError::Stale { reason }) => assert_eq!(reason, stale),
                other => panic!("expected localized stale error, got {other:?}"),
            }
            assert_eq!(crate::i18n::lang(), ambient);
        }
        let me = e.me().unwrap().id;
        e.lock()
            .append(
                &f.space,
                &me,
                EventBody::MemberAdded {
                    identity: me.clone(),
                    role: Role::Reader,
                },
            )
            .unwrap();
        let _ambient = crate::i18n::enter(ambient);
        assert!(!e
            .file_edit_allowed(f.item.clone(), version, hash.clone())
            .unwrap());
        match checked(&e, &f.item, version, &hash) {
            Err(CoreError::Forbidden { reason }) => assert_eq!(reason, forbidden),
            other => panic!("expected localized forbidden error, got {other:?}"),
        }
        assert_eq!(crate::i18n::lang(), ambient);
    }
}

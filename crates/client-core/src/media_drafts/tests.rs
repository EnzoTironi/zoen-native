use super::*;
use crate::{PrivacyDto, SecretVault};
use std::sync::{Arc, Mutex};

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
    engine: Arc<Client>,
    vault: Arc<Vault>,
    scope: MediaDraftScope,
}
fn fixture(path: &str) -> Fixture {
    let engine = Client::open(path.into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    engine
        .create_account(
            "Media draft author".into(),
            "media_draft_author".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    let space = engine
        .create_group_with("Media draft QA".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let account = engine.account().unwrap();
    Fixture {
        engine,
        vault,
        scope: MediaDraftScope {
            expected_identity_id: account.identity_id,
            expected_device_id: account.device_id,
            space_id: space,
            thread_root_id: None,
        },
    }
}
fn input(id: &str) -> MediaDraftAssetInput {
    MediaDraftAssetInput {
        id: id.into(),
        name: format!("{id}.pdf"),
        mime: "application/pdf".into(),
        bytes: format!("Private media bytes {id}").into_bytes(),
    }
}
fn attachments(ids: &[&str], reply: Option<&str>) -> String {
    serde_json::json!({"kind":"attachments", "reply_to":reply,
        "items":ids.iter().map(|id| serde_json::json!({"asset_id":id,"caption":format!("Private caption {id}"),"mutation_id":format!("send-{id}")})).collect::<Vec<_>>()}).to_string()
}
fn save(
    f: &Fixture,
    revision: &str,
    json: &str,
    assets: Vec<MediaDraftAssetInput>,
) -> MediaDraftDto {
    let result = save_ack(f, revision, json, assets);
    assert_eq!(result.accepted_revision, result.draft.revision);
    result.draft
}
fn save_ack(
    f: &Fixture,
    revision: &str,
    json: &str,
    assets: Vec<MediaDraftAssetInput>,
) -> MediaDraftSaveResult {
    f.engine
        .save_media_draft(f.scope.clone(), revision.into(), json.into(), assets)
        .unwrap()
}
fn read(f: &Fixture) -> MediaDraftDto {
    f.engine.media_draft(f.scope.clone()).unwrap()
}
fn counts(e: &Client) -> Vec<i64> {
    let e = e.lock();
    let mut counts = ["events", "media", "media_keys", "blob_uploads", "outbox"]
        .map(|table| {
            e.store
                .conn()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        })
        .to_vec();
    counts.push(
        e.store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key NOT LIKE 'media.draft.%'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
    );
    counts
}
fn rows(f: &Fixture) -> Vec<(String, String)> {
    let e = f.engine.lock();
    let mut statement = e
        .store
        .conn()
        .prepare("SELECT key,value FROM meta WHERE key LIKE 'media.draft.%' ORDER BY key")
        .unwrap();
    statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}
fn temp() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(types::new_id("media-draft-test"));
    std::fs::create_dir_all(&path).unwrap();
    path
}
fn asset(f: &Fixture, draft: &MediaDraftDto, id: &str) -> Vec<u8> {
    f.engine
        .media_draft_asset(f.scope.clone(), draft.revision.clone(), id.into())
        .unwrap()
}

#[test]
fn private_selection_reopens_encrypted_without_a_message_file_or_upload() {
    let dir = temp();
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
}

#[test]
fn compare_and_swap_preserves_immutable_assets_and_prunes_only_removed_references() {
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

#[test]
fn second_file_failure_rolls_back_entire_batch_and_freezes_exact_retry() {
    let f = fixture(":memory:");
    let draft = save(
        &f,
        "0",
        &attachments(&["a", "b"], None),
        vec![input("a"), input("b")],
    );
    let before = counts(&f.engine);
    let timeline = f.engine.timeline(f.scope.space_id.clone()).unwrap();
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER fail_second BEFORE INSERT ON outbox WHEN NEW.json LIKE '%MessagePosted%' AND NEW.json LIKE '%Private caption b%' BEGIN SELECT RAISE(ABORT,'second failed'); END;").unwrap();
    assert!(f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert_eq!(
        f.engine.timeline(f.scope.space_id.clone()).unwrap(),
        timeline
    );
    let attempted = read(&f);
    assert!(attempted.attempted);
    assert_eq!(attempted.assets, draft.assets);
    assert!(matches!(
        f.engine.save_media_draft(
            f.scope.clone(),
            draft.revision.clone(),
            attachments(&["a"], None),
            vec![]
        ),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(asset(&f, &attempted, "b"), input("b").bytes);
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_second")
        .unwrap();
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    assert_eq!(sent.entries.len(), 2);
    assert_eq!(sent.draft.content_json, "null");
    for (entry, id) in sent.entries.iter().zip(["a", "b"]) {
        match &entry.kind {
            crate::EntryKind::Message {
                text,
                card: Some(card),
            } => {
                assert_eq!(text, &format!("Private caption {id}"));
                assert_eq!(
                    f.engine
                        .file_bytes(card.item_id.clone(), None)
                        .unwrap()
                        .unwrap(),
                    input(id).bytes
                );
            }
            other => panic!("expected file message, got {other:?}"),
        }
    }
    assert_eq!(
        rows(&f).len(),
        1,
        "accepted batch must release its private asset rows"
    );
    let accepted_counts = counts(&f.engine);
    assert_eq!(
        f.engine
            .send_media_draft(f.scope.clone(), draft.revision)
            .unwrap()
            .entries,
        sent.entries
    );
    assert_eq!(counts(&f.engine), accepted_counts);
}

#[test]
fn accepted_cold_retry_returns_same_entries_and_preserves_a_newer_unsent_selection() {
    let dir = temp();
    let path = dir.join("device.sqlite");
    let f = fixture(path.to_str().unwrap());
    let draft = save(
        &f,
        "0",
        &attachments(&["a", "b"], None),
        vec![input("a"), input("b")],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    let newer = save(
        &f,
        &sent.draft.revision,
        &attachments(&["new"], None),
        vec![input("new")],
    );
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let e = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(e.unlock(vault).unwrap());
    let before = counts(&e);
    let retried = e
        .send_media_draft(scope.clone(), draft.revision.clone())
        .unwrap();
    assert_eq!(retried.entries, sent.entries);
    assert_eq!(retried.draft, newer);
    let json: serde_json::Value = serde_json::from_str(
        &e.send_media_draft_json(scope.clone(), draft.revision.clone())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["entries"][0]["id"], sent.entries[0].id);
    assert_eq!(json["draft"]["revision"], newer.revision);
    assert_eq!(counts(&e), before);
    let saved = e
        .save_media_draft(
            scope.clone(),
            "0".into(),
            attachments(&["a", "b"], None),
            vec![input("a"), input("b")],
        )
        .unwrap();
    assert_eq!(saved.accepted_revision, draft.revision);
    assert_eq!(saved.draft, newer);
    assert_eq!(counts(&e), before);
    assert_eq!(
        e.media_draft_asset(scope, newer.revision, "new".into())
            .unwrap(),
        input("new").bytes
    );
    drop(e);
    std::fs::remove_dir_all(dir).unwrap();
}

fn voice(render: bool) -> String {
    serde_json::json!({"kind":"voice","reply_to":null,"source_asset_id":"original",
        "editor":{"trim":[0,12000],"cuts":[[1000,2000]],"transcript":"Private transcript"},
        "render":if render { serde_json::json!({"asset_id":"edited","mime":"audio/mp4","duration_ms":900.5,"levels":[0.1,0.8],"transcript":"spoken result"}) } else { serde_json::Value::Null }}).to_string()
}
fn audio(id: &str) -> MediaDraftAssetInput {
    let mut input = input(id);
    input.name = format!("{id}.m4a");
    input.mime = "audio/mp4".into();
    input
}

#[test]
fn voice_keeps_original_and_edit_recipe_private_then_sends_one_exact_render_once() {
    let f = fixture(":memory:");
    let before = counts(&f.engine);
    let original = save(&f, "0", &voice(false), vec![audio("original")]);
    assert!(matches!(
        f.engine
            .send_media_draft(f.scope.clone(), original.revision.clone()),
        Err(CoreError::DraftRejected { .. })
    ));
    assert!(!read(&f).attempted);
    assert_eq!(counts(&f.engine), before);
    let draft = save(&f, &original.revision, &voice(true), vec![audio("edited")]);
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    assert_eq!(sent.entries.len(), 1);
    let note = sent.entries[0].voice.as_ref().unwrap();
    assert_eq!(note.ms, 901);
    assert_eq!(note.transcript, "spoken result");
    assert_eq!(
        f.engine
            .file_bytes(note.item_id.clone(), None)
            .unwrap()
            .unwrap(),
        audio("edited").bytes
    );
    let accepted_counts = counts(&f.engine);
    assert_eq!(
        f.engine
            .send_media_draft(f.scope.clone(), draft.revision)
            .unwrap()
            .entries,
        sent.entries
    );
    assert_eq!(counts(&f.engine), accepted_counts);
    assert!(!serde_json::to_string(&f.engine.lock().history_bundle())
        .unwrap()
        .contains("Private transcript"));
}

#[test]
fn owner_membership_reply_and_thread_admission_precede_attempted_or_publication() {
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

#[test]
fn ciphertext_corruption_or_asset_substitution_never_publishes_or_overwrites_original_bytes() {
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

#[test]
fn malformed_content_duplicate_ids_and_invalid_voice_metadata_fail_without_private_or_public_writes(
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

#[test]
fn private_asset_and_record_storage_failures_rollback_and_can_retry_without_orphans() {
    let f = fixture(":memory:");
    for trigger in [
        "CREATE TRIGGER fail_private BEFORE INSERT ON meta WHEN NEW.key LIKE 'media.draft.record.%' BEGIN SELECT RAISE(ABORT,'record failed'); END;",
        "CREATE TRIGGER fail_private BEFORE INSERT ON meta WHEN NEW.key LIKE 'media.draft.asset.%' BEGIN SELECT RAISE(ABORT,'asset failed'); END;",
    ] {
        f.engine.lock().store.conn().execute_batch(trigger).unwrap();
        let before = counts(&f.engine);
        assert!(f.engine.save_media_draft(f.scope.clone(), "0".into(), attachments(&["a"], None), vec![input("a")]).is_err());
        assert!(rows(&f).is_empty()); assert_eq!(counts(&f.engine), before);
        f.engine.lock().store.conn().execute_batch("DROP TRIGGER fail_private").unwrap();
    }
    let draft = save(&f, "0", &attachments(&["a"], None), vec![input("a")]);
    assert_eq!(asset(&f, &draft, "a"), input("a").bytes);
}

#[test]
fn accepted_retry_precedes_changed_writer_reply_and_clear_retains_receipt() {
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
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    let clear = save(&f, &sent.draft.revision, "null", vec![]);
    f.engine
        .delete_message(f.scope.space_id.clone(), root.id)
        .unwrap();
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
    let before = counts(&f.engine);
    let retried = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    assert_eq!(
        retried
            .entries
            .iter()
            .map(|entry| &entry.id)
            .collect::<Vec<_>>(),
        sent.entries
            .iter()
            .map(|entry| &entry.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(retried.entries[0].kind, sent.entries[0].kind);
    assert_eq!(
        retried.entries[0].reply_to.as_ref().unwrap().id,
        sent.entries[0].reply_to.as_ref().unwrap().id
    );
    assert!(
        retried.entries[0]
            .reply_to
            .as_ref()
            .unwrap()
            .text
            .is_empty(),
        "retry returns the current canonical quote tombstone"
    );
    assert_eq!(retried.draft, clear);
    assert_eq!(counts(&f.engine), before);
}

#[test]
fn backup_restore_and_device_erase_never_carry_a_private_media_draft_to_another_account() {
    let f = fixture(":memory:");
    let draft = save(&f, "0", &attachments(&["a"], None), vec![input("a")]);
    // Confirm the legitimate group history so a restored account can see its normal membership.
    let pending = f.engine.lock().store.outbox().unwrap();
    let mut relay = event_log::SpaceLog::new(f.scope.space_id.clone());
    for pending in pending
        .into_iter()
        .filter(|p| p.event.space == f.scope.space_id)
    {
        let event = relay.sequence(pending.event);
        assert!(matches!(
            f.engine.lock().ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(event)
            }),
            crate::sync::Ingest::Confirmed
        ));
    }
    let private = rows(&f);
    let backup = f.engine.lock().backup_snapshot().unwrap();
    let dir = temp();
    let path = dir.join("backup.sqlite");
    std::fs::write(&path, backup).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM meta WHERE key LIKE 'media.draft.%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(rows(&f), private);
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
    let restored = Client::open(":memory:".into(), "en".into()).unwrap();
    restored
        .lock()
        .restore_snapshot(
            &header,
            &std::fs::read(&path).unwrap(),
            event_log::Signer::generate(),
        )
        .unwrap();
    assert_eq!(
        restored
            .lock()
            .store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key LIKE 'media.draft.%'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let mut new_scope = f.scope.clone();
    new_scope.expected_device_id = restored.account().unwrap().device_id;
    assert_eq!(restored.media_draft(new_scope).unwrap().revision, "0");
    assert_eq!(asset(&f, &draft, "a"), input("a").bytes);
    f.engine.erase_device(f.vault.clone()).unwrap();
    assert!(rows(&f).is_empty());
    assert!(f.engine.media_draft(f.scope.clone()).is_err());
    drop(restored);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_receipt_commit_rolls_back_canonical_send_and_retains_the_attempted_payload() {
    let f = fixture(":memory:");
    let draft = save(&f, "0", &attachments(&["a"], None), vec![input("a")]);
    let before = counts(&f.engine);
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER fail_receipt BEFORE INSERT ON meta WHEN NEW.key LIKE 'media.draft.record.%' AND EXISTS(SELECT 1 FROM outbox WHERE json LIKE '%MessagePosted%' AND json LIKE '%Private caption a%') BEGIN SELECT RAISE(ABORT,'receipt failed'); END;").unwrap();
    assert!(f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert!(read(&f).attempted);
    assert_eq!(asset(&f, &draft, "a"), input("a").bytes);
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_receipt")
        .unwrap();
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    assert_eq!(sent.entries.len(), 1);
    assert_eq!(sent.draft.content_json, "null");
}

#[test]
fn voice_append_failure_rolls_back_chunks_keys_and_uploads_then_retries_once() {
    let f = fixture(":memory:");
    let draft = save(
        &f,
        "0",
        &voice(true),
        vec![audio("original"), audio("edited")],
    );
    let before = counts(&f.engine);
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER fail_voice BEFORE INSERT ON outbox WHEN NEW.json LIKE '%ItemCreated%' AND NEW.json LIKE '%VoiceNotes/%' BEGIN SELECT RAISE(ABORT,'voice failed'); END;").unwrap();
    assert!(f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert!(read(&f).attempted);
    assert_eq!(asset(&f, &draft, "original"), audio("original").bytes);
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_voice")
        .unwrap();
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    assert_eq!(sent.entries.len(), 1);
    let accepted_counts = counts(&f.engine);
    let retried = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    assert_eq!(retried.entries, sent.entries);
    assert_eq!(retried.draft, sent.draft);
    assert_eq!(counts(&f.engine), accepted_counts);
}

#[test]
fn history_export_remains_only_canonical_events_while_media_draft_is_private() {
    let mut f = fixture(":memory:");
    f.scope.space_id = f
        .engine
        .create_group_with(
            "Actual encrypted history".into(),
            vec![],
            PrivacyDto::EndToEnd,
        )
        .unwrap();
    f.engine
        .send_message(f.scope.space_id.clone(), "Visible canonical history".into())
        .unwrap();
    // Use actual MLS sealing and canonical wire ordering. Plain Closed history is
    // deliberately absent from the linked-device opened-E2E history API.
    for _ in 0..12 {
        let mut engine = f.engine.lock();
        for (sha, _) in engine.pending_uploads(100) {
            engine.upload_done(&sha);
        }
        engine.mls_seal_outbox().unwrap();
        let envelopes = engine
            .outbox_envelopes()
            .into_iter()
            .filter(|env| env.space() == f.scope.space_id)
            .collect::<Vec<_>>();
        if envelopes.is_empty() {
            break;
        }
        for env in envelopes {
            let (seq, prev) = engine
                .logs
                .get(env.space())
                .map(|log| {
                    (
                        log.next_seq(),
                        log.events()
                            .last()
                            .map(|event| event.hash.clone())
                            .unwrap_or_else(|| types::GENESIS_PREV.into()),
                    )
                })
                .unwrap_or_else(|| (0, types::GENESIS_PREV.into()));
            let frame = protocol::Sequenced {
                seq,
                hash: event_log::chain_hash(env.space(), seq, &prev, &env.wire_hash()),
                prev,
                env,
            };
            assert_eq!(engine.ingest(frame), crate::sync::Ingest::Confirmed);
        }
    }
    assert!(
        !f.engine
            .lock()
            .store
            .outbox()
            .unwrap()
            .iter()
            .any(|pending| pending.event.space == f.scope.space_id),
        "encrypted history must fully confirm"
    );
    let before = serde_json::to_string(&f.engine.lock().history_bundle()).unwrap();
    assert!(
        before.contains("Visible canonical history"),
        "the history export must be nonempty"
    );
    save(&f, "0", &attachments(&["a"], None), vec![input("a")]);
    let after = serde_json::to_string(&f.engine.lock().history_bundle()).unwrap();
    assert_eq!(after, before);
    assert!(!after.contains("Private"));
}

#[test]
fn exact_lost_save_ack_replay_returns_newer_current_and_checks_supplied_bytes_metadata_and_order() {
    let f = fixture(":memory:");
    let original_json = attachments(&["a", "b"], None);
    let first = save(&f, "0", &original_json, vec![input("a"), input("b")]);
    let current = save(
        &f,
        &first.revision,
        &attachments(&["b", "c"], None),
        vec![input("c")],
    );
    let before = rows(&f);
    let counts_before = counts(&f.engine);
    // Whitespace/property ordering is not part of normalized content identity.
    let pretty = serde_json::to_string_pretty(
        &serde_json::from_str::<serde_json::Value>(&original_json).unwrap(),
    )
    .unwrap();
    let replay = f
        .engine
        .save_media_draft(
            f.scope.clone(),
            "0".into(),
            pretty,
            vec![input("a"), input("b")],
        )
        .unwrap();
    assert_eq!(replay.accepted_revision, first.revision);
    assert_eq!(replay.draft, current);
    assert_eq!(
        rows(&f),
        before,
        "replay cannot resurrect pruned a or rewrite newer c"
    );
    let mut changed_bytes = input("a");
    changed_bytes.bytes[0] ^= 1;
    let mut changed_name = input("a");
    changed_name.name = "renamed.pdf".into();
    let mut changed_mime = input("a");
    changed_mime.mime = "image/png".into();
    for inputs in [
        vec![changed_bytes, input("b")],
        vec![changed_name, input("b")],
        vec![changed_mime, input("b")],
        vec![input("b"), input("a")],
        vec![],
    ] {
        assert!(
            matches!(
                f.engine.save_media_draft(
                    f.scope.clone(),
                    "0".into(),
                    original_json.clone(),
                    inputs
                ),
                Err(CoreError::Stale { .. })
            ),
            "JSON equality alone must never acknowledge a different ingress selection"
        );
        assert_eq!(rows(&f), before);
    }
    assert_eq!(read(&f), current);
    assert_eq!(asset(&f, &current, "c"), input("c").bytes);
    assert_eq!(counts(&f.engine), counts_before);
}

#[test]
fn exact_clear_replay_preserves_newer_content_and_save_receipts_survive_send() {
    let f = fixture(":memory:");
    let first = save(&f, "0", &attachments(&["a"], None), vec![input("a")]);
    let clear = save(&f, &first.revision, "null", vec![]);
    let current = save(
        &f,
        &clear.revision,
        &attachments(&["b"], None),
        vec![input("b")],
    );
    let before = rows(&f);
    let replay = f
        .engine
        .save_media_draft(f.scope.clone(), first.revision, " null ".into(), vec![])
        .unwrap();
    assert_eq!(replay.accepted_revision, clear.revision);
    assert_eq!(replay.draft, current);
    assert_eq!(rows(&f), before);
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), current.revision.clone())
        .unwrap();
    let accepted_counts = counts(&f.engine);
    let replay = f
        .engine
        .save_media_draft(
            f.scope.clone(),
            clear.revision,
            attachments(&["b"], None),
            vec![input("b")],
        )
        .unwrap();
    assert_eq!(replay.accepted_revision, current.revision);
    assert_eq!(replay.draft, sent.draft);
    assert_eq!(counts(&f.engine), accepted_counts);
    assert!(read(&f).assets.is_empty());
}

#[test]
fn evicted_save_ack_is_stale_without_guessing_or_overwriting_current_payload() {
    let f = fixture(":memory:");
    let original = attachments(&["a"], None);
    let mut current = save(&f, "0", &original, vec![input("a")]);
    for index in 0..MAX_SAVE_RECEIPTS {
        let mut content: serde_json::Value = serde_json::from_str(&original).unwrap();
        content["items"][0]["caption"] = format!("Changed caption {index}").into();
        current = save(&f, &current.revision, &content.to_string(), vec![]);
    }
    let storage = f.engine.lock().media_draft_scope(&f.scope).unwrap();
    assert_eq!(
        f.engine
            .lock()
            .load_media_draft(&storage)
            .unwrap()
            .save_receipts
            .len(),
        MAX_SAVE_RECEIPTS
    );
    let before = rows(&f);
    assert!(matches!(
        f.engine
            .save_media_draft(f.scope.clone(), "0".into(), original, vec![input("a")]),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(rows(&f), before);
    assert_eq!(read(&f), current);
}

#[test]
fn voice_save_ack_survives_realistic_float_normalization_and_identifies_newer_current() {
    let f = fixture(":memory:");
    let before = counts(&f.engine);
    // Native waveform peaks are Float32 values promoted to Double before JSON encoding.
    let levels: Vec<f64> = (0..160)
        .map(|index| f64::from((index % 81) as f32 / 100.0))
        .collect();
    let waveform = serde_json::to_string(&levels).unwrap();
    let original_json = r#"{"kind":"voice","reply_to":null,"source_asset_id":"original","editor":{"version":1,"draft":{"original":{"id":"original","sampleRate":48000,"frameCount":48000,"levels":WAVEFORM}},"pendingTrim":null},"render":null}"#
        .replace("WAVEFORM", &waveform);
    let first = save_ack(&f, "0", &original_json, vec![audio("original")]);
    assert_eq!(first.accepted_revision, first.draft.revision);
    let original: serde_json::Value = serde_json::from_str(&first.draft.content_json).unwrap();
    assert_eq!(
        original["editor"]["draft"]["original"]["levels"]
            .as_array()
            .unwrap()
            .len(),
        160
    );
    assert_eq!(asset(&f, &first.draft, "original"), audio("original").bytes);

    // Private render metadata is intentionally normalized through the canonical f32 type.
    let rendered_json = original_json.replace(
        r#""render":null"#,
        r#""render":{"asset_id":"edited","mime":"audio/mp4","duration_ms":900.5,"levels":[0.4000000059604645],"transcript":"spoken result"}"#,
    );
    let rendered = save_ack(
        &f,
        &first.accepted_revision,
        &rendered_json,
        vec![audio("edited")],
    );
    assert_eq!(rendered.accepted_revision, rendered.draft.revision);
    assert!(rendered.draft.content_json.contains(r#""levels":[0.4]"#));
    assert!(!rendered
        .draft
        .content_json
        .contains(r#""levels":[0.4000000059604645]"#));
    assert_eq!(asset(&f, &rendered.draft, "edited"), audio("edited").bytes);
    assert_eq!(
        counts(&f.engine),
        before,
        "preparing audio must not publish media"
    );

    let newer = save(
        &f,
        &rendered.accepted_revision,
        &attachments(&["a"], None),
        vec![input("a")],
    );
    let retained = rows(&f);
    let replay = save_ack(
        &f,
        &first.accepted_revision,
        &rendered_json,
        vec![audio("edited")],
    );
    assert_eq!(replay.accepted_revision, rendered.accepted_revision);
    assert_ne!(replay.accepted_revision, replay.draft.revision);
    assert_eq!(replay.draft, newer);
    assert_eq!(
        rows(&f),
        retained,
        "old ACK must not resurrect normalized audio"
    );
}

#[test]
fn legacy_receipt_without_output_revision_preserves_cold_draft_but_cannot_guess_ack() {
    let dir = temp();
    let path = dir.join("device.sqlite");
    let f = fixture(path.to_str().unwrap());
    let json = attachments(&["a"], None);
    let draft = save(&f, "0", &json, vec![input("a")]);
    {
        let engine = f.engine.lock();
        let storage = engine.media_draft_scope(&f.scope).unwrap();
        let record = engine.load_media_draft(&storage).unwrap();
        let mut legacy = serde_json::to_value(record).unwrap();
        for receipt in legacy["save_receipts"].as_array_mut().unwrap() {
            receipt.as_object_mut().unwrap().remove("accepted_revision");
        }
        let plain = serde_json::to_vec(&legacy).unwrap();
        let encoded = crate::local_sealed::seal(&storage.key, &storage.aad, &plain).unwrap();
        engine.store.set_meta(&storage.id, &encoded).unwrap();
    }
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let engine = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(engine.unlock(vault.clone()).unwrap());
    let f = Fixture {
        engine,
        vault,
        scope,
    };
    assert_eq!(read(&f), draft);
    assert_eq!(asset(&f, &draft, "a"), input("a").bytes);
    let retained = rows(&f);
    assert!(matches!(
        f.engine
            .save_media_draft(f.scope.clone(), "0".into(), json.clone(), vec![input("a")]),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(
        rows(&f),
        retained,
        "unknown receipt output cannot rewrite retained media"
    );
    let next = save_ack(&f, &draft.revision, &json, vec![]);
    assert_eq!(next.accepted_revision, next.draft.revision);
    assert_eq!(asset(&f, &next.draft, "a"), input("a").bytes);
    drop(f);
    std::fs::remove_dir_all(dir).unwrap();
}

fn media_options(ids: &[&str], presentation: &str, grouping: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(&attachments(ids, None)).unwrap();
    value["options"] = serde_json::json!({"presentation":presentation,"grouping":grouping});
    value.to_string()
}

#[test]
fn media_contract_defaults_preserve_legacy_save_receipts_and_signed_bytes() {
    let f = fixture(":memory:");
    let old = attachments(&["a"], None);
    let draft = save(&f, "0", &old, vec![input("a")]);
    assert!(!draft.content_json.contains("options"));
    let replay = save_ack(
        &f,
        "0",
        &media_options(&["a"], "media", "separate"),
        vec![input("a")],
    );
    assert_eq!(replay.accepted_revision, draft.revision);
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    assert!(sent.entries[0].media.is_none());
    let pending = f.engine.lock().store.outbox().unwrap();
    let posted = pending
        .iter()
        .find(|pending| pending.event.client_id == sent.entries[0].id)
        .unwrap();
    assert!(!serde_json::to_string(&posted.event.body)
        .unwrap()
        .contains("media"));
    event_log::verify_author(&posted.event).unwrap();
    let before = counts(&f.engine);
    assert_eq!(
        f.engine
            .send_media_draft(f.scope.clone(), draft.revision)
            .unwrap()
            .entries,
        sent.entries
    );
    assert_eq!(counts(&f.engine), before);
}

#[test]
fn media_contract_file_album_preserves_real_mime_order_signature_and_retry() {
    let dir = temp();
    let path = dir.join("album.sqlite");
    let f = fixture(path.to_str().unwrap());
    let mut image = input("a");
    image.name = "photo.jpg".into();
    image.mime = "image/jpeg".into();
    let mut video = input("b");
    video.name = "video.mov".into();
    video.mime = "video/quicktime".into();
    let draft = save(
        &f,
        "0",
        &media_options(&["b", "a"], "file", "grouped"),
        vec![image, video],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    let first = sent.entries[0].media.as_ref().unwrap();
    assert_eq!(first.presentation, crate::AttachmentPresentationDto::File);
    let group_id = first.group.as_ref().unwrap().id.clone();
    for (index, (entry, expected_mime)) in sent
        .entries
        .iter()
        .zip(["video/quicktime", "image/jpeg"])
        .enumerate()
    {
        let media = entry.media.as_ref().unwrap();
        let group = media.group.as_ref().unwrap();
        assert_eq!(
            (&group.id, group.index, group.count),
            (&group_id, index as u32, 2)
        );
        let crate::EntryKind::Message {
            card: Some(card), ..
        } = &entry.kind
        else {
            panic!("missing file")
        };
        assert_eq!(
            f.engine
                .file_info_at(card.item_id.clone(), None)
                .unwrap()
                .mime,
            expected_mime
        );
    }
    let posted = f
        .engine
        .lock()
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .find(|pending| pending.event.client_id == sent.entries[0].id)
        .unwrap()
        .event;
    event_log::verify_author(&posted).unwrap();
    let mut tampered = posted.clone();
    let types::EventBody::MessagePosted {
        media: Some(media), ..
    } = &mut tampered.body
    else {
        panic!("missing signed media")
    };
    media.presentation = types::AttachmentPresentation::Media;
    assert!(event_log::verify_author(&tampered).is_err());
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let restored = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
    assert!(restored.unlock(vault).unwrap());
    let before = counts(&restored);
    let replay = restored.send_media_draft(scope, draft.revision).unwrap();
    assert_eq!(replay.entries, sent.entries);
    assert_eq!(counts(&restored), before);
    drop(restored);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn media_contract_group_failure_rolls_back_and_retains_frozen_options() {
    let f = fixture(":memory:");
    let json = media_options(&["a", "b"], "media", "grouped");
    let draft = save(&f, "0", &json, vec![input("a"), input("b")]);
    let before = counts(&f.engine);
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER fail_album BEFORE INSERT ON outbox WHEN NEW.json LIKE '%MessagePosted%' AND NEW.json LIKE '%Private caption b%' BEGIN SELECT RAISE(ABORT,'fail album'); END;").unwrap();
    assert!(f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert!(read(&f).attempted);
    assert_eq!(read(&f).content_json, draft.content_json);
    assert!(f
        .engine
        .save_media_draft(
            f.scope.clone(),
            draft.revision.clone(),
            media_options(&["a", "b"], "file", "separate"),
            vec![]
        )
        .is_err());
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_album")
        .unwrap();
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    assert_eq!(sent.entries.len(), 2);
    assert_eq!(
        sent.entries[0]
            .media
            .as_ref()
            .unwrap()
            .group
            .as_ref()
            .unwrap()
            .index,
        0
    );
    assert_eq!(
        sent.entries[1]
            .media
            .as_ref()
            .unwrap()
            .group
            .as_ref()
            .unwrap()
            .index,
        1
    );
    let after = counts(&f.engine);
    assert_eq!(
        f.engine
            .send_media_draft(f.scope.clone(), draft.revision)
            .unwrap()
            .entries,
        sent.entries
    );
    assert_eq!(counts(&f.engine), after);
}

#[test]
fn media_contract_rejects_invalid_options_and_foreign_authority_without_mutation() {
    let f = fixture(":memory:");
    let before = counts(&f.engine);
    for options in [
        serde_json::json!({"presentation":"file","grouping":"grouped"}),
        serde_json::json!({"presentation":"media","grouping":"guessed"}),
        serde_json::json!({"presentation":"fake_mime","grouping":"separate"}),
        serde_json::json!({"presentation":"media","grouping":"separate","quality":"original"}),
        serde_json::json!({"grouping":"separate"}),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&attachments(&["a"], None)).unwrap();
        value["options"] = options;
        assert!(f
            .engine
            .save_media_draft(
                f.scope.clone(),
                "0".into(),
                value.to_string(),
                vec![input("a")]
            )
            .is_err());
        assert_eq!(counts(&f.engine), before);
    }
    let draft = save(
        &f,
        "0",
        &media_options(&["a", "b"], "file", "grouped"),
        vec![input("a"), input("b")],
    );
    let mut foreign = f.scope.clone();
    foreign.expected_identity_id = event_log::Signer::generate().id();
    let before = counts(&f.engine);
    assert!(f.engine.send_media_draft(foreign, draft.revision).is_err());
    assert_eq!(counts(&f.engine), before);
}

#[test]
fn media_contract_rejects_invalid_group_index_and_duplicate_membership_atomically() {
    let f = fixture(":memory:");
    let options = types::AttachmentSendOptions {
        presentation: types::AttachmentPresentation::File,
        grouping: types::AttachmentGrouping::Grouped,
    };
    let before = counts(&f.engine);
    let invalid = options.metadata(
        &f.scope.expected_identity_id,
        &f.scope.space_id,
        None,
        false,
        &["one", "two"],
        1,
    );
    assert!(f
        .engine
        .lock()
        .send_file_message_with_media(
            &f.scope.space_id,
            "a.jpg",
            "image/jpeg",
            &[1, 2, 3],
            None,
            None,
            false,
            "one",
            "",
            invalid
        )
        .is_err());
    assert_eq!(counts(&f.engine), before);
    let first = options.metadata(
        &f.scope.expected_identity_id,
        &f.scope.space_id,
        None,
        false,
        &["one", "two"],
        0,
    );
    f.engine
        .lock()
        .send_file_message_with_media(
            &f.scope.space_id,
            "a.jpg",
            "image/jpeg",
            &[1, 2, 3],
            None,
            None,
            false,
            "one",
            "",
            first.clone(),
        )
        .unwrap();
    let before = counts(&f.engine);
    assert!(f
        .engine
        .lock()
        .send_file_message_with_media(
            &f.scope.space_id,
            "b.jpg",
            "image/jpeg",
            &[4, 5, 6],
            None,
            None,
            false,
            "other",
            "",
            first
        )
        .is_err());
    assert_eq!(counts(&f.engine), before);
}

#[test]
fn media_contract_forwarded_group_is_rejected_and_single_forward_preserves_file_presentation() {
    let f = fixture(":memory:");
    let draft = save(
        &f,
        "0",
        &media_options(&["a", "b"], "file", "grouped"),
        vec![input("a"), input("b")],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    let forwarded = f
        .engine
        .lock()
        .forward(&f.scope.space_id, &sent.entries[0].id, &f.scope.space_id)
        .unwrap();
    assert_eq!(
        forwarded.media.as_ref().unwrap().presentation,
        crate::AttachmentPresentationDto::File
    );
    assert!(forwarded.media.as_ref().unwrap().group.is_none());
    assert!(forwarded.forwarded);
    let item = f
        .engine
        .lock()
        .file_add(
            &f.scope.space_id,
            "new",
            "new.pdf",
            "application/pdf",
            &[7, 8, 9],
            None,
        )
        .unwrap();
    let metadata = types::AttachmentSendOptions {
        presentation: types::AttachmentPresentation::File,
        grouping: types::AttachmentGrouping::Grouped,
    }
    .metadata(
        &f.scope.expected_identity_id,
        &f.scope.space_id,
        None,
        false,
        &["new-1", "new-2"],
        0,
    );
    let before = counts(&f.engine);
    assert!(f
        .engine
        .lock()
        .append(
            &f.scope.space_id,
            &f.scope.expected_identity_id,
            types::EventBody::MessagePosted {
                message: types::new_id("msg"),
                text: String::new(),
                attaches: Some(item),
                media: metadata,
                reply: None,
                forwarded: true,
            }
        )
        .is_err());
    assert_eq!(counts(&f.engine), before);
}

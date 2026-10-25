//! recovery cases scenarios; existing shared fixture remains in the parent.
use super::*;

#[test]
pub(super) fn encrypted_draft_reopens_exactly_and_main_threads_are_independent() {
    let dir = temp().unwrap();
    let path = dir.join("account.sqlite");
    let f = fixture(path.to_str().unwrap());
    let root1 = f
        .engine
        .send_message(f.scope.space_id.clone(), "Root one".into())
        .unwrap();
    let root2 = f
        .engine
        .send_message(f.scope.space_id.clone(), "Root two".into())
        .unwrap();
    let text = "  Private 雨 👩🏽‍💻\r\nCafe\u{301}\0exact bytes  ";
    let main = save(
        &f,
        MESSAGE_DRAFT_REVISION_ZERO,
        text,
        Some(root1.id.clone()),
    );
    let mut thread1 = f.scope.clone();
    thread1.thread_root_id = Some(root1.id.clone());
    let mut thread2 = f.scope.clone();
    thread2.thread_root_id = Some(root2.id);
    let a = f
        .engine
        .lock()
        .save_message_draft(&thread1, "0", "Thread A".into(), None)
        .unwrap();
    let b = f
        .engine
        .lock()
        .save_message_draft(&thread2, "0", "Thread B".into(), None)
        .unwrap();
    let (_, cipher) = row(&f);
    assert!(!cipher.contains("Private"));
    assert!(cipher.starts_with("v1:"));
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let e = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(
        e.lock().message_draft(&scope).is_err(),
        "locked account cannot read draft"
    );
    assert!(e.unlock(vault).unwrap());
    assert_eq!(e.lock().message_draft(&scope).unwrap(), main);
    assert_eq!(e.lock().message_draft(&thread1).unwrap(), a);
    assert_eq!(e.lock().message_draft(&thread2).unwrap(), b);
    drop(e);
    for entry in std::fs::read_dir(&dir).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        assert!(
            !bytes.windows(b"Private".len()).any(|p| p == b"Private"),
            "SQLite/WAL contains plaintext draft"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
pub(super) fn send_and_consume_are_atomic_idempotent_and_preserve_newer_text_after_cold_reopen() {
    let dir = temp().unwrap();
    let path = dir.join("account.sqlite");
    let f = fixture(path.to_str().unwrap());
    let root = f
        .engine
        .send_message(f.scope.space_id.clone(), "Root".into())
        .unwrap();
    let saved = save(&f, "0", "First reply", Some(root.id.clone()));
    let before = counts(&f.engine);
    let accepted = f
        .engine
        .lock()
        .send_message_draft(&f.scope, &saved.revision)
        .unwrap();
    assert!(accepted.draft.text.is_empty());
    assert_eq!(counts(&f.engine), (before.0, before.1 + 1));
    assert_eq!(accepted.entry.reply_to.as_ref().unwrap().id, root.id);
    let new = save(&f, &accepted.draft.revision, "New unsubmitted text", None);
    let again = f
        .engine
        .lock()
        .send_message_draft(&f.scope, &saved.revision)
        .unwrap();
    assert_eq!(again.entry.id, accepted.entry.id);
    assert_eq!(again.draft, new);
    assert_eq!(counts(&f.engine), (before.0, before.1 + 1));
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let e = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    e.unlock(vault).unwrap();
    assert_eq!(e.lock().message_draft(&scope).unwrap(), new);
    assert_eq!(
        e.lock()
            .send_message_draft(&scope, &saved.revision)
            .unwrap()
            .entry
            .id,
        accepted.entry.id
    );
    assert_eq!(
        e.timeline(scope.space_id.clone())
            .unwrap()
            .iter()
            .filter(|m| m.id == accepted.entry.id)
            .count(),
        1
    );
    drop(e);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
pub(super) fn storage_failure_rolls_back_send_and_keeps_exact_previous_draft_for_retry() {
    for target in ["journal", "outbox"] {
        let f = fixture(":memory:");
        let saved = save(&f, "0", "Atomic send", None);
        let before = counts(&f.engine);
        let before_row = row(&f);
        let sql = if target == "journal" {
            "CREATE TRIGGER fail_draft BEFORE UPDATE ON meta WHEN NEW.key LIKE 'message.draft.%' BEGIN SELECT RAISE(ABORT,'draft failed'); END;"
        } else {
            "CREATE TRIGGER fail_draft BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT,'outbox failed'); END;"
        };
        f.engine.lock().store.conn().execute_batch(sql).unwrap();
        assert!(f
            .engine
            .lock()
            .send_message_draft(&f.scope, &saved.revision)
            .is_err());
        assert_eq!(counts(&f.engine), before);
        assert_eq!(row(&f), before_row);
        assert_eq!(read(&f), saved);
        f.engine
            .lock()
            .store
            .conn()
            .execute_batch("DROP TRIGGER fail_draft")
            .unwrap();
        let accepted = f
            .engine
            .lock()
            .send_message_draft(&f.scope, &saved.revision)
            .unwrap();
        assert!(accepted.draft.text.is_empty());
        assert_eq!(
            f.engine
                .lock()
                .send_message_draft(&f.scope, &saved.revision)
                .unwrap()
                .entry
                .id,
            accepted.entry.id
        );
    }
}

#[test]
pub(super) fn unsent_text_never_enters_signed_peer_history_and_atomic_send_replays_once(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::sync::Ingest;
    use protocol::{Envelope, Sequenced};
    let f = fixture(":memory:");
    let peer = fixture(":memory:");
    let me = f.scope.expected_identity_id.clone();
    f.engine
        .lock()
        .append(
            &f.scope.space_id,
            &me,
            types::EventBody::MemberAdded {
                identity: peer.scope.expected_identity_id.clone(),
                role: types::Role::Member,
            },
        )
        .unwrap();
    let before = counts(&f.engine);
    let saved = save(&f, "0", "Private before deliberate send", None);
    assert_eq!(counts(&f.engine), before);
    assert!(!serde_json::to_string(
        &f.engine
            .lock()
            .store
            .outbox()
            .unwrap()
            .iter()
            .map(|p| &p.event)
            .collect::<Vec<_>>()
    )
    .unwrap()
    .contains(&saved.text));
    let result = f
        .engine
        .lock()
        .send_message_draft(&f.scope, &saved.revision)
        .unwrap();
    let again = f
        .engine
        .lock()
        .send_message_draft(&f.scope, &saved.revision)
        .unwrap();
    assert_eq!(result.entry.id, again.entry.id);
    let mut relay = event_log::SpaceLog::new(f.scope.space_id.clone());
    let pending = f.engine.lock().store.outbox().unwrap();
    for pending in pending
        .into_iter()
        .filter(|pending| pending.event.space == f.scope.space_id)
    {
        let event = relay.sequence(pending.event)?;
        assert_eq!(
            peer.engine.lock().ingest(Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: Envelope::plain(event)?
            }),
            Ingest::Applied
        );
    }
    let timeline = peer.engine.timeline(f.scope.space_id.clone()).unwrap();
    let entries: Vec<_> = timeline
        .iter()
        .filter(|e| e.id == result.entry.id)
        .collect();
    assert_eq!(entries.len(), 1);
    assert!(
        matches!(&entries[0].kind,crate::EntryKind::Message{text,card:None} if text==&saved.text)
    );
    assert_eq!(entries[0].author.id, me);
    assert!(!entries[0].author.is_me);
    assert!(peer.engine.verify_log(f.scope.space_id.clone()).valid);
    peer.engine.lock().reload().unwrap();
    assert_eq!(
        peer.engine
            .timeline(f.scope.space_id)
            .unwrap()
            .iter()
            .filter(|e| e.id == result.entry.id)
            .count(),
        1
    );
    Ok(())
}

#[test]
pub(super) fn backup_and_new_device_restore_exclude_device_local_drafts(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    save(&f, "0", "Never portable", None);
    let old_row = row(&f);
    // Backups intentionally omit pending outbox data; confirm this group's
    // legitimate history before requiring membership on the restored device.
    let pending = f.engine.lock().store.outbox().unwrap();
    let mut relay = event_log::SpaceLog::new(f.scope.space_id.clone());
    for pending in pending
        .into_iter()
        .filter(|p| p.event.space == f.scope.space_id)
    {
        let event = relay.sequence(pending.event)?;
        assert!(matches!(
            f.engine.lock().ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(event)?,
            }),
            crate::sync::Ingest::Confirmed
        ));
    }
    let bytes = f.engine.lock().backup_snapshot().unwrap();
    let dir = temp().unwrap();
    let path = dir.join("backup.sqlite");
    std::fs::write(&path, &bytes).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM meta WHERE key LIKE 'message.draft.%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(row(&f), old_row);
    // Also refuse device-local rows in an otherwise valid retained snapshot.
    conn.execute(
        "INSERT INTO meta(key,value) VALUES (?1,?2)",
        rusqlite::params![old_row.0, old_row.1],
    )
    .unwrap();
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
            event_log::Signer::generate().unwrap(),
        )
        .unwrap();
    let count: i64 = restored
        .lock()
        .store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM meta WHERE key LIKE 'message.draft.%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    let mut scope = f.scope.clone();
    scope.expected_device_id = restored.account().unwrap().device_id;
    assert_eq!(restored.lock().message_draft(&scope).unwrap().revision, "0");
    assert!(restored.lock().message_draft(&f.scope).is_err());
    drop(restored);
    std::fs::remove_dir_all(dir).unwrap();
    Ok(())
}

#[test]
pub(super) fn accepted_send_cold_restart_never_recovers_sent_text_or_sends_a_duplicate() {
    let dir = temp().unwrap();
    let path = dir.join("account.sqlite");
    let f = fixture(path.to_str().unwrap());
    let draft = save(&f, "0", "Sent once, never a ghost draft", None);
    let accepted = f
        .engine
        .lock()
        .send_message_draft(&f.scope, &draft.revision)
        .unwrap();
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let e = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    e.unlock(vault).unwrap();
    let restored = e.lock().message_draft(&scope).unwrap();
    assert!(
        restored.text.is_empty(),
        "cold open must not resurrect text already accepted into the outbox"
    );
    let before = counts(&e);
    let retry = e
        .lock()
        .send_message_draft(&scope, &draft.revision)
        .unwrap();
    assert_eq!(retry.entry.id, accepted.entry.id);
    assert_eq!(counts(&e), before);
    drop(e);
    std::fs::remove_dir_all(dir).unwrap();
}

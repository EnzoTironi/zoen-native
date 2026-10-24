//! Whole private custody cases on the same original parent fixture.

use super::*;

pub(super) fn complete_captures_are_private_encrypted_and_survive_a_locked_cold_reopen(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = temp()?;
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
    let engine = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
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

    Ok(())
}

pub(super) fn backup_restore_and_device_erase_exclude_private_pdf_journals_even_from_retained_snapshots(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    let a = stage(&f, "0", "editor", "a");
    // Confirm real membership/file history, so the restored device can access the same PDF.
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
                env: protocol::Envelope::plain(event)
            }),
            crate::sync::Ingest::Confirmed
        ));
    }
    let private = rows(&f);
    let snapshot = f.engine.lock().backup_snapshot().unwrap();
    let dir = temp()?;
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
    let restored = Client::open(":memory:".into(), "en".into()).unwrap();
    restored
        .lock()
        .restore_snapshot(
            &header,
            &std::fs::read(&path).unwrap(),
            event_log::Signer::generate()?,
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

    Ok(())
}

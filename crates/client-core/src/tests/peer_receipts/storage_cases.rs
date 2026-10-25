//! Complete storage scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn sql_insert_and_update_failures_leave_disk_and_published_ticks_unchanged_then_retry(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for operation in ["INSERT", "UPDATE"] {
        let (_db, e, c) = fixture()?;
        let expected = if operation == "UPDATE" {
            assert!(e.lock().receipt(&c.space, &c.peer, c.voice.seq, 0).unwrap());
            Delivery::Delivered
        } else {
            Delivery::Sent
        };
        let before = snapshot(&e);
        e.lock().store.conn().execute_batch(&format!(
            "CREATE TEMP TRIGGER refuse_receipt BEFORE {operation} ON peer_receipts BEGIN SELECT RAISE(ABORT, 'synthetic receipt failure'); END;",
        )).unwrap();
        assert!(
            matches!(
                e.lock()
                    .receipt(&c.space, &c.peer, c.voice.seq + 10, c.voice.seq + 5),
                Err(CoreError::Storage { .. })
            ),
            "{operation}"
        );
        assert_eq!(snapshot(&e), before, "{operation}");
        assert_delivery(&e, &c, expected);
        e.lock()
            .store
            .conn()
            .execute_batch("DROP TRIGGER refuse_receipt")
            .unwrap();
        assert!(e
            .lock()
            .receipt(&c.space, &c.peer, c.voice.seq + 10, c.voice.seq + 5)
            .unwrap());
        assert_delivery(&e, &c, Delivery::Read);
        assert_eq!(
            snapshot(&e).rows,
            vec![(
                c.space.clone(),
                c.peer.clone(),
                format!("{:020}", c.voice.seq + 10),
                format!("{:020}", c.voice.seq + 5)
            )]
        );
    }
    Ok(())
}

pub(super) fn reload_preserves_watermarks_and_wipe_or_erase_device_clears_only_its_database(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for through_public_api in [false, true] {
        let (db, e, c) = fixture()?;
        let (_other_db, other, other_c) = fixture()?;
        assert!(e.lock().receipt(&c.space, &c.peer, 0, c.voice.seq).unwrap());
        assert!(other
            .lock()
            .receipt(&other_c.space, &other_c.peer, other_c.voice.seq, 0)
            .unwrap());
        {
            let engine = e.lock();
            assert_eq!(
                engine
                    .store
                    .raise_peer_receipt(&c.space, &c.peer, u64::MAX, u64::MAX - 1)
                    .unwrap(),
                (u64::MAX, u64::MAX - 1)
            );
            assert_eq!(
                engine.net.receipts[&c.space][&c.peer],
                (c.voice.seq, c.voice.seq)
            );
        }
        e.lock().reload().unwrap();
        assert_eq!(
            e.lock().net.receipts[&c.space][&c.peer],
            (u64::MAX, u64::MAX - 1)
        );
        let before = snapshot(&e);
        let other_before = snapshot(&other);
        for _ in 0..2 {
            e.lock().reload().unwrap();
            assert_eq!(snapshot(&e), before);
            assert_delivery(&e, &c, Delivery::Read);
            assert!(!e.lock().receipt(&c.space, &c.peer, 1, 1).unwrap());
        }
        if through_public_api {
            e.erase_device(Arc::new(MemVault::default())).unwrap();
        } else {
            e.lock().wipe().unwrap();
        }
        assert!(snapshot(&e).rows.is_empty());
        assert!(snapshot(&e).memory.is_empty());
        assert_eq!(snapshot(&other), other_before);
        drop(e);
        let e = db.open();
        assert!(snapshot(&e).rows.is_empty());
        assert!(snapshot(&e).memory.is_empty());
        assert!(e.spaces().is_empty());
        assert_delivery(&other, &other_c, Delivery::Delivered);
    }
    Ok(())
}

pub(super) fn device_peer_receipts_are_excluded_from_backup_without_changing_the_live_store(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (db, e, c) = fixture()?;
    assert!(e.lock().receipt(&c.space, &c.peer, 0, c.voice.seq).unwrap());
    let before = snapshot(&e);
    let bytes = e.lock().backup_snapshot().unwrap();
    let path = db.directory.join("backup.sqlite");
    std::fs::write(&path, bytes).unwrap();
    let backup = rusqlite::Connection::open(path).unwrap();
    let table_count: i64 = backup
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'peer_receipts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        table_count, 0,
        "peer acknowledgements stay local to this device"
    );
    let events: i64 = backup
        .query_row(
            "SELECT COUNT(*) FROM events WHERE space = ?1",
            [&c.space],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        events > 0,
        "canonical conversation history is still backed up"
    );
    assert_eq!(snapshot(&e), before);
    assert_delivery(&e, &c, Delivery::Read);

    Ok(())
}

pub(super) fn legacy_sample_receipts_migrate_only_canonical_peers_and_failure_keeps_the_blob(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (db, e, c) = fixture()?;
    assert!(e.lock().receipt(&c.space, &c.peer, c.voice.seq, 0).unwrap());
    let legacy = BTreeMap::from([
        (
            c.space.clone(),
            BTreeMap::from([
                (c.peer.clone(), (c.voice.seq, c.voice.seq)),
                (c.outsider.clone(), (u64::MAX, u64::MAX)),
                (c.me.clone(), (u64::MAX, u64::MAX)),
            ]),
        ),
        (
            "missing-space".into(),
            BTreeMap::from([(c.peer.clone(), (u64::MAX, u64::MAX))]),
        ),
    ]);
    {
        let mut engine = e.lock();
        engine
            .store
            .conn()
            .execute_batch("DELETE FROM peer_receipts")
            .unwrap();
        engine.net.receipts.clear();
        engine
            .store
            .set_meta("seed_receipts", &serde_json::to_string(&legacy).unwrap())
            .unwrap();
        // Fail after the receipt inserts, proving the whole migration rolls back.
        engine.store.conn().execute_batch(
            "CREATE TEMP TRIGGER refuse_legacy_cleanup BEFORE DELETE ON meta WHEN OLD.key = 'seed_receipts' BEGIN SELECT RAISE(ABORT, 'synthetic migration failure'); END;",
        ).unwrap();
    }
    let before = snapshot(&e);
    assert!(matches!(e.lock().reload(), Err(CoreError::Storage { .. })));
    assert_eq!(snapshot(&e), before);
    assert_delivery(&e, &c, Delivery::Sent);
    e.lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER refuse_legacy_cleanup")
        .unwrap();
    e.lock().reload().unwrap();
    assert_eq!(
        snapshot(&e).rows,
        vec![(
            c.space.clone(),
            c.peer.clone(),
            format!("{:020}", c.voice.seq),
            format!("{:020}", c.voice.seq)
        )]
    );
    assert!(e.lock().store.meta("seed_receipts").unwrap().is_none());
    assert!(
        e.lock().read_up_to(&c.space).is_none(),
        "the legacy own-identity spoof is not a read here"
    );
    assert_delivery(&e, &c, Delivery::Read);
    drop(e);
    let e = db.open();
    assert_eq!(snapshot(&e).rows.len(), 1);
    assert_delivery(&e, &c, Delivery::Read);

    Ok(())
}

pub(super) fn own_identity_read_storage_failure_propagates_through_receive_and_retry_is_durable(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (db, e, c) = fixture()?;
    let mut deferred = crate::net::DeferredReceipts::default();
    let before = snapshot(&e);
    e.lock().store.conn().execute_batch(
        "CREATE TEMP TRIGGER refuse_own_read BEFORE INSERT ON meta WHEN NEW.key LIKE 'read:%' BEGIN SELECT RAISE(ABORT, 'synthetic own read failure'); END;",
    ).unwrap();
    assert!(matches!(
        deferred.receive(&mut e.lock(), &c.space, &c.me, 0, c.voice.seq),
        Err(CoreError::Storage { .. })
    ));
    assert_eq!(snapshot(&e), before);
    assert!(e.lock().read_up_to(&c.space).is_none());
    e.lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER refuse_own_read")
        .unwrap();
    assert!(deferred
        .receive(&mut e.lock(), &c.space, &c.me, 0, c.voice.seq)
        .unwrap());
    assert!(!deferred
        .receive(&mut e.lock(), &c.space, &c.me, 0, c.voice.seq - 1)
        .unwrap());
    assert_eq!(e.lock().read_up_to(&c.space), Some(c.voice.seq));
    assert!(
        snapshot(&e).rows.is_empty(),
        "my other device never becomes a peer recipient"
    );
    assert_delivery(&e, &c, Delivery::Sent);
    drop(e);
    let e = db.open();
    assert_eq!(e.lock().read_up_to(&c.space), Some(c.voice.seq));
    assert!(snapshot(&e).rows.is_empty());

    Ok(())
}

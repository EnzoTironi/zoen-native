//! SQLite persistence, schema upgrade and maintenance boundaries.
use super::*;

fn event(id: &str, at_ms: i64, kind: &str) -> Event {
    Event {
        space: "s".into(),
        seq: 0,
        prev: String::new(),
        author: "fixture".into(),
        at_ms,
        body: types::EventBody::Sealed { kind: kind.into() },
        hash: String::new(),
        sig: "retained-signature".into(),
        client_id: id.into(),
        device: None,
        cert: None,
        seen: None,
        content: vec![1, 2, 3],
        sealed_wire: None,
    }
}

fn ids(store: &Store) -> Vec<String> {
    store
        .outbox()
        .unwrap()
        .into_iter()
        .map(|p| p.event.client_id)
        .collect()
}

fn legacy(store: &Store) {
    store
        .conn
        .execute_batch(
            "DROP TABLE outbox;
        CREATE TABLE outbox (client_id TEXT PRIMARY KEY, space TEXT NOT NULL,
        json TEXT NOT NULL, created_ms INTEGER NOT NULL, attempts INTEGER NOT NULL DEFAULT 0,
        last_error TEXT, failed INTEGER NOT NULL DEFAULT 0);
        CREATE INDEX outbox_by_space ON outbox(space, created_ms);",
        )
        .unwrap();
}

fn insert_legacy(store: &Store, event: &Event, rowid: i64) {
    store
        .conn
        .execute(
            "INSERT INTO outbox(rowid,client_id,space,json,created_ms,attempts,last_error,failed)
        VALUES (?1,?2,?3,?4,?5,2,'retained error',1)",
            params![
                rowid,
                event.client_id,
                event.space,
                serde_json::to_string(event).unwrap(),
                event.at_ms
            ],
        )
        .unwrap();
}

#[test]
fn enqueue_order_survives_clock_rollback_retry_duplicate_and_reinsert(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::in_memory().unwrap();
    let events = [
        event("commit", 300, "Commit"),
        event("welcome", 100, "Welcome"),
        event("later", 100, "Application"),
    ];
    for e in &events {
        store.outbox_put(e).unwrap();
    }
    store.outbox_note("commit", "retry", false).unwrap();
    let mut changed = events[0].clone();
    changed.content = vec![9];
    store.outbox_put(&changed).unwrap();
    assert_eq!(ids(&store), ["commit", "welcome", "later"]);
    assert_eq!(store.outbox_get("commit").unwrap(), Some(events[0].clone()));
    assert_eq!(
        store
            .outbox_heads()
            .unwrap()
            .iter()
            .map(|p| p.client_id.as_str())
            .collect::<Vec<_>>(),
        ["commit", "welcome", "later"]
    );
    assert_eq!(
        store
            .outbox_handshakes()
            .unwrap()
            .iter()
            .map(|p| p.0.as_str())
            .collect::<Vec<_>>(),
        ["commit", "welcome"]
    );
    store.outbox_remove("commit").unwrap();
    store.outbox_put(&events[0]).unwrap();
    assert_eq!(ids(&store), ["welcome", "later", "commit"]);
    assert_eq!(store.outbox_get("commit").unwrap(), Some(events[0].clone()));
    Ok(())
}

#[test]
fn migration_preserves_bytes_retry_state_and_order_through_vacuum_and_image(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::in_memory().unwrap();
    legacy(&store);
    let events = [
        event("first", 300, "Commit"),
        event("second", 100, "Welcome"),
    ];
    insert_legacy(&store, &events[0], 4);
    insert_legacy(&store, &events[1], 900);
    store.upgrade_outbox_order().unwrap();
    let before = store.outbox().unwrap();
    assert_eq!(ids(&store), ["first", "second"]);
    store.conn.execute_batch("VACUUM").unwrap();
    let cold = Store::from_memory_image(&store.memory_image().unwrap()).unwrap();
    assert_eq!(ids(&cold), ["first", "second"]);
    for (expected, actual) in before.iter().zip(cold.outbox().unwrap()) {
        assert_eq!(actual.event, expected.event);
        assert_eq!(actual.attempts, 2);
        assert_eq!(actual.last_error.as_deref(), Some("retained error"));
        assert!(actual.failed);
    }
    let orders = cold
        .conn
        .prepare("SELECT queue_order FROM outbox ORDER BY queue_order")
        .unwrap()
        .query_map([], |r| r.get::<_, i64>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(orders, [4, 900]);
    Ok(())
}

#[test]
fn failed_migration_leaves_legacy_schema_and_original_rows_unchanged(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::in_memory().unwrap();
    legacy(&store);
    let e = event("original", 3, "Commit");
    insert_legacy(&store, &e, 9);
    store
        .conn
        .execute_batch(
            "CREATE TEMP TRIGGER reject_order BEFORE UPDATE ON outbox
        BEGIN SELECT RAISE(ABORT,'injected order migration failure'); END;",
        )
        .unwrap();
    assert!(store.upgrade_outbox_order().is_err());
    let columns: i64 = store
        .conn
        .query_row(
            "SELECT count(*) FROM pragma_table_info('outbox') WHERE name='queue_order'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(columns, 0);
    assert_eq!(store.outbox_get("original").unwrap(), Some(e));
    store
        .conn
        .execute_batch("DROP TRIGGER reject_order")
        .unwrap();
    store.upgrade_outbox_order().unwrap();
    assert_eq!(ids(&store), ["original"]);
    Ok(())
}

#[test]
fn exhausted_order_refuses_new_intent_but_duplicate_stays_noop(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::in_memory().unwrap();
    let e = event("last", 1, "Commit");
    store.outbox_put(&e).unwrap();
    store
        .conn
        .execute("UPDATE outbox SET queue_order=?1", [i64::MAX])
        .unwrap();
    store.outbox_put(&e).unwrap();
    assert!(store.outbox_put(&event("overflow", 0, "Welcome")).is_err());
    assert_eq!(store.outbox_get("last").unwrap(), Some(e));
    assert_eq!(ids(&store), ["last"]);
    Ok(())
}

#[test]
fn disk_reopen_and_vacuum_into_keep_named_order() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::temp_dir().join(types::new_id("outbox-fifo")?);
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("original.sqlite");
    let copy = directory.join("copy.sqlite");
    {
        let store = Store::open(path.to_str().unwrap()).unwrap();
        legacy(&store);
        insert_legacy(&store, &event("first", 300, "Commit"), 8);
        insert_legacy(&store, &event("second", 100, "Welcome"), 700);
    }
    {
        let store = Store::open(path.to_str().unwrap()).unwrap();
        assert_eq!(ids(&store), ["first", "second"]);
        store
            .conn
            .execute("VACUUM INTO ?1", [copy.to_str().unwrap()])
            .unwrap();
    }
    {
        let store = Store::open(copy.to_str().unwrap()).unwrap();
        assert_eq!(ids(&store), ["first", "second"]);
        assert_eq!(
            store
                .conn
                .query_row(
                    "SELECT queue_order FROM outbox WHERE client_id='second'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            700
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
    Ok(())
}

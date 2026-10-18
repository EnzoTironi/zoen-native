//! Real-device receipt history must not depend on the demo or a live relay replay.
use super::*;
use roda_types::{EventBody, Identity, IdentityKind, Privacy, Role, SpaceKind};
use std::collections::BTreeMap;
use std::path::PathBuf;

struct Database {
    directory: PathBuf,
    path: String,
}

impl Database {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(roda_types::new_id("peer-receipts"));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("core.sqlite").to_string_lossy().into_owned();
        Self { directory, path }
    }

    fn open(&self) -> Arc<RodaEngine> {
        RodaEngine::open(self.path.clone(), "en".into()).unwrap()
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).unwrap();
    }
}

struct Conversation {
    space: String,
    me: String,
    peer: String,
    outsider: String,
    text: TimelineEntry,
    voice: TimelineEntry,
}

fn person(name: &str) -> Identity {
    Identity {
        id: String::new(),
        kind: IdentityKind::Person,
        name: name.into(),
        handle: name.to_lowercase(),
        tint_hex: "#246844".into(),
        glyph: None,
        owner: None,
        owner_proof: None,
        bio: "Synthetic receipt test".into(),
    }
}

fn fixture() -> (Database, Arc<RodaEngine>, Conversation) {
    let db = Database::new();
    let e = db.open();
    let (space, me, peer, outsider) = {
        let mut engine = e.lock();
        let me = engine.create_identity(person("Owner"), true).unwrap();
        let peer = engine.create_identity(person("Peer"), true).unwrap();
        let outsider = engine.create_identity(person("Outsider"), true).unwrap();
        engine.store.set_meta("me", &me).unwrap();
        engine.me = Some(me.clone());
        let space = roda_types::new_id("sp");
        engine
            .append(
                &space,
                &me,
                EventBody::SpaceCreated {
                    title: "Synthetic receipt conversation".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::Closed,
                },
            )
            .unwrap();
        engine
            .append(
                &space,
                &me,
                EventBody::MemberAdded {
                    identity: peer.clone(),
                    role: Role::Member,
                },
            )
            .unwrap();
        (space, me, peer, outsider)
    };
    // Confirmed signed history is already on disk before this device goes offline.
    let text = e
        .send_message(space.clone(), "A retained message".into())
        .unwrap();
    let voice = e
        .send_voice_note(
            space.clone(),
            vec![7; 900],
            "audio/mp4".into(),
            1500,
            vec![0.2, 0.8],
            "A retained voice transcript".into(),
            None,
            false,
        )
        .unwrap();
    {
        let mut engine = e.lock();
        engine.store.set_synced(&space).unwrap();
        engine.net.synced.insert(space.clone());
        assert!(engine.store.meta("seed_receipts").unwrap().is_none());
        assert!(engine.store.outbox().unwrap().is_empty());
    }
    assert!(text.seq > 0 && voice.seq > text.seq);
    let conversation = Conversation {
        space,
        me,
        peer,
        outsider,
        text,
        voice,
    };
    (db, e, conversation)
}

fn entry(e: &RodaEngine, c: &Conversation, id: &str) -> TimelineEntry {
    e.timeline(c.space.clone())
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == id)
        .unwrap()
}

fn assert_delivery(e: &RodaEngine, c: &Conversation, delivery: Delivery) {
    for original in [&c.text, &c.voice] {
        let current = entry(e, c, &original.id);
        assert_eq!(current.delivery, delivery, "{}", original.id);
        assert_eq!(current.voice, original.voice);
        assert_eq!(current.kind, original.kind);
        assert!(current.author.is_me);
    }
    assert_eq!(
        e.space(c.space.clone()).unwrap().last_delivery,
        Some(delivery)
    );
}

// Apply an actual signed, relay-sequenced membership change to canonical history.
fn membership(e: &RodaEngine, c: &Conversation, body: EventBody) {
    let mut engine = e.lock();
    let signed = engine.author_for(&c.me).unwrap().sign_event(
        &c.space,
        &roda_types::new_id("membership"),
        crate::engine::now_ms(),
        engine.logs[&c.space].head(),
        body,
    );
    let mut relay = engine.logs[&c.space].clone();
    let event = relay.sequence(signed).clone();
    assert_eq!(
        engine.ingest(roda_proto::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: roda_proto::Envelope::plain(&event),
        }),
        crate::sync::Ingest::Applied
    );
}

type ReceiptRow = (String, String, String, String);
type MemoryReceipts = BTreeMap<String, BTreeMap<String, (u64, u64)>>;

#[derive(Debug, PartialEq)]
struct ReceiptSnapshot {
    rows: Vec<ReceiptRow>,
    memory: MemoryReceipts,
    meta: Vec<(String, String)>,
}

fn snapshot(e: &RodaEngine) -> ReceiptSnapshot {
    let engine = e.lock();
    let mut query = engine
        .store
        .conn()
        .prepare(
            "SELECT space, identity, delivered, read FROM peer_receipts ORDER BY space, identity",
        )
        .unwrap();
    let rows = query
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let memory = engine
        .net
        .receipts
        .iter()
        .map(|(space, peers)| {
            (
                space.clone(),
                peers
                    .iter()
                    .map(|(peer, mark)| (peer.clone(), *mark))
                    .collect(),
            )
        })
        .collect();
    let mut query = engine
        .store
        .conn()
        .prepare("SELECT key, value FROM meta ORDER BY key")
        .unwrap();
    let meta = query
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    ReceiptSnapshot { rows, memory, meta }
}

#[test]
fn own_text_and_voice_keep_delivered_then_read_after_offline_cold_reopens() {
    let (db, e, c) = fixture();
    assert_delivery(&e, &c, Delivery::Sent);
    assert!(e.lock().receipt(&c.space, &c.peer, c.voice.seq, 0).unwrap());
    assert_delivery(&e, &c, Delivery::Delivered);
    drop(e);
    let e = db.open();
    assert_delivery(&e, &c, Delivery::Delivered);
    assert!(e.lock().receipt(&c.space, &c.peer, 0, c.voice.seq).unwrap());
    assert_delivery(&e, &c, Delivery::Read);
    drop(e);
    let e = db.open();
    assert_delivery(&e, &c, Delivery::Read);
    assert_eq!(
        e.file_bytes(c.voice.voice.as_ref().unwrap().item_id.clone(), None)
            .unwrap(),
        Some(vec![7; 900])
    );
    assert!(e.verify_log(c.space.clone()).valid);
}

#[test]
fn independent_watermark_maxima_are_exact_u64_and_stale_replay_cannot_lower_them() {
    let (db, e, c) = fixture();
    assert!(e.lock().receipt(&c.space, &c.peer, u64::MAX, 1).unwrap());
    assert!(e
        .lock()
        .receipt(&c.space, &c.peer, 2, u64::MAX - 1)
        .unwrap());
    let before = snapshot(&e);
    assert_eq!(
        before.rows,
        vec![(
            c.space.clone(),
            c.peer.clone(),
            format!("{:020}", u64::MAX),
            format!("{:020}", u64::MAX - 1)
        )]
    );
    for (delivered, read) in [
        (0, 0),
        (u64::MAX, 1),
        (1, u64::MAX - 2),
        (u64::MAX, u64::MAX - 1),
    ] {
        assert!(!e
            .lock()
            .receipt(&c.space, &c.peer, delivered, read)
            .unwrap());
        assert_eq!(snapshot(&e), before);
    }
    drop(e);
    let e = db.open();
    assert_eq!(snapshot(&e), before);
    assert!(e.lock().receipt(&c.space, &c.peer, 0, u64::MAX).unwrap());
    assert_eq!(
        e.lock().net.receipts[&c.space][&c.peer],
        (u64::MAX, u64::MAX)
    );
    assert_eq!(snapshot(&e).rows[0].3, format!("{:020}", u64::MAX));
}

#[test]
fn unknown_foreign_removed_and_inaccessible_receipts_never_write() {
    let (_db, e, c) = fixture();
    let before = snapshot(&e);
    for (space, peer) in [
        ("missing-space", c.peer.as_str()),
        (c.space.as_str(), "unknown-person"),
        (c.space.as_str(), c.outsider.as_str()),
    ] {
        assert!(!e.lock().receipt(space, peer, u64::MAX, u64::MAX).unwrap());
        assert_eq!(snapshot(&e), before);
    }
    membership(
        &e,
        &c,
        EventBody::MemberRemoved {
            identity: c.peer.clone(),
        },
    );
    let before = snapshot(&e);
    assert!(!e
        .lock()
        .receipt(&c.space, &c.peer, u64::MAX, u64::MAX)
        .unwrap());
    assert_eq!(snapshot(&e), before);
    membership(
        &e,
        &c,
        EventBody::MemberAdded {
            identity: c.peer.clone(),
            role: Role::Member,
        },
    );
    membership(
        &e,
        &c,
        EventBody::MemberRemoved {
            identity: c.me.clone(),
        },
    );
    let before = snapshot(&e);
    for peer in [&c.peer, &c.me] {
        assert!(!e
            .lock()
            .receipt(&c.space, peer, u64::MAX, u64::MAX)
            .unwrap());
        assert_eq!(snapshot(&e), before);
    }
}

#[test]
fn delivery_aggregates_only_current_members_and_read_implies_delivered() {
    let (_db, e, c) = fixture();
    membership(
        &e,
        &c,
        EventBody::MemberAdded {
            identity: c.outsider.clone(),
            role: Role::Member,
        },
    );
    assert!(e.lock().receipt(&c.space, &c.peer, 0, c.voice.seq).unwrap());
    assert_delivery(&e, &c, Delivery::Sent);
    assert!(e
        .lock()
        .receipt(&c.space, &c.outsider, c.voice.seq, 0)
        .unwrap());
    assert_delivery(&e, &c, Delivery::Delivered);
    membership(
        &e,
        &c,
        EventBody::MemberRemoved {
            identity: c.outsider.clone(),
        },
    );
    assert_delivery(&e, &c, Delivery::Read);
    assert_eq!(
        e.lock().net.receipts[&c.space][&c.peer],
        (c.voice.seq, c.voice.seq)
    );
}

#[test]
fn receipt_history_does_not_override_pending_or_permanently_failed_sends() {
    let (db, e, c) = fixture();
    let pending = e
        .send_message(c.space.clone(), "Still queued".into())
        .unwrap();
    assert!(e
        .lock()
        .receipt(&c.space, &c.peer, u64::MAX, u64::MAX)
        .unwrap());
    assert_eq!(entry(&e, &c, &pending.id).delivery, Delivery::Sending);
    {
        let mut engine = e.lock();
        let client = engine
            .store
            .outbox()
            .unwrap()
            .into_iter()
            .find(|row| row.event.space == c.space)
            .unwrap()
            .event
            .client_id;
        assert!(engine
            .reject(&client, "Synthetic permanent refusal", true, None)
            .user_failed());
    }
    assert_eq!(entry(&e, &c, &pending.id).delivery, Delivery::Failed);
    assert_eq!(entry(&e, &c, &c.voice.id).delivery, Delivery::Read);
    drop(e);
    let e = db.open();
    assert_eq!(entry(&e, &c, &pending.id).delivery, Delivery::Failed);
    assert_eq!(entry(&e, &c, &c.voice.id).delivery, Delivery::Read);
}

#[test]
fn sql_insert_and_update_failures_leave_disk_and_published_ticks_unchanged_then_retry() {
    for operation in ["INSERT", "UPDATE"] {
        let (_db, e, c) = fixture();
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
}

#[test]
fn reload_preserves_watermarks_and_wipe_or_erase_device_clears_only_its_database() {
    for through_public_api in [false, true] {
        let (db, e, c) = fixture();
        let (_other_db, other, other_c) = fixture();
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
}

#[test]
fn device_peer_receipts_are_excluded_from_backup_without_changing_the_live_store() {
    let (db, e, c) = fixture();
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
}

#[test]
fn early_receipts_survive_sync_boundaries_until_membership_and_foreign_pairs_stay_bounded() {
    let (_db, e, c) = fixture();
    let mut deferred = crate::net::DeferredReceipts::default();
    let mut dirty = std::collections::HashSet::new();
    let before = snapshot(&e);
    assert!(!deferred
        .receive(&mut e.lock(), &c.space, &c.outsider, c.voice.seq, 0)
        .unwrap());
    assert!(!deferred
        .receive(&mut e.lock(), &c.space, &c.outsider, 0, c.voice.seq)
        .unwrap());
    // Neither a partial nor a full wire SyncDone proves every room's dependencies
    // have landed; a future member's pair must survive both boundaries.
    for _ in 0..2 {
        deferred.flush(&mut e.lock(), &mut dirty).unwrap();
    }
    assert_eq!(snapshot(&e), before);
    assert!(dirty.is_empty());
    membership(
        &e,
        &c,
        EventBody::MemberAdded {
            identity: c.outsider.clone(),
            role: Role::Member,
        },
    );
    let before_flush = snapshot(&e);
    e.lock().store.conn().execute_batch(
        "CREATE TEMP TRIGGER refuse_deferred BEFORE INSERT ON peer_receipts BEGIN SELECT RAISE(ABORT, 'synthetic deferred failure'); END;",
    ).unwrap();
    assert!(matches!(
        deferred.flush(&mut e.lock(), &mut dirty),
        Err(CoreError::Storage { .. })
    ));
    assert!(dirty.is_empty());
    assert_eq!(snapshot(&e), before_flush);
    e.lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER refuse_deferred")
        .unwrap();
    deferred.flush(&mut e.lock(), &mut dirty).unwrap();
    assert_eq!(dirty, std::collections::HashSet::from([c.space.clone()]));
    assert_eq!(
        e.lock().net.receipts[&c.space][&c.outsider],
        (c.voice.seq, c.voice.seq)
    );
    assert_eq!(
        snapshot(&e).rows,
        vec![(
            c.space.clone(),
            c.outsider.clone(),
            format!("{:020}", c.voice.seq),
            format!("{:020}", c.voice.seq)
        )]
    );

    let before = snapshot(&e);
    let mut dirty = std::collections::HashSet::new();
    assert!(!deferred
        .receive(
            &mut e.lock(),
            &c.space,
            "foreign-person",
            u64::MAX,
            u64::MAX
        )
        .unwrap());
    deferred.flush(&mut e.lock(), &mut dirty).unwrap();
    assert_eq!(snapshot(&e), before);
    assert!(dirty.is_empty());
    // A fresh socket has 1024 bounded slots. Unknown pairs never reach storage;
    // overflow requests reconnect instead of silently evicting a legitimate pair.
    let mut deferred = crate::net::DeferredReceipts::default();
    for index in 0..1024 {
        assert!(!deferred
            .receive(&mut e.lock(), &c.space, &format!("unknown-{index}"), 1, 0)
            .unwrap());
    }
    assert!(matches!(
        deferred.receive(&mut e.lock(), &c.space, "overflow", 1, 0),
        Err(CoreError::Invalid { .. })
    ));
    deferred.flush(&mut e.lock(), &mut dirty).unwrap();
    assert_eq!(snapshot(&e), before);
    assert!(dirty.is_empty());
}

#[test]
fn legacy_sample_receipts_migrate_only_canonical_peers_and_failure_keeps_the_blob() {
    let (db, e, c) = fixture();
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
}

#[test]
fn own_identity_read_storage_failure_propagates_through_receive_and_retry_is_durable() {
    let (db, e, c) = fixture();
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
}

#[test]
fn pending_membership_cannot_admit_a_new_peer_or_suppress_a_confirmed_peers_receipt() {
    let (_db, e, c) = fixture();
    e.lock()
        .add_member(&c.space, &c.outsider, Role::Member)
        .unwrap();
    let before = snapshot(&e);
    assert!(!e
        .lock()
        .receipt(&c.space, &c.outsider, 0, c.voice.seq)
        .unwrap());
    assert_eq!(snapshot(&e), before);
    let mut deferred = crate::net::DeferredReceipts::default();
    let mut dirty = std::collections::HashSet::new();
    assert!(!deferred
        .receive(&mut e.lock(), &c.space, &c.outsider, 0, c.voice.seq)
        .unwrap());
    deferred.flush(&mut e.lock(), &mut dirty).unwrap();
    assert!(dirty.is_empty());
    assert_eq!(snapshot(&e), before);
    {
        let mut engine = e.lock();
        let pending = engine.store.outbox().unwrap().into_iter().find(|row| {
            matches!(&row.event.body, EventBody::MemberAdded { identity, .. } if identity == &c.outsider)
        }).unwrap().event;
        let mut relay = engine.logs[&c.space].clone();
        let event = relay.sequence(pending).clone();
        assert_eq!(
            engine.ingest(roda_proto::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: roda_proto::Envelope::plain(&event),
            }),
            crate::sync::Ingest::Confirmed
        );
    }
    deferred.flush(&mut e.lock(), &mut dirty).unwrap();
    assert_eq!(dirty, std::collections::HashSet::from([c.space.clone()]));
    assert_eq!(
        e.lock().net.receipts[&c.space][&c.outsider],
        (c.voice.seq, c.voice.seq)
    );

    e.lock().remove_member(&c.space, &c.peer).unwrap();
    assert!(!e.lock().state.spaces[&c.space]
        .members
        .iter()
        .any(|(id, _)| id == &c.peer));
    assert!(e
        .lock()
        .receipt(&c.space, &c.peer, u64::MAX, u64::MAX)
        .unwrap());
    assert_eq!(
        e.lock().net.receipts[&c.space][&c.peer],
        (u64::MAX, u64::MAX)
    );
    assert_eq!(snapshot(&e).rows.len(), 2);
    assert_delivery(&e, &c, Delivery::Read);
}

//! Signal C02/C04-inspired own signed queue faults: no skipped chat head or discarded intent.
use super::*;
use event_log::content::Sealed;
use rusqlite::{params, types::Value};

struct Fixture {
    engine: Engine,
    root: [u8; 32],
    device: [u8; 32],
}

fn sequence(engine: &Engine, env: Envelope) -> Sequenced {
    let (seq, prev) = engine.logs.get(env.space()).map_or_else(
        || (0, GENESIS_PREV.to_string()),
        |log| (log.next_seq(), log.head().unwrap().hash),
    );
    Sequenced {
        seq,
        hash: chain_hash(env.space(), seq, &prev, &env.wire_hash()),
        prev,
        env,
    }
}

impl Fixture {
    fn new(path: &str, second_privacy: Privacy) -> Self {
        let mut engine = Engine::open(path).unwrap();
        let (root, device, _) = engine
            .create_account("Queue faults", "queue_faults", "http://127.0.0.1:1")
            .unwrap();
        let author = engine.net.author.clone().unwrap();
        for (space, privacy) in [
            ("sp_fault_a", Privacy::Closed),
            ("sp_fault_b", second_privacy),
        ] {
            let env = Envelope::plain(&author.sign_event(
                space,
                &format!("{space}-genesis"),
                1,
                None,
                EventBody::SpaceCreated {
                    title: space.into(),
                    kind: SpaceKind::Group,
                    privacy,
                },
            ));
            assert_eq!(engine.ingest(sequence(&engine, env)), Ingest::Applied);
        }
        Self {
            engine,
            root,
            device,
        }
    }

    fn message(&mut self, space: &str, n: i64) -> Event {
        let author = self.engine.net.author.clone().unwrap();
        self.engine
            .append_synced(
                space,
                &author,
                100 + n,
                EventBody::MessagePosted {
                    message: format!("message-{n}"),
                    text: format!("Original message {n}"),
                    attaches: None,
                    reply: None,
                    media: None,
                    forwarded: false,
                },
            )
            .unwrap()
    }

    fn real_handshakes(&mut self) -> (Event, Event) {
        let space = "sp_fault_b";
        self.engine.create_mls_group(space).unwrap();
        let peer_author = Author::device(&Signer::from_secret(&self.root), Signer::generate());
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        ::mls::migrate(&mut conn).unwrap();
        let peer = ::mls::Device::new(
            &conn,
            ::mls::sealed::state_key(&peer_author.key.secret()),
            &peer_author.identity,
            peer_author.key.secret(),
            peer_author.cert.as_deref().unwrap(),
        )
        .unwrap();
        let packages = peer.key_packages(1, false).unwrap();
        let adding = self
            .engine
            .device()
            .unwrap()
            .commit(space, &packages, &Default::default())
            .unwrap();
        let commit = self
            .engine
            .handshake_event(
                space,
                Sealed::new(SealedKind::Commit, ::mls::SUITE_ID, adding.commit),
            )
            .unwrap();
        let welcome = self
            .engine
            .handshake_event(
                space,
                Sealed::new(
                    SealedKind::Welcome,
                    ::mls::SUITE_ID,
                    adding.welcome.unwrap(),
                ),
            )
            .unwrap();
        for event in [&commit, &welcome] {
            self.engine.store.outbox_put(event).unwrap();
        }
        self.engine
            .queued_recovery_commit(vec![commit.clone(), welcome.clone()]);
        (commit, welcome)
    }
}

type RawRow = (String, String, Value, i64, Option<String>, i64, i64, i64);
fn raw_queue(engine: &Engine) -> Vec<RawRow> {
    let mut stmt = engine.store.conn().prepare("SELECT client_id,space,json,attempts,last_error,failed,created_ms,queue_order FROM outbox ORDER BY queue_order").unwrap();
    stmt.query_map([], |r| {
        Ok((
            r.get(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
            r.get(6)?,
            r.get(7)?,
        ))
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

fn original_json(engine: &Engine, event: &Event) -> String {
    engine
        .store
        .conn()
        .query_row(
            "SELECT json FROM outbox WHERE client_id=?1",
            [&event.client_id],
            |r| r.get(0),
        )
        .unwrap()
}

fn set_json(engine: &Engine, event: &Event, json: Value) {
    assert_eq!(
        engine
            .store
            .conn()
            .execute(
                "UPDATE outbox SET json=?1 WHERE client_id=?2",
                params![json, event.client_id]
            )
            .unwrap(),
        1
    );
}

#[test]
fn malformed_or_unreadable_head_blocks_only_its_reliably_associated_chat_without_mutating_queue() {
    for fault in [Value::Text("{not JSON".into()), Value::Blob(vec![0])] {
        let mut f = Fixture::new(":memory:", Privacy::Closed);
        let independent = f.message("sp_fault_b", 0);
        let first = f.message("sp_fault_a", 1);
        let second = f.message("sp_fault_a", 2);
        let original = original_json(&f.engine, &first);
        let healthy = f.engine.outbox_envelopes();
        set_json(&f.engine, &first, fault.clone());
        match fault {
            Value::Text(_) => assert!(matches!(
                f.engine.store.outbox_get(&first.client_id),
                Err(store::StoreError::Json(_))
            )),
            Value::Blob(_) => assert!(matches!(
                f.engine.store.outbox_get(&first.client_id),
                Err(store::StoreError::Sqlite(_))
            )),
            _ => unreachable!("explicit fixture faults"),
        }
        let broken = raw_queue(&f.engine);
        let offered = f.engine.outbox_envelopes();
        assert!(
            offered.iter().all(|e| e.space() != "sp_fault_a"),
            "an unreadable durable head must not be treated as completed"
        );
        assert_eq!(
            offered.iter().map(Envelope::client_id).collect::<Vec<_>>(),
            [independent.client_id.as_str()]
        );
        assert_eq!(
            raw_queue(&f.engine),
            broken,
            "read cannot delete/re-sign/fail/retry the queue"
        );
        assert_eq!(f.engine.outbox_len(), 3);
        set_json(&f.engine, &first, Value::Text(original));
        assert_eq!(f.engine.outbox_envelopes(), healthy);
        assert_eq!(
            f.engine
                .outbox_envelopes()
                .iter()
                .filter(|e| e.space() == "sp_fault_a")
                .map(Envelope::client_id)
                .collect::<Vec<_>>(),
            [first.client_id.as_str(), second.client_id.as_str()]
        );
    }
}

#[test]
fn unreadable_head_without_retained_association_does_not_return_an_already_collected_prefix() {
    let mut f = Fixture::new(":memory:", Privacy::Closed);
    f.message("sp_fault_b", 0);
    let first = f.message("sp_fault_a", 1);
    f.message("sp_fault_a", 2);
    let original = original_json(&f.engine, &first);
    let healthy = f.engine.outbox_envelopes();
    set_json(&f.engine, &first, Value::Text("{not JSON".into()));
    let association = f.engine.net.pending.remove(&first.client_id).unwrap();
    let broken = raw_queue(&f.engine);
    assert!(
        f.engine.outbox_envelopes().is_empty(),
        "do not guess an unknown association or leak a partial offer prefix"
    );
    assert_eq!(raw_queue(&f.engine), broken);
    set_json(&f.engine, &first, Value::Text(original));
    f.engine.net.pending.insert(first.client_id, association);
    assert_eq!(f.engine.outbox_envelopes(), healthy);
}

#[test]
fn malformed_other_chat_does_not_release_real_mls_welcome_ahead_of_its_commit() {
    let mut f = Fixture::new(":memory:", Privacy::EndToEnd);
    let first = f.message("sp_fault_a", 0);
    let (commit, welcome) = f.real_handshakes();
    let healthy = f.engine.outbox_envelopes();
    assert!(healthy.iter().any(|e| e.client_id() == commit.client_id));
    assert!(healthy.iter().all(|e| e.client_id() != welcome.client_id));
    set_json(&f.engine, &first, Value::Text("{not JSON".into()));
    let broken = raw_queue(&f.engine);
    let offered = f.engine.outbox_envelopes();
    assert!(
        offered.iter().any(|e| e.client_id() == commit.client_id),
        "an independently scoped real commit remains live"
    );
    assert!(
        offered.iter().all(|e| e.client_id() != welcome.client_id),
        "an unrelated parse failure cannot erase the Commit-to-Welcome dependency"
    );
    assert_eq!(raw_queue(&f.engine), broken);
}

#[test]
fn exact_head_repair_and_cold_reopen_preserve_original_ids_order_signatures_and_retry_fields() {
    let path = std::env::temp_dir().join(types::new_id("outbox-head-repair"));
    let mut f = Fixture::new(path.to_str().unwrap(), Privacy::Closed);
    f.message("sp_fault_b", 0);
    let first = f.message("sp_fault_a", 1);
    f.message("sp_fault_a", 2);
    f.engine
        .store
        .outbox_note(&first.client_id, "original retry retained", false)
        .unwrap();
    let original = original_json(&f.engine, &first);
    let healthy = f.engine.outbox_envelopes();
    let original_rows = raw_queue(&f.engine);
    set_json(&f.engine, &first, Value::Text("{not JSON".into()));
    assert!(f
        .engine
        .outbox_envelopes()
        .iter()
        .all(|e| e.space() != "sp_fault_a"));
    set_json(&f.engine, &first, Value::Text(original));
    assert_eq!(raw_queue(&f.engine), original_rows);
    let key = f.device;
    drop(f);
    let mut cold = Engine::open(path.to_str().unwrap()).unwrap();
    assert!(cold.unlock(Some(key.to_vec())).unwrap());
    assert_eq!(raw_queue(&cold), original_rows);
    assert_eq!(cold.outbox_envelopes(), healthy);
    assert_eq!(cold.outbox_len(), 3);
    drop(cold);
    std::fs::remove_file(path).unwrap();
}

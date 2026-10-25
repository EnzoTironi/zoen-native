//! Signal C02/C04-inspired own signed queue faults: no skipped chat head or discarded intent.
use super::*;
use event_log::content::Sealed;
use rusqlite::{params, types::Value};

pub(super) struct OwnedDatabase(pub(super) std::path::PathBuf);
impl Drop for OwnedDatabase {
    fn drop(&mut self) {
        if self.0.exists() {
            let removed = std::fs::remove_file(&self.0);
            if !std::thread::panicking() {
                removed.expect("owned SQLite fixture removed after closing Engine");
            }
        }
    }
}

pub(super) struct Fixture {
    pub(super) engine: Engine,
    root: [u8; 32],
    pub(super) device: [u8; 32],
}

fn sequence(
    engine: &Engine,
    env: Envelope,
) -> std::result::Result<Sequenced, Box<dyn std::error::Error>> {
    let (seq, prev) = engine.logs.get(env.space()).map_or_else(
        || (0, GENESIS_PREV.to_string()),
        |log| (log.next_seq(), log.head().unwrap().hash),
    );
    Ok(Sequenced {
        seq,
        hash: chain_hash(env.space(), seq, &prev, &env.wire_hash())?,
        prev,
        env,
    })
}

impl Fixture {
    fn new(
        path: &str,
        second_privacy: Privacy,
    ) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        Self::with_privacies(path, Privacy::Closed, second_privacy)
    }

    pub(super) fn with_privacies(
        path: &str,
        first_privacy: Privacy,
        second_privacy: Privacy,
    ) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let mut engine = Engine::open(path).unwrap();
        let (root, device, _) = engine
            .create_account("Queue faults", "queue_faults", "http://127.0.0.1:1")
            .unwrap();
        let author = engine.net.author.clone().unwrap();
        for (space, privacy) in [
            ("sp_fault_a", first_privacy),
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
            )?)?;
            assert_eq!(engine.ingest(sequence(&engine, env)?), Ingest::Applied);
        }
        Ok(Self {
            engine,
            root,
            device,
        })
    }

    pub(super) fn message(&mut self, space: &str, n: i64) -> Event {
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

    fn real_handshakes(
        &mut self,
    ) -> std::result::Result<(Event, Event), Box<dyn std::error::Error>> {
        let space = "sp_fault_b";
        self.engine.create_mls_group(space).unwrap();
        let peer_author = Author::device(&Signer::from_secret(&self.root), Signer::generate()?);
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
        Ok((commit, welcome))
    }
}

type RawRow = (String, String, Value, i64, Option<String>, i64, i64, i64);
pub(super) fn raw_queue(engine: &Engine) -> Vec<RawRow> {
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

pub(super) fn original_json(engine: &Engine, event: &Event) -> String {
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

pub(super) fn set_json(engine: &Engine, event: &Event, json: Value) {
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
fn malformed_or_unreadable_head_blocks_only_its_reliably_associated_chat_without_mutating_queue(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for fault in [Value::Text("{not JSON".into()), Value::Blob(vec![0])] {
        let mut f = Fixture::new(":memory:", Privacy::Closed)?;
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
        let head = f
            .engine
            .store
            .outbox_heads()
            .unwrap()
            .into_iter()
            .find(|h| h.client_id == first.client_id)
            .unwrap();
        let expected = if matches!(fault, Value::Text(_)) {
            OutboxHeadFailure::Corrupt
        } else {
            OutboxHeadFailure::Read
        };
        assert_eq!(f.engine.read_outbox_head(&head).unwrap_err(), expected);
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
    Ok(())
}

#[test]
fn unreadable_head_without_retained_association_does_not_return_an_already_collected_prefix(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new(":memory:", Privacy::Closed)?;
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
    Ok(())
}

#[test]
fn malformed_other_chat_does_not_release_real_mls_welcome_ahead_of_its_commit(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new(":memory:", Privacy::EndToEnd)?;
    let first = f.message("sp_fault_a", 0);
    let (commit, welcome) = f.real_handshakes()?;
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
    Ok(())
}

#[test]
fn exact_head_repair_and_cold_reopen_preserve_original_ids_order_signatures_and_retry_fields(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let database = OwnedDatabase(std::env::temp_dir().join(types::new_id("outbox-head-repair")?));
    let mut f = Fixture::new(database.0.to_str().unwrap(), Privacy::Closed)?;
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
    let mut cold = Engine::open(database.0.to_str().unwrap()).unwrap();
    assert!(cold.unlock(Some(key.to_vec())).unwrap());
    assert_eq!(raw_queue(&cold), original_rows);
    assert_eq!(cold.outbox_envelopes(), healthy);
    assert_eq!(cold.outbox_len(), 3);
    drop(cold);
    drop(database);
    Ok(())
}

#[test]
fn missing_after_head_query_blocks_its_original_lane_and_exact_row_restoration_recovers_order(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new(":memory:", Privacy::Closed)?;
    let independent = f.message("sp_fault_b", 0);
    let first = f.message("sp_fault_a", 1);
    f.message("sp_fault_a", 2);
    let healthy = f.engine.outbox_envelopes();
    let original_rows = raw_queue(&f.engine);
    let row = original_rows
        .iter()
        .find(|r| r.0 == first.client_id)
        .unwrap();
    let heads = f.engine.store.outbox_heads().unwrap();
    let head = heads
        .iter()
        .find(|h| h.client_id == first.client_id)
        .unwrap();
    f.engine.store.outbox_remove(&first.client_id).unwrap();
    assert_eq!(
        f.engine.read_outbox_head(head).unwrap_err(),
        OutboxHeadFailure::Missing
    );
    let broken = raw_queue(&f.engine);
    let offered = f.engine.outbox_envelopes_from_heads(&HashMap::new(), heads);
    assert_eq!(
        offered.iter().map(Envelope::client_id).collect::<Vec<_>>(),
        [independent.client_id.as_str()]
    );
    assert_eq!(raw_queue(&f.engine), broken);
    // Restore the exact retained row, including its original enqueue position.
    // This is test fault repair, never an automatic runtime reconstruction.
    f.engine.store.conn().execute("INSERT INTO outbox(client_id,space,json,attempts,last_error,failed,created_ms,queue_order) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![row.0,row.1,row.2,row.3,row.4,row.5,row.6,row.7]).unwrap();
    assert_eq!(raw_queue(&f.engine), original_rows);
    assert_eq!(f.engine.outbox_envelopes(), healthy);
    Ok(())
}

#[test]
fn corrupt_row_cannot_change_its_cached_chat_association_to_release_a_partial_prefix(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new(":memory:", Privacy::Closed)?;
    f.message("sp_fault_b", 0);
    let first = f.message("sp_fault_a", 1);
    f.message("sp_fault_a", 2);
    let healthy = f.engine.outbox_envelopes();
    let json = original_json(&f.engine, &first);
    f.engine
        .store
        .conn()
        .execute(
            "UPDATE outbox SET json='{invalid', space='sp_fault_b' WHERE client_id=?1",
            [&first.client_id],
        )
        .unwrap();
    let broken = raw_queue(&f.engine);
    assert!(f.engine.outbox_envelopes().is_empty());
    assert_eq!(raw_queue(&f.engine), broken);
    f.engine
        .store
        .conn()
        .execute(
            "UPDATE outbox SET json=?1, space=?2 WHERE client_id=?3",
            params![json, first.space, first.client_id],
        )
        .unwrap();
    assert_eq!(f.engine.outbox_envelopes(), healthy);
    Ok(())
}

#[test]
fn unreadable_queue_or_handshake_metadata_never_becomes_an_empty_dependency_success(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for projection in [
        "SELECT queue_order FROM main.outbox",
        "SELECT client_id,space,attempts,last_error,failed,created_ms,queue_order FROM main.outbox",
    ] {
        let mut f = Fixture::new(":memory:", Privacy::Closed)?;
        f.message("sp_fault_b", 0);
        f.message("sp_fault_a", 1);
        let healthy = f.engine.outbox_envelopes();
        let original = raw_queue(&f.engine);
        // A temp read projection shadows only the owned fixture; actual durable
        // table, signed bytes and queue positions remain untouched.
        f.engine
            .store
            .conn()
            .execute_batch(&format!("CREATE TEMP VIEW outbox AS {projection}"))
            .unwrap();
        assert!(f.engine.outbox_envelopes().is_empty());
        f.engine
            .store
            .conn()
            .execute_batch("DROP VIEW temp.outbox")
            .unwrap();
        assert_eq!(raw_queue(&f.engine), original);
        assert_eq!(f.engine.outbox_envelopes(), healthy);
    }
    Ok(())
}

#[test]
fn deserializable_event_cannot_replace_signed_bytes_projection_signature_or_certificate(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new(":memory:", Privacy::Closed)?;
    let independent = f.message("sp_fault_b", 0);
    let first = f.message("sp_fault_a", 1);
    f.message("sp_fault_a", 2);
    let original = original_json(&f.engine, &first);
    let healthy = f.engine.outbox_envelopes();
    let head = f
        .engine
        .store
        .outbox_heads()
        .unwrap()
        .into_iter()
        .find(|h| h.client_id == first.client_id)
        .unwrap();
    for fault in [
        "empty-content",
        "body",
        "space",
        "cid",
        "time",
        "signature",
        "certificate",
    ] {
        let mut invalid = first.clone();
        match fault {
            "empty-content" => invalid.content.clear(),
            "body" => {
                let EventBody::MessagePosted { text, .. } = &mut invalid.body else {
                    panic!("actual original is a signed message");
                };
                *text = "Projection differs from signed bytes".into();
            }
            "space" => invalid.space = "sp_fault_b".into(),
            "cid" => invalid.client_id.push_str("-different"),
            "time" => invalid.at_ms += 1,
            "signature" => invalid.sig.clear(),
            "certificate" => invalid.cert = None,
            _ => unreachable!("explicit fixture faults"),
        }
        set_json(
            &f.engine,
            &first,
            Value::Text(serde_json::to_string(&invalid).unwrap()),
        );
        assert!(
            f.engine
                .store
                .outbox_get(&first.client_id)
                .unwrap()
                .is_some(),
            "serde accepts the damaged record; validity cannot be inferred from JSON: {fault}"
        );
        assert_eq!(
            f.engine.read_outbox_head(&head).unwrap_err(),
            OutboxHeadFailure::Corrupt,
            "{fault}"
        );
        let broken = raw_queue(&f.engine);
        let offered = f.engine.outbox_envelopes();
        assert_eq!(
            offered.iter().map(Envelope::client_id).collect::<Vec<_>>(),
            [independent.client_id.as_str()],
            "{fault}"
        );
        assert_eq!(raw_queue(&f.engine), broken, "{fault}");
    }
    set_json(&f.engine, &first, Value::Text(original));
    assert_eq!(f.engine.outbox_envelopes(), healthy);
    Ok(())
}

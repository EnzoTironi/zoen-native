use super::*;

fn confirm_personal_genesis(engine: &mut crate::engine::Engine, personal: &str) {
    let genesis = engine
        .outbox_envelopes()
        .into_iter()
        .find(|e| e.space() == personal)
        .unwrap();
    let seq = 0;
    let prev = roda_types::GENESIS_PREV.to_string();
    let hash = roda_log::chain_hash(personal, seq, &prev, &genesis.wire_hash());
    assert_eq!(
        engine.ingest(roda_proto::Sequenced {
            seq,
            prev,
            hash,
            env: genesis
        }),
        crate::sync::Ingest::Confirmed
    );
    engine.mls_seal_outbox().unwrap();
}

#[test]
fn personal_state_is_queued_and_replayed_on_a_linked_identity() {
    let a = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    let b = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    let mut a = a.lock();
    a.create_account("Alice", "alice", "https://relay.example")
        .unwrap();
    let personal = a.personal_space().unwrap();
    assert!(a.net.synced.contains(&personal));
    assert!(a.is_e2e(&personal));
    let me = a.me.clone().unwrap();
    a.append(
        &personal,
        &me,
        roda_types::EventBody::ThreadStateSet {
            space: "chat".into(),
            root: "root".into(),
            follow: Some(true),
            read_up_to: 3,
            read_only: false,
        },
    )
    .unwrap();
    let mut b = b.lock();
    b.store.set_meta("me", &me).unwrap();
    b.reload().unwrap();
    let pending: Vec<_> = a
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .filter(|p| p.event.space == personal)
        .collect();
    assert_eq!(pending.len(), 2);
    confirm_personal_genesis(&mut a, &personal);
    let envelopes = a.outbox_envelopes();
    let state_envelope = envelopes
        .iter()
        .find(|e| e.client_id() == pending[1].event.client_id)
        .unwrap();
    assert!(state_envelope.is_sealed());
    assert!(state_envelope.body().is_none());
    assert!(!String::from_utf8_lossy(state_envelope.content()).contains("ThreadStateSet"));
    // Projection replay here uses decrypted logs; wire encryption is asserted above.
    b.store.set_synced(&personal).unwrap();
    let mut decrypted_log = roda_log::SpaceLog::new(&personal);
    for p in pending {
        let event = decrypted_log.sequence(p.event);
        b.store.append_event(event).unwrap();
    }
    b.reload().unwrap();
    assert_eq!(b.personal_space(), Some(personal.clone()));
    assert!(b.net.synced.contains(&personal));
    assert_eq!(b.store.events(&personal).unwrap().len(), 2);
    assert_eq!(
        b.state.thread_state.get(&("chat".into(), "root".into())),
        Some(&(Some(true), 3))
    );
}

#[test]
fn unlock_migrates_legacy_personal_state_once_without_rewriting_history() {
    let a = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    let mut a = a.lock();
    let (_, secret, _) = a
        .create_account("Alice", "alice", "https://relay.example")
        .unwrap();
    a.store
        .conn()
        .execute_batch("DELETE FROM outbox; DELETE FROM synced_spaces")
        .unwrap();
    a.reload().unwrap();
    let me = a.me.clone().unwrap();
    a.append_at(
        "legacy_personal",
        &me,
        1,
        roda_types::EventBody::SpaceCreated {
            title: "Personal".into(),
            kind: roda_types::SpaceKind::Personal,
            privacy: roda_types::Privacy::EndToEnd,
        },
    )
    .unwrap();
    a.append_at(
        "legacy_personal",
        &me,
        2,
        roda_types::EventBody::ThreadStateSet {
            space: "chat".into(),
            root: "root".into(),
            follow: Some(false),
            read_up_to: 42,
            read_only: false,
        },
    )
    .unwrap();
    let original = a.store.events("legacy_personal").unwrap();
    a.store.conn().execute_batch("CREATE TEMP TRIGGER refuse_personal_copy BEFORE INSERT ON outbox WHEN NEW.json LIKE '%ThreadStateSet%' BEGIN SELECT RAISE(FAIL, 'test migration failure'); END;").unwrap();
    assert!(a.unlock(Some(secret.to_vec())).is_err());
    assert_eq!(a.personal_space().as_deref(), Some("legacy_personal"));
    assert!(a.store.outbox().unwrap().is_empty());
    a.store
        .conn()
        .execute_batch("DROP TRIGGER refuse_personal_copy")
        .unwrap();
    a.unlock(Some(secret.to_vec())).unwrap();
    let migrated = a.personal_space().unwrap();
    assert_ne!(migrated, "legacy_personal");
    assert!(a.net.synced.contains(&migrated));
    assert!(a.is_e2e(&migrated));
    let pending = a.store.outbox().unwrap();
    assert_eq!(pending.len(), 2);
    assert!(pending
        .iter()
        .any(|p| matches!(p.event.body, roda_types::EventBody::SpaceCreated { .. })));
    let copied = pending
        .iter()
        .find(|p| {
            matches!(
                p.event.body,
                roda_types::EventBody::ThreadStateSet {
                    read_up_to: 42,
                    follow: Some(false),
                    ..
                }
            )
        })
        .unwrap();
    assert_eq!(copied.event.at_ms, 2, "migration preserves follow recency");
    confirm_personal_genesis(&mut a, &migrated);
    let wire = a.outbox_envelopes();
    let migrated_state = wire
        .iter()
        .find(|env| env.client_id() == copied.event.client_id)
        .unwrap();
    assert!(migrated_state.is_sealed());
    assert!(migrated_state.body().is_none());
    assert_eq!(
        serde_json::to_string(&original).unwrap(),
        serde_json::to_string(&a.store.events("legacy_personal").unwrap()).unwrap()
    );
    a.unlock(Some(secret.to_vec())).unwrap();
    assert_eq!(a.store.outbox().unwrap().len(), 1);
}

#[test]
fn personal_thread_merge_is_monotonic_and_uses_newest_follow_in_either_order() {
    let engine = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    let mut e = engine.lock();
    e.create_account("Alice", "alice", "https://relay.example")
        .unwrap();
    let me = e.me.clone().unwrap();
    for (space, at, follow, read) in [("old", 10, true, 99), ("new", 20, false, 3)] {
        e.append_at(
            space,
            &me,
            1,
            roda_types::EventBody::SpaceCreated {
                title: "Personal".into(),
                kind: roda_types::SpaceKind::Personal,
                privacy: roda_types::Privacy::EndToEnd,
            },
        )
        .unwrap();
        e.append_at(
            space,
            &me,
            at,
            roda_types::EventBody::ThreadStateSet {
                space: "chat".into(),
                root: "root".into(),
                follow: Some(follow),
                read_up_to: read,
                read_only: false,
            },
        )
        .unwrap();
    }
    for order in [["old", "new"], ["new", "old"]] {
        let state = crate::engine::Engine::merge_space_projections(
            &e.space_projections,
            &order.map(String::from),
        );
        assert_eq!(
            state.thread_state[&("chat".into(), "root".into())],
            (Some(false), 99)
        );
    }
    e.append_at(
        "new",
        &me,
        20,
        roda_types::EventBody::ThreadStateSet {
            space: "chat".into(),
            root: "root".into(),
            follow: Some(true),
            read_up_to: 0,
            read_only: false,
        },
    )
    .unwrap();
    let merged = crate::engine::Engine::merge_space_projections(
        &e.space_projections,
        &["new".into(), "old".into()],
    );
    assert_eq!(
        merged.thread_state[&("chat".into(), "root".into())],
        (Some(true), 99),
        "later events in one log win even in the same millisecond"
    );
}

#[test]
fn read_only_updates_preserve_unfollow_and_preferences_merge_by_recency() {
    let engine = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    let mut e = engine.lock();
    e.create_account("Alice", "alice", "https://relay.example")
        .unwrap();
    let me = e.me.clone().unwrap();
    for (space, at, pinned) in [("old", 30, true), ("new", 20, false)] {
        e.append_at(
            space,
            &me,
            1,
            roda_types::EventBody::SpaceCreated {
                title: "Personal".into(),
                kind: roda_types::SpaceKind::Personal,
                privacy: roda_types::Privacy::EndToEnd,
            },
        )
        .unwrap();
        e.append_at(
            space,
            &me,
            at,
            roda_types::EventBody::ChatPrefsSet {
                space: "chat".into(),
                prefs: roda_types::ChatPrefs {
                    pinned,
                    ..Default::default()
                },
            },
        )
        .unwrap();
        e.append_at(
            space,
            &me,
            at,
            roda_types::EventBody::ThreadStateSet {
                space: "chat".into(),
                root: "root".into(),
                follow: Some(false),
                read_up_to: 1,
                read_only: false,
            },
        )
        .unwrap();
    }
    e.append_at(
        "new",
        &me,
        40,
        roda_types::EventBody::ThreadStateSet {
            space: "chat".into(),
            root: "root".into(),
            follow: Some(true),
            read_up_to: 99,
            read_only: true,
        },
    )
    .unwrap();
    for order in [["old", "new"], ["new", "old"]] {
        let state = crate::engine::Engine::merge_space_projections(
            &e.space_projections,
            &order.map(String::from),
        );
        assert!(state.chat_prefs["chat"].pinned);
        assert_eq!(
            state.thread_state[&("chat".into(), "root".into())],
            (Some(false), 99)
        );
    }
}

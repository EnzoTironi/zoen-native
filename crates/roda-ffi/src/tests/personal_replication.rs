use super::*;

#[test]
fn personal_state_is_queued_and_replayed_on_a_linked_identity() {
    let a = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    let b = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    let mut a = a.lock();
    a.create_account("Alice", "alice", "https://relay.example")
        .unwrap();
    let personal = a.personal_space().unwrap();
    assert!(a.net.synced.contains(&personal));
    assert!(!a.is_e2e(&personal));
    let me = a.me.clone().unwrap();
    a.append(
        &personal,
        &me,
        roda_types::EventBody::ThreadStateSet {
            space: "chat".into(),
            root: "root".into(),
            follow: Some(true),
            read_up_to: 3,
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
    let mut log = roda_log::SpaceLog::new(&personal);
    for p in pending {
        let event = log.sequence(p.event);
        let ordered = roda_proto::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: roda_proto::Envelope::plain(event),
        };
        assert_eq!(b.ingest(ordered), crate::sync::Ingest::Applied);
    }
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
    let pending = a.store.outbox().unwrap();
    assert_eq!(pending.len(), 2);
    assert!(matches!(
        pending[0].event.body,
        roda_types::EventBody::SpaceCreated { .. }
    ));
    assert!(matches!(
        pending[1].event.body,
        roda_types::EventBody::ThreadStateSet {
            read_up_to: 42,
            follow: Some(false),
            ..
        }
    ));
    assert_eq!(
        serde_json::to_string(&original).unwrap(),
        serde_json::to_string(&a.store.events("legacy_personal").unwrap()).unwrap()
    );
    a.unlock(Some(secret.to_vec())).unwrap();
    assert_eq!(a.store.outbox().unwrap().len(), 2);
}

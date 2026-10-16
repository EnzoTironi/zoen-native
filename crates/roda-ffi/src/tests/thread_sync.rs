//! A second device receives the signed logs, never the first device's local read metadata.
use super::*;

fn replay_to_device(source: &RodaEngine, destination: &RodaEngine) {
    let source = source.lock();
    let mut destination = destination.lock();
    for (identity, secret) in source.store.identities().unwrap() {
        destination
            .store
            .put_identity(&identity, secret.as_ref())
            .unwrap();
    }
    destination
        .store
        .set_meta("me", source.me.as_deref().unwrap())
        .unwrap();
    for space in source.store.space_ids().unwrap() {
        let have = destination.store.events(&space).unwrap().len();
        for event in source.store.events(&space).unwrap().into_iter().skip(have) {
            destination.store.append_event(&event).unwrap();
        }
    }
    destination.reload().unwrap();
}

#[test]
fn thread_follow_and_read_watermarks_replay_on_another_device() {
    let dir = std::env::temp_dir().join(format!("zoen-thread-sync-{}", roda_types::new_id("t")));
    std::fs::create_dir_all(&dir).unwrap();
    let a = RodaEngine::open(
        dir.join("a.sqlite").to_string_lossy().into(),
        "pt-BR".into(),
    )
    .unwrap();
    let b = RodaEngine::open(
        dir.join("b.sqlite").to_string_lossy().into(),
        "pt-BR".into(),
    )
    .unwrap();
    a.seed_demo_if_empty().unwrap();
    let space = a
        .spaces()
        .into_iter()
        .find(|s| s.members.len() >= 3)
        .unwrap();
    let other = space
        .members
        .iter()
        .find(|m| !m.is_me && m.kind == PersonaKind::Person)
        .unwrap();
    let root = a
        .send_message(space.id.clone(), "Vamos conversar?".into())
        .unwrap();
    a.lock()
        .reply_with(
            &space.id,
            &other.id,
            "Primeira resposta",
            None,
            &root.id,
            true,
        )
        .unwrap();
    replay_to_device(&a, &b);
    let unread =
        |e: &RodaEngine| e.thread(space.id.clone(), root.id.clone()).unwrap()[0].thread_unread;
    assert_eq!(unread(&a), 1);
    assert_eq!(unread(&b), 1);
    a.lock().store.conn().execute_batch(
        "CREATE TEMP TRIGGER refuse_thread_read BEFORE INSERT ON events BEGIN SELECT RAISE(FAIL, 'test append failure'); END;"
    ).unwrap();
    assert!(
        a.mark_thread_read(space.id.clone(), root.id.clone())
            .is_err()
    );
    assert_eq!(
        unread(&a),
        1,
        "a failed sync append must not consume the local unread state"
    );
    a.lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER refuse_thread_read")
        .unwrap();
    a.mark_thread_read(space.id.clone(), root.id.clone())
        .unwrap();
    replay_to_device(&a, &b);
    assert_eq!(
        unread(&b),
        0,
        "the Personal Space read watermark clears the second device"
    );
    assert!(
        b.lock()
            .store
            .meta(&format!("tread:{}:{}", space.id, root.id))
            .unwrap()
            .is_none()
    );
    a.lock()
        .reply_with(&space.id, &other.id, "Nova resposta", None, &root.id, true)
        .unwrap();
    replay_to_device(&a, &b);
    assert_eq!(
        unread(&b),
        1,
        "only replies newer than the watermark are unread"
    );
    a.set_thread_follow(space.id.clone(), root.id.clone(), Some(false))
        .unwrap();
    replay_to_device(&a, &b);
    assert!(!b.threads().iter().any(|t| t.root.id == root.id));
    a.set_thread_follow(space.id.clone(), root.id.clone(), Some(true))
        .unwrap();
    replay_to_device(&a, &b);
    assert!(
        b.threads()
            .iter()
            .any(|t| t.root.id == root.id && t.root.thread_unread == 1)
    );
    // An older read event from a stale device must not resurrect already read replies.
    let read = a.lock().state.thread_state[&(space.id.clone(), root.id.clone())].1;
    a.mark_thread_read(space.id.clone(), root.id.clone())
        .unwrap();
    let personal = a.lock().personal_space().unwrap();
    let me = a.me().unwrap().id;
    a.lock()
        .append(
            &personal,
            &me,
            roda_types::EventBody::ThreadStateSet {
                space: space.id.clone(),
                root: root.id.clone(),
                follow: Some(true),
                read_up_to: read,
            },
        )
        .unwrap();
    replay_to_device(&a, &b);
    assert_eq!(
        unread(&b),
        0,
        "read watermarks are monotonic across devices"
    );
}

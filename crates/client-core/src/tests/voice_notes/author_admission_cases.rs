//! Complete author admission scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn another_author_or_a_removed_author_cannot_deduplicate_an_earlier_note(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina");
    let root = e.send_message(space.id.clone(), "Root".into()).unwrap();
    let original = note(&e, &space.id, None).unwrap();
    let me = e.me().unwrap().id;
    let other = space.members.iter().find(|p| !p.is_me).unwrap().id.clone();
    let item = original.voice.as_ref().unwrap().item_id.clone();
    {
        let mut engine = e.lock();
        engine
            .reply_with(&space.id, &other, "", Some(item.clone()), &root.id, true)
            .unwrap();
        engine
            .append(
                &space.id,
                &me,
                EventBody::MemberRemoved {
                    identity: me.clone(),
                },
            )
            .unwrap();
        // Even a signed message placed in the log by an untrusted relay cannot remove
        // the former member's old note. Projection must check membership itself.
        engine
            .append(
                &space.id,
                &me,
                EventBody::MessagePosted {
                    message: types::new_id("msg")?,
                    text: String::new(),
                    attaches: Some(item),
                    reply: Some(types::ReplyRef::thread(root.id)),
                    media: None,
                    forwarded: false,
                    forwarded_author: None,
                },
            )
            .unwrap();
        engine.reload().unwrap();
    }
    assert!(e
        .timeline(space.id)
        .unwrap()
        .iter()
        .any(|x| x.id == original.id));

    Ok(())
}

pub(super) fn invalid_voice_reply_targets_do_not_publish_audio_or_create_items() {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina").id;
    let elsewhere = find_space(&e, "Zoen").id;
    let foreign_root = e.send_message(elsewhere, "Elsewhere".into()).unwrap();
    let non_message = e
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .find(|x| !matches!(x.kind, EntryKind::Message { .. }))
        .unwrap();
    let before = e.stats().events;
    let items = e.items().len();
    for target in ["missing".to_string(), foreign_root.id, non_message.id] {
        assert!(note(&e, &space, Some(target)).is_err());
        assert_eq!(e.stats().events, before);
        assert_eq!(e.items().len(), items);
    }
}

pub(super) fn only_writers_can_send_voice_notes() {
    for role in [Some(Role::Reader), None] {
        let e = seeded();
        let space = find_space(&e, "Paraty com a Marina").id;
        let me = e.me().unwrap().id;
        let body = match role {
            Some(role) => EventBody::MemberAdded {
                identity: me.clone(),
                role,
            },
            None => EventBody::MemberRemoved {
                identity: me.clone(),
            },
        };
        e.lock().append(&space, &me, body).unwrap();
        let before = e.stats().events;
        let items = e.items().len();
        assert!(matches!(
            note(&e, &space, None),
            Err(CoreError::Forbidden { .. })
        ));
        assert_eq!(e.stats().events, before);
        assert_eq!(e.items().len(), items);
    }
}

pub(super) fn voice_versions_and_reverts_are_author_only_locally_and_on_replay(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for legacy in [false, true] {
        for hidden in [false, true] {
            let e = seeded();
            let space = find_space(&e, "Paraty com a Marina");
            let me = e.me().unwrap().id;
            let other = space.members.iter().find(|p| !p.is_me).unwrap().id.clone();
            let item = if legacy {
                e.file_add(
                    space.id.clone(),
                    String::new(),
                    "Mensagem de voz.webm".into(),
                    "audio/webm".into(),
                    vec![7; 900],
                    None,
                )
                .unwrap()
                .id
            } else {
                note(&e, &space.id, None).unwrap().voice.unwrap().item_id
            };
            // Even an owner version that no longer looks like voice keeps ownership.
            e.file_new_version_typed(
                item.clone(),
                vec![8; 500],
                None,
                "Edit".into(),
                "renamed.m4a".into(),
                if hidden {
                    "application/octet-stream"
                } else {
                    "audio/mp4"
                }
                .into(),
            )
            .unwrap();
            let mut engine = e.lock();
            let current = engine.state.items[&item].current().content.clone();
            let original = engine.state.items[&item].versions.initial().content.clone();
            assert!(engine.state.writer(&space.id, &other));
            engine.me = Some(other.clone());
            let events = engine.store.event_count().unwrap();
            assert!(engine
                .file_new_version(&item, &[9; 500], None, "Replace")
                .is_err());
            assert!(engine
                .file_new_version_typed(
                    &item,
                    &[9; 500],
                    None,
                    "Replace",
                    "other.mp3",
                    "audio/mpeg"
                )
                .is_err());
            assert!(engine.restore_version(&item, 1, "Revert").is_err());
            assert_eq!(engine.store.event_count().unwrap(), events);
            for body in [
                EventBody::ItemVersioned {
                    item: item.clone(),
                    content: original,
                    note: "Replace".into(),
                },
                EventBody::ItemVersioned {
                    item: item.clone(),
                    content: current.clone(),
                    note: "Replace without voice metadata".into(),
                },
                EventBody::ItemReverted {
                    item: item.clone(),
                    to_version: 1,
                    note: "Revert".into(),
                },
            ] {
                assert!(engine.append(&space.id, &other, body.clone()).is_err());
                ingest_as(&mut engine, &space.id, &other, body)?;
                assert_eq!(engine.state.items[&item].versions.len(), 2);
                assert_eq!(engine.state.items[&item].current().content, current);
            }
            engine.reload().unwrap();
            assert_eq!(engine.state.items[&item].versions.len(), 2);
            assert_eq!(engine.file_bytes(&item, None).unwrap(), Some(vec![8; 500]));
            engine.me = Some(me);
            engine.restore_version(&item, 1, "Owner revert").unwrap();
            assert_eq!(engine.file_bytes(&item, None).unwrap(), Some(vec![7; 900]));
        }
    }
    Ok(())
}

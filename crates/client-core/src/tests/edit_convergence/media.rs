use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;
struct MediaReplay {
    local: Engine,
    remote: Engine,
    relay: SpaceLog,
    author: Author,
    owner: Signer,
}
fn media_replay(
    local_path: &str,
    remote_path: &str,
) -> Result<MediaReplay, Box<dyn std::error::Error>> {
    let owner = Signer::from_secret(&[71; 32]);
    let peer = Signer::from_secret(&[72; 32]);
    let author = Author::root(owner.clone());
    let mut local = replica(local_path, &owner, &peer);
    let mut remote = replica(remote_path, &peer, &owner);
    let mut relay = SpaceLog::new(SPACE);
    for (id, body) in [
        (
            "create-media",
            EventBody::SpaceCreated {
                title: "Media edits".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        ),
        (
            "member-media",
            EventBody::MemberAdded {
                identity: peer.id(),
                role: Role::Member,
            },
        ),
        (ROOT, post(ROOT, "Before media", None)),
    ] {
        let signed = author.sign_event(SPACE, id, AT + 1, relay.head(), body)?;
        let event = relay.sequence(signed)?;
        assert_eq!(local.ingest(frame(event)?), Ingest::Applied);
        assert_eq!(remote.ingest(frame(event)?), Ingest::Applied);
    }
    Ok(MediaReplay {
        local,
        remote,
        relay,
        author,
        owner,
    })
}
#[test]
fn media_replacement_waits_for_its_confirmed_file_and_replays_on_a_peer() -> TestResult {
    let directory = Database::new()?;
    let remote_path = directory.path("remote-media.sqlite");
    let MediaReplay {
        mut local,
        mut remote,
        mut relay,
        owner,
        ..
    } = media_replay(&directory.path("local-media.sqlite"), &remote_path)?;
    let original = local
        .timeline(SPACE)?
        .into_iter()
        .find(|row| row.id == ROOT)
        .ok_or("original")?;
    let item = local.file_add(
        SPACE,
        "photo.png",
        "photo.png",
        "image/png",
        &[1, 2, 3],
        None,
    )?;
    let file = local.file_info_at(&item, Some(1))?;
    let edit = types::MediaChange {
        mutation_id: "confirmed-media".into(),
        expected_revision: original.text_revision.ok_or("revision")?,
        text: "After media".into(),
        replacement: types::MediaReplacement {
            item_id: item.clone(),
            version: 1,
            sha256: file.sha256,
        },
    };
    let body = EventBody::MessageAnnotated {
        target: ROOT.into(),
        note: Annotation::ReplaceMedia { edit: edit.clone() },
    };
    assert!(!remote
        .state
        .media_message_dependencies_valid(SPACE, &owner.id(), &body));
    let file_event = local.store.outbox()?.into_iter().find(|pending| matches!(&pending.event.body, EventBody::ItemCreated { item: id, .. } if id == &item)).ok_or("file outbox")?.event;
    let confirmed_file = relay.sequence(file_event)?.clone();
    assert_eq!(local.ingest(frame(&confirmed_file)?), Ingest::Confirmed);
    assert_eq!(remote.ingest(frame(&confirmed_file)?), Ingest::Applied);
    assert!(remote
        .state
        .media_message_dependencies_valid(SPACE, &owner.id(), &body));
    local.annotate(SPACE, ROOT, Annotation::ReplaceMedia { edit })?;
    let pending = local
        .store
        .outbox()?
        .into_iter()
        .find(|pending| {
            matches!(
                pending.event.body,
                EventBody::MessageAnnotated {
                    note: Annotation::ReplaceMedia { .. },
                    ..
                }
            )
        })
        .ok_or("edit outbox")?
        .event;
    let confirmed = relay.sequence(pending)?.clone();
    assert_eq!(local.ingest(frame(&confirmed)?), Ingest::Confirmed);
    assert_eq!(remote.ingest(frame(&confirmed)?), Ingest::Applied);
    assert_eq!(remote.ingest(frame(&confirmed)?), Ingest::Duplicate);
    drop(remote);
    let reopened = Engine::open(&remote_path)?;
    let rows = reopened.timeline(SPACE)?;
    assert_eq!(
        rows.len(),
        1,
        "Neither replacement file nor edit adds a chat row"
    );
    assert_eq!(rows[0].id, ROOT);
    assert!(rows[0].edited);
    let EntryKind::Message {
        text,
        card: Some(card),
    } = &rows[0].kind
    else {
        return Err("media row".into());
    };
    assert_eq!(text, "After media");
    assert_eq!(card.item_id, item);
    assert_eq!(card.version, 1);
    assert_eq!(
        rows[0].text_revision.as_deref(),
        Some(event_log::content_hash_of(&confirmed).as_str())
    );
    Ok(())
}

#[test]
fn a_concurrent_caption_invalidates_queued_media_without_an_applied_receipt() -> TestResult {
    let MediaReplay {
        mut local,
        mut remote,
        mut relay,
        author,
        ..
    } = media_replay(":memory:", ":memory:")?;
    let original = local
        .timeline(SPACE)?
        .into_iter()
        .find(|row| row.id == ROOT)
        .ok_or("root")?;
    let item = local.file_add(
        SPACE,
        "photo.png",
        "photo.png",
        "image/png",
        &[1, 2, 3],
        None,
    )?;
    let file = local.file_info_at(&item, Some(1))?;
    let pending_file = local.store.outbox()?.into_iter().find(|pending| matches!(&pending.event.body, EventBody::ItemCreated { item: id, .. } if id == &item)).ok_or("file")?.event;
    let confirmed = relay.sequence(pending_file)?.clone();
    assert_eq!(local.ingest(frame(&confirmed)?), Ingest::Confirmed);
    assert_eq!(remote.ingest(frame(&confirmed)?), Ingest::Applied);
    let edit = types::MediaChange {
        mutation_id: "conflict-media".into(),
        expected_revision: original.text_revision.ok_or("revision")?,
        text: "Pending media".into(),
        replacement: types::MediaReplacement {
            item_id: item,
            version: 1,
            sha256: file.sha256,
        },
    };
    local.annotate(SPACE, ROOT, Annotation::ReplaceMedia { edit })?;
    let pending = local
        .store
        .outbox()?
        .into_iter()
        .find(|pending| {
            matches!(
                pending.event.body,
                EventBody::MessageAnnotated {
                    note: Annotation::ReplaceMedia { .. },
                    ..
                }
            )
        })
        .ok_or("media edit")?
        .event;
    let concurrent = author.sign_event(
        SPACE,
        "other-owner-device-edit",
        AT + 5,
        relay.head(),
        EventBody::MessageAnnotated {
            target: ROOT.into(),
            note: Annotation::Edit {
                text: "Concurrent caption".into(),
            },
        },
    )?;
    let confirmed_concurrent = relay.sequence(concurrent)?.clone();
    assert_eq!(local.ingest(frame(&confirmed_concurrent)?), Ingest::Applied);
    assert_eq!(
        remote.ingest(frame(&confirmed_concurrent)?),
        Ingest::Applied
    );
    let stale = relay.sequence(pending)?.clone();
    assert_eq!(local.ingest(frame(&stale)?), Ingest::Confirmed);
    assert_eq!(remote.ingest(frame(&stale)?), Ingest::Applied);
    for engine in [&local, &remote] {
        let rows = engine.timeline(SPACE)?;
        assert_eq!(rows.len(), 1);
        assert!(
            matches!(&rows[0].kind, EntryKind::Message { text, card: None } if text == "Concurrent caption"),
            "actual projection: {:?}",
            rows[0].kind
        );
        let source = engine.state.spaces[SPACE]
            .entries
            .iter()
            .find(|entry| entry.id() == ROOT)
            .ok_or("source")?;
        let crate::engine::EntryBody::Message { marks, .. } = &source.body else {
            return Err("marks".into());
        };
        assert!(
            marks.media_edit_receipt("conflict-media").is_none(),
            "Transport confirmation is not evidence that a stale edit applied"
        );
    }
    Ok(())
}

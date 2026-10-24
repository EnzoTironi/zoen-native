//! Complete album publication scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn media_contract_file_album_preserves_real_mime_order_signature_and_retry(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = temp()?;
    let path = dir.join("album.sqlite");
    let f = fixture(path.to_str().unwrap());
    let mut image = input("a");
    image.name = "photo.jpg".into();
    image.mime = "image/jpeg".into();
    let mut video = input("b");
    video.name = "video.mov".into();
    video.mime = "video/quicktime".into();
    let draft = save(
        &f,
        "0",
        &media_options(&["b", "a"], "file", "grouped"),
        vec![image, video],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    let first = sent.entries[0].media.as_ref().unwrap();
    assert_eq!(first.presentation, crate::AttachmentPresentationDto::File);
    let group_id = first.group.as_ref().unwrap().id.clone();
    for (index, (entry, expected_mime)) in sent
        .entries
        .iter()
        .zip(["video/quicktime", "image/jpeg"])
        .enumerate()
    {
        let media = entry.media.as_ref().unwrap();
        let group = media.group.as_ref().unwrap();
        assert_eq!(
            (&group.id, group.index, group.count),
            (&group_id, index as u32, 2)
        );
        let crate::EntryKind::Message {
            card: Some(card), ..
        } = &entry.kind
        else {
            panic!("missing file")
        };
        assert_eq!(
            f.engine
                .file_info_at(card.item_id.clone(), None)
                .unwrap()
                .mime,
            expected_mime
        );
    }
    let posted = f
        .engine
        .lock()
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .find(|pending| pending.event.client_id == sent.entries[0].id)
        .unwrap()
        .event;
    event_log::verify_author(&posted).unwrap();
    let mut tampered = posted.clone();
    let types::EventBody::MessagePosted {
        media: Some(media), ..
    } = &mut tampered.body
    else {
        panic!("missing signed media")
    };
    media.presentation = types::AttachmentPresentation::Media;
    assert!(event_log::verify_author(&tampered).is_err());
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let restored = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
    assert!(restored.unlock(vault).unwrap());
    let before = counts(&restored);
    let replay = restored.send_media_draft(scope, draft.revision).unwrap();
    assert_eq!(replay.entries, sent.entries);
    assert_eq!(counts(&restored), before);
    drop(restored);
    std::fs::remove_dir_all(dir).unwrap();

    Ok(())
}

pub(super) fn media_contract_transient_first_album_refusal_preserves_all_confirmed_members_after_reopen(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = temp()?;
    let sender_path = dir.join("album-sender.sqlite");
    let receiver_path = dir.join("album-receiver.sqlite");
    let mut f = fixture(sender_path.to_str().unwrap());
    media_contract_confirm_scope(&f);
    let mut relay = f.engine.lock().logs[&f.scope.space_id].clone();
    let mut receiver = crate::engine::Engine::open(receiver_path.to_str().unwrap()).unwrap();
    let frame = |event: &types::Event| protocol::Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env: protocol::Envelope::plain(event),
    };
    for event in relay.events() {
        assert_eq!(receiver.ingest(frame(event)), crate::sync::Ingest::Applied);
    }
    let draft = save(
        &f,
        "0",
        &media_options(&["a", "b"], "file", "grouped"),
        vec![input("a"), input("b")],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    media_contract_upload_all(&f);
    let expected_ids: Vec<_> = sent.entries.iter().map(|entry| entry.id.clone()).collect();
    assert_eq!(expected_ids.len(), 2);
    // Drive actual offered batches as the dispatcher does: all offers in a batch
    // precede answers. Refuse the first group member once, then sequence every
    // other original signed envelope in the offered order, including its retry.
    let mut rejected = None;
    let mut retried = false;
    let mut reopened = false;
    for _ in 0..8 {
        let offered = f.engine.lock().outbox_envelopes();
        if offered.is_empty() {
            break;
        }
        for env in offered {
            if env.client_id() == expected_ids[0] {
                if let Some(original) = rejected.as_ref() {
                    assert_eq!(
                        &env, original,
                        "retry must preserve the exact signed envelope"
                    );
                    retried = true;
                } else {
                    f.engine.lock().reject(
                        env.client_id(),
                        "injected transient relay refusal",
                        false,
                        None,
                    );
                    rejected = Some(env);
                    continue;
                }
            }
            let event = f
                .engine
                .lock()
                .store
                .outbox_get(env.client_id())
                .unwrap()
                .unwrap();
            let accepted = relay.sequence(event)?.clone();
            assert_eq!(
                f.engine.lock().ingest(frame(&accepted)),
                crate::sync::Ingest::Confirmed
            );
            assert_eq!(
                receiver.ingest(frame(&accepted)),
                crate::sync::Ingest::Applied
            );
        }
        if rejected.is_some() && !reopened {
            let Fixture {
                engine,
                vault,
                scope,
            } = f;
            drop(engine);
            let engine = Client::open(sender_path.to_string_lossy().into(), "en".into()).unwrap();
            assert!(engine.unlock(vault.clone()).unwrap());
            f = Fixture {
                engine,
                vault,
                scope,
            };
            reopened = true;
        }
    }
    assert!(
        retried,
        "the transient rejection and its retry must both execute"
    );
    assert!(
        reopened,
        "retry must use the reopened sender's durable outbox"
    );
    assert!(f.engine.lock().store.outbox().unwrap().is_empty());
    assert_eq!(
        receiver.store.event_count().unwrap(),
        relay.events().len() as u64
    );
    relay.verify().unwrap();
    drop(receiver);
    let receiver = crate::engine::Engine::open(receiver_path.to_str().unwrap()).unwrap();
    let actual_ids: Vec<_> = receiver
        .timeline(&f.scope.space_id)
        .unwrap()
        .iter()
        .filter(|entry| expected_ids.contains(&entry.id))
        .map(|entry| entry.id.clone())
        .collect();
    assert_eq!(
        actual_ids, expected_ids,
        "both confirmed album members must survive transient refusal and cold replay"
    );
    drop(receiver);
    drop(f);
    std::fs::remove_dir_all(dir).unwrap();

    Ok(())
}

pub(super) fn media_contract_single_media_transient_file_refusal_survives_cold_receiver(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = temp()?;
    let path = dir.join("single-receiver.sqlite");
    let f = fixture(":memory:");
    media_contract_confirm_scope(&f);
    let mut relay = f.engine.lock().logs[&f.scope.space_id].clone();
    let mut receiver = crate::engine::Engine::open(path.to_str().unwrap()).unwrap();
    let frame = |event: &types::Event| protocol::Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env: protocol::Envelope::plain(event),
    };
    for event in relay.events() {
        assert_eq!(receiver.ingest(frame(event)), crate::sync::Ingest::Applied);
    }
    let draft = save(
        &f,
        "0",
        &media_options(&["single"], "file", "separate"),
        vec![input("single")],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    media_contract_upload_all(&f);
    assert_eq!(sent.entries.len(), 1);
    assert!(sent.entries[0].media.as_ref().unwrap().group.is_none());
    let mut rejected = None;
    let mut retried = false;
    for _ in 0..4 {
        let offered = f.engine.lock().outbox_envelopes();
        if offered.is_empty() {
            break;
        }
        for env in offered {
            let event = f
                .engine
                .lock()
                .store
                .outbox_get(env.client_id())
                .unwrap()
                .unwrap();
            if matches!(
                &event.body,
                types::EventBody::ItemCreated {
                    kind: types::ItemKind::File,
                    ..
                }
            ) {
                if let Some(original) = rejected.as_ref() {
                    assert_eq!(
                        &env, original,
                        "single-file retry must preserve the signed envelope"
                    );
                    retried = true;
                } else {
                    assert!(matches!(
                        f.engine.lock().reject(
                            env.client_id(),
                            "transient File refusal",
                            false,
                            None
                        ),
                        crate::sync::RejectionOutcome::Retry
                    ));
                    rejected = Some(env);
                    continue;
                }
            }
            let accepted = relay.sequence(event)?.clone();
            assert_eq!(
                f.engine.lock().ingest(frame(&accepted)),
                crate::sync::Ingest::Confirmed
            );
            assert_eq!(
                receiver.ingest(frame(&accepted)),
                crate::sync::Ingest::Applied
            );
        }
    }
    assert!(retried);
    assert!(f.engine.lock().store.outbox().unwrap().is_empty());
    assert_eq!(
        receiver.store.event_count().unwrap(),
        relay.events().len() as u64
    );
    relay.verify().unwrap();
    drop(receiver);
    let receiver = crate::engine::Engine::open(path.to_str().unwrap()).unwrap();
    assert!(
        receiver
            .timeline(&f.scope.space_id)
            .unwrap()
            .iter()
            .any(|entry| entry.id == sent.entries[0].id),
        "a confirmed single media message must survive its File refusal and cold receiver replay"
    );
    drop(receiver);
    std::fs::remove_dir_all(dir).unwrap();

    Ok(())
}

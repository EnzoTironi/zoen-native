//! Complete refusals scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn media_contract_permanent_file_refusal_fails_dependents_and_releases_later_heads(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    Ok(
        for (grouping, ids) in [("separate", vec!["a"]), ("grouped", vec!["a", "b"])] {
            let dir = temp()?;
            let path = dir.join("terminal-file.sqlite");
            let f = fixture(path.to_str().unwrap());
            media_contract_confirm_scope(&f)?;
            let draft = save(
                &f,
                "0",
                &media_options(&ids, "file", grouping),
                ids.iter().map(|id| input(id)).collect(),
            );
            let sent = f
                .engine
                .send_media_draft(f.scope.clone(), draft.revision)
                .unwrap();
            media_contract_upload_all(&f);
            let later = f
                .engine
                .send_message(f.scope.space_id.clone(), "Later ordinary message".into())
                .unwrap();
            let refused = f
                .engine
                .lock()
                .store
                .outbox()
                .unwrap()
                .into_iter()
                .find(|row| {
                    matches!(
                        &row.event.body,
                        types::EventBody::ItemCreated {
                            kind: types::ItemKind::File,
                            ..
                        }
                    )
                })
                .unwrap()
                .event
                .client_id;
            assert!(f
                .engine
                .lock()
                .reject(&refused, "permanent File refusal", true, None)
                .user_failed());
            let check = |client: &Client| {
                let timeline = client.timeline(f.scope.space_id.clone()).unwrap();
                for original in &sent.entries {
                    let recovered = timeline
                        .iter()
                        .find(|entry| entry.id == original.id)
                        .unwrap();
                    assert_eq!(
                        recovered.delivery,
                        crate::Delivery::Failed,
                        "terminal File refusal must fail every dependent member"
                    );
                    assert_eq!(recovered.media, original.media);
                    assert_eq!(recovered.kind, original.kind);
                    let crate::EntryKind::Message {
                        card: Some(card), ..
                    } = &recovered.kind
                    else {
                        panic!("missing recovery card")
                    };
                    assert!(client
                        .file_bytes(card.item_id.clone(), None)
                        .unwrap()
                        .is_some());
                }
                assert!(
                    client
                        .lock()
                        .outbox_envelopes()
                        .iter()
                        .any(|env| env.client_id() == later.id),
                    "a terminal media group must not strand later ordinary heads"
                );
            };
            check(&f.engine);
            let scope = f.scope.clone();
            let vault = f.vault.clone();
            // Release the actual SQLite owner, then inspect independent cold replay.
            drop(f);
            let reopened = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
            assert!(reopened.unlock(vault).unwrap());
            let timeline = reopened.timeline(scope.space_id).unwrap();
            for original in &sent.entries {
                assert_eq!(
                    timeline
                        .iter()
                        .find(|entry| entry.id == original.id)
                        .unwrap()
                        .delivery,
                    crate::Delivery::Failed
                );
            }
            drop(reopened);
            std::fs::remove_dir_all(dir).unwrap();
        },
    )
}

pub(super) fn media_contract_refused_duplicate_creation_cannot_poison_confirmed_file_reference(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    Ok(for presentation in ["media", "file"] {
        media_contract_refused_duplicate_creation(presentation)?;
    })
}

pub(super) fn media_contract_terminal_group_refusal_is_atomic_and_exactly_scoped(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    media_contract_confirm_scope(&f)?;
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
    let later = f
        .engine
        .send_message(
            f.scope.space_id.clone(),
            "Ordinary head after refused album".into(),
        )
        .unwrap();
    let draft = save(
        &f,
        &read(&f).revision,
        &media_options(&["c", "d"], "file", "grouped"),
        vec![input("c"), input("d")],
    );
    let other_group = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    // Confirm only the first File. The member itself remains an actual transport head.
    let first_file = f
        .engine
        .lock()
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .find(|row| {
            matches!(
                &row.event.body,
                types::EventBody::ItemCreated {
                    kind: types::ItemKind::File,
                    ..
                }
            )
        })
        .unwrap()
        .event
        .client_id;
    media_contract_confirm_id(&f, &first_file)?;
    let original = f
        .engine
        .lock()
        .store
        .outbox_get(&sent.entries[0].id)
        .unwrap()
        .unwrap();
    let own = f
        .engine
        .lock()
        .author_for(&f.scope.expected_identity_id)
        .unwrap();
    let foreign = event_log::Author::root(event_log::Signer::generate()?);
    let identity_secret: [u8; 32] = f
        .vault
        .load(crate::sync::VAULT_IDENTITY.into())
        .unwrap()
        .try_into()
        .unwrap();
    let sibling = event_log::Author::device(
        &event_log::Signer::from_secret(&identity_secret),
        event_log::Signer::generate()?,
    );
    let mut unrelated = Vec::new();
    // Authenticated negative fixtures deliberately do not represent authorized local sends.
    for (author, space) in [
        (&foreign, f.scope.space_id.as_str()),
        (&own, "unrelated-space"),
        (&sibling, f.scope.space_id.as_str()),
    ] {
        let event = author.sign_event(
            space,
            &types::new_id("unrelated")?,
            original.at_ms + 10,
            None,
            original.body.clone(),
        )?;
        f.engine.lock().store.outbox_put(&event).unwrap();
        unrelated.push(event.client_id);
    }
    for drift in ["count", "presentation", "reply", "forwarded", "signature"] {
        let mut body = original.body.clone();
        let types::EventBody::MessagePosted {
            media: Some(media),
            reply,
            forwarded,
            ..
        } = &mut body
        else {
            unreachable!()
        };
        match drift {
            "count" => media.group.as_mut().unwrap().count += 1,
            "presentation" => media.presentation = types::AttachmentPresentation::Media,
            "reply" => *reply = Some(types::ReplyRef::thread("other-thread")),
            "forwarded" => *forwarded = true,
            _ => {}
        }
        let mut event = own.sign_event(
            &f.scope.space_id,
            &types::new_id("drift")?,
            original.at_ms + 11,
            None,
            body,
        )?;
        if drift == "signature" {
            event.sig.push('x');
        }
        f.engine.lock().store.outbox_put(&event).unwrap();
        unrelated.push(event.client_id);
    }
    let snapshot = || {
        f.engine
            .lock()
            .store
            .outbox()
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.event.client_id.clone(),
                    row.failed,
                    row.attempts,
                    row.last_error,
                    serde_json::to_string(&row.event).unwrap(),
                )
            })
            .collect::<Vec<_>>()
    };
    let before = snapshot();
    f.engine.lock().store.conn().execute_batch(&format!("CREATE TEMP TRIGGER fail_terminal_group BEFORE UPDATE OF failed ON outbox WHEN NEW.client_id = '{}' BEGIN SELECT RAISE(ABORT,'dependent refusal failed'); END;", sent.entries[1].id)).unwrap();
    assert!(matches!(
        f.engine
            .lock()
            .reject(&sent.entries[0].id, "permanent member refusal", true, None),
        crate::sync::RejectionOutcome::StorageFailure(_)
    ));
    assert_eq!(snapshot(), before, "dependent refusal failure must roll back every status/error/attempt and preserve signatures");
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_terminal_group")
        .unwrap();
    assert!(f
        .engine
        .lock()
        .reject(&sent.entries[0].id, "permanent member refusal", true, None)
        .user_failed());
    let rows = f.engine.lock().store.outbox().unwrap();
    for entry in &sent.entries {
        let row = rows
            .iter()
            .find(|row| row.event.client_id == entry.id)
            .unwrap();
        assert!(row.failed);
        assert_eq!(row.attempts, 1);
        assert_eq!(row.last_error.as_deref(), Some("permanent member refusal"));
        assert_eq!(
            serde_json::to_string(&row.event).unwrap(),
            before.iter().find(|row| row.0 == entry.id).unwrap().4
        );
    }
    for id in unrelated
        .iter()
        .chain(other_group.entries.iter().map(|entry| &entry.id))
        .chain(std::iter::once(&later.id))
    {
        let row = rows.iter().find(|row| &row.event.client_id == id).unwrap();
        assert!(
            !row.failed,
            "unrelated signed intent {id} must remain untouched"
        );
        assert_eq!(row.attempts, 0);
        assert!(row.last_error.is_none());
    }
    assert!(f
        .engine
        .lock()
        .outbox_envelopes()
        .iter()
        .any(|env| env.client_id() == later.id));
    let timeline = f.engine.timeline(f.scope.space_id.clone()).unwrap();
    Ok(for original in &sent.entries {
        let entry = timeline
            .iter()
            .find(|entry| entry.id == original.id)
            .unwrap();
        assert_eq!(entry.delivery, crate::Delivery::Failed);
        assert_eq!(entry.media, original.media);
    })
}

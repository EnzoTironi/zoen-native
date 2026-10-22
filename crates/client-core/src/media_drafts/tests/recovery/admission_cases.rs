//! Complete admission scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn failed_media_recovery_rejects_pending_confirmed_foreign_scope_and_revoked_access() {
    for case in [
        "pending",
        "confirmed",
        "account",
        "device",
        "space",
        "thread",
        "revoked",
        "reader",
    ] {
        let f = fixture(":memory:");
        let sent = refused(&f, &["a"], "separate");
        let revision = read(&f).revision;
        let mut scope = f.scope.clone();
        match case {
            "pending" => {
                let mut engine = f.engine.lock();
                engine.net.failed.remove(&sent.entries[0].id);
                engine
                    .store
                    .conn()
                    .execute(
                        "UPDATE outbox SET failed=0 WHERE client_id=?1",
                        [&sent.entries[0].id],
                    )
                    .unwrap();
            }
            "confirmed" => {
                let pending = f.engine.lock().store.outbox().unwrap();
                for row in pending {
                    confirm_failed_id(&f, &row.event.client_id);
                }
            }
            "account" => scope.expected_identity_id = "another".into(),
            "device" => scope.expected_device_id = "another".into(),
            "space" => scope.space_id = "another".into(),
            "thread" => scope.thread_root_id = Some(sent.entries[0].id.clone()),
            "revoked" => f
                .engine
                .lock()
                .state
                .spaces
                .get_mut(&scope.space_id)
                .unwrap()
                .members
                .clear(),
            "reader" => {
                f.engine
                    .lock()
                    .state
                    .spaces
                    .get_mut(&scope.space_id)
                    .unwrap()
                    .members
                    .iter_mut()
                    .find(|(id, _)| id == &scope.expected_identity_id)
                    .unwrap()
                    .1 = types::Role::Reader
            }
            _ => unreachable!(),
        }
        let before = public_state(&f);
        let private = rows(&f);
        assert!(
            f.engine
                .lock()
                .recover_failed_media_draft(&scope, &sent.entries[0].id, &revision, "0")
                .is_err(),
            "{case}"
        );
        assert_eq!(public_state(&f), before, "{case}");
        assert_eq!(rows(&f), private, "{case}");
    }
}

pub(super) fn failed_media_recovery_revalidates_actual_signed_outbox_device_and_complete_group() {
    for case in [
        "signature",
        "device",
        "certified_other_device",
        "missing_member",
        "duplicate_index",
        "partial_confirmation",
    ] {
        let f = fixture(":memory:");
        let sent = refused(&f, &["a", "b"], "grouped");
        {
            let engine = f.engine.lock();
            let mut event = engine
                .store
                .outbox_get(&sent.entries[1].id)
                .unwrap()
                .unwrap();
            match case {
                "signature" => event.sig = "00".repeat(64),
                "device" => event.device = Some("different-device".into()),
                "certified_other_device" => {
                    let secret: [u8; 32] = f
                        .vault
                        .load(crate::sync::VAULT_IDENTITY.into())
                        .unwrap()
                        .try_into()
                        .unwrap();
                    let other = event_log::Author::device(
                        &event_log::Signer::from_secret(&secret),
                        event_log::Signer::generate(),
                    );
                    event = other.sign_event(
                        &event.space,
                        &event.client_id,
                        event.at_ms,
                        event.seen.clone(),
                        event.body.clone(),
                    );
                    assert!(event_log::verify_author(&event).is_ok());
                    assert_eq!(event.author, f.scope.expected_identity_id);
                    assert_ne!(
                        event.device.as_deref(),
                        Some(f.scope.expected_device_id.as_str())
                    );
                }
                "missing_member" => {
                    engine.store.outbox_remove(&event.client_id).unwrap();
                }
                "duplicate_index" => {
                    if let types::EventBody::MessagePosted {
                        media: Some(media), ..
                    } = &mut event.body
                    {
                        media.group.as_mut().unwrap().index = 0;
                    }
                    event = engine
                        .author_for(&f.scope.expected_identity_id)
                        .unwrap()
                        .sign_event(
                            &event.space,
                            &event.client_id,
                            event.at_ms,
                            event.seen.clone(),
                            event.body.clone(),
                        );
                }
                "partial_confirmation" => {}
                _ => unreachable!(),
            }
            if [
                "signature",
                "device",
                "certified_other_device",
                "duplicate_index",
            ]
            .contains(&case)
            {
                engine
                    .store
                    .conn()
                    .execute(
                        "UPDATE outbox SET json=?2 WHERE client_id=?1",
                        rusqlite::params![event.client_id, serde_json::to_string(&event).unwrap()],
                    )
                    .unwrap();
            }
        }
        if case == "partial_confirmation" {
            let pending = f.engine.lock().store.outbox().unwrap();
            for row in pending.into_iter().take(2) {
                confirm_failed_id(&f, &row.event.client_id);
            }
        }
        let before = public_state(&f);
        let private = rows(&f);
        assert!(recover(&f, &sent.entries[1].id).is_err(), "{case}");
        assert_eq!(rows(&f), private, "{case}");
        assert_eq!(public_state(&f), before, "{case}");
    }
}

pub(super) fn failed_media_recovery_missing_corrupt_chunks_and_private_storage_failure_rollback_every_copy(
) {
    for case in ["missing", "corrupt", "asset_write", "record_write"] {
        let f = fixture(":memory:");
        let sent = refused(&f, &["a", "b"], "grouped");
        {
            let engine = f.engine.lock();
            match case {
                "missing" => { engine.store.conn().execute("DELETE FROM media WHERE sha256=(SELECT sha256 FROM media ORDER BY sha256 LIMIT 1)",[]).unwrap(); },
                "corrupt" => { engine.store.conn().execute("UPDATE media SET bytes=zeroblob(length(bytes)) WHERE sha256=(SELECT sha256 FROM media ORDER BY sha256 LIMIT 1)",[]).unwrap(); },
                "asset_write" => engine.store.conn().execute_batch("CREATE TRIGGER recovery_fail BEFORE INSERT ON meta WHEN NEW.key LIKE 'media.draft.asset.%' BEGIN SELECT RAISE(ABORT,'synthetic asset fault'); END;").unwrap(),
                "record_write" => engine.store.conn().execute_batch("CREATE TRIGGER recovery_fail BEFORE INSERT ON meta WHEN NEW.key LIKE 'media.draft.record.%' BEGIN SELECT RAISE(ABORT,'synthetic record fault'); END;").unwrap(),
                _ => unreachable!()
            }
        }
        let private = rows(&f);
        let before = public_state(&f);
        let count = counts(&f.engine);
        assert!(recover(&f, &sent.entries[0].id).is_err(), "{case}");
        assert_eq!(rows(&f), private, "{case}");
        assert_eq!(public_state(&f), before, "{case}");
        assert_eq!(counts(&f.engine), count, "{case}");
    }
}

pub(super) fn failed_voice_recovery_imports_only_final_render_as_a_new_source_without_invented_editor(
) {
    let f = fixture(":memory:");
    media_contract_confirm_scope(&f);
    let draft = save(
        &f,
        "0",
        &voice(true),
        vec![audio("original"), audio("edited")],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    assert!(f
        .engine
        .lock()
        .reject(&sent.entries[0].id, "synthetic voice refusal", true, None)
        .user_failed());
    let before = public_state(&f);
    let recovered = recover(&f, &sent.entries[0].id).unwrap();
    let Content::Voice {
        source_asset_id,
        editor,
        render,
        ..
    } = serde_json::from_str::<Content>(&recovered.content_json).unwrap()
    else {
        panic!()
    };
    assert_ne!(source_asset_id, "original");
    assert_ne!(source_asset_id, "edited");
    assert!(editor.is_null());
    assert!(render.is_none());
    assert!(!recovered.attempted);
    assert_eq!(recovered.assets.len(), 1);
    assert_eq!(
        asset(&f, &recovered, &source_asset_id),
        audio("edited").bytes
    );
    assert_eq!(public_state(&f), before);
    assert!(
        f.engine
            .send_media_draft(f.scope.clone(), recovered.revision)
            .is_err(),
        "new source still requires explicit authenticated preparation"
    );
}

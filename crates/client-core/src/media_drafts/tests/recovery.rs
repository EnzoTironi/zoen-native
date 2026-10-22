use super::*;

fn refused(f: &Fixture, ids: &[&str], grouping: &str) -> MediaDraftSendResult {
    media_contract_confirm_scope(f);
    let draft = save(
        f,
        "0",
        &media_options(ids, "file", grouping),
        ids.iter().map(|id| input(id)).collect(),
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    let file = f
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
        .reject(&file, "synthetic permanent refusal", true, None)
        .user_failed());
    for entry in &sent.entries {
        assert_eq!(
            f.engine
                .timeline(f.scope.space_id.clone())
                .unwrap()
                .iter()
                .find(|row| row.id == entry.id)
                .unwrap()
                .delivery,
            crate::Delivery::Failed
        );
    }
    sent
}
fn recover(f: &Fixture, id: &str) -> R<MediaDraftDto> {
    let media = read(f);
    let engine = f.engine.lock();
    let normal = engine.message_draft(&f.scope.normal())?;
    engine.recover_failed_media_draft(&f.scope, id, &media.revision, &normal.revision)
}
fn confirm_failed_id(f: &Fixture, id: &str) {
    let mut engine = f.engine.lock();
    let pending = engine.store.outbox_get(id).unwrap().unwrap();
    let mut relay = engine.logs[&pending.space].clone();
    let event = relay.sequence(pending);
    assert!(matches!(
        engine.ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(event)
        }),
        crate::sync::Ingest::Confirmed | crate::sync::Ingest::Applied
    ));
    assert!(engine.logs[&f.scope.space_id]
        .events()
        .iter()
        .any(|event| event.client_id == id));
}
fn public_state(f: &Fixture) -> serde_json::Value {
    let engine = f.engine.lock();
    let mut statement = engine
        .store
        .conn()
        .prepare("SELECT key,value FROM meta WHERE key NOT LIKE 'media.draft.%' ORDER BY key")
        .unwrap();
    let meta = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let outbox = engine.store.outbox().unwrap().into_iter().map(|row| serde_json::json!({"event":row.event,"failed":row.failed,"attempts":row.attempts,"error":row.last_error})).collect::<Vec<_>>();
    serde_json::json!({"meta":meta,"outbox":outbox,"events":engine.store.event_count().unwrap()})
}

#[test]
fn failed_media_recovery_default_ws_file_rejection_is_durable_and_explicit() {
    let dir = temp();
    let path = dir.join("default-ws-refusal.sqlite");
    let f = fixture(path.to_str().unwrap());
    media_contract_confirm_scope(&f);
    let draft = save(&f, "0", &attachments(&["a"], None), vec![input("a")]);
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    assert_eq!(sent.entries.len(), 1);
    assert_eq!(sent.entries[0].media, None);
    media_contract_upload_all(&f);
    let original = f.engine.lock().store.outbox().unwrap();
    assert_eq!(original.len(), 2, "the real send queues File and Message");
    let file = original
        .iter()
        .find(|row| {
            matches!(
                row.event.body,
                types::EventBody::ItemCreated {
                    kind: types::ItemKind::File,
                    ..
                }
            )
        })
        .unwrap();
    assert!(f
        .engine
        .lock()
        .reject(&file.event.client_id, "permanent File refusal", true, None)
        .user_failed());
    let refused = f.engine.lock().store.outbox().unwrap();
    for source in &original {
        let retained = refused
            .iter()
            .find(|row| row.event.client_id == source.event.client_id)
            .unwrap();
        assert!(
            retained.failed,
            "permanent File refusal must also fail its default-media Message"
        );
        assert_eq!(retained.event, source.event, "signed source is immutable");
    }
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let engine = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
    assert!(engine.unlock(vault.clone()).unwrap());
    let f = Fixture {
        engine,
        vault,
        scope,
    };
    let retained = f
        .engine
        .timeline(f.scope.space_id.clone())
        .unwrap()
        .into_iter()
        .find(|row| row.id == sent.entries[0].id)
        .unwrap();
    assert_eq!(retained.delivery, crate::Delivery::Failed);
    assert_eq!(retained.kind, sent.entries[0].kind);
    assert_eq!(retained.media, None);
    let before = public_state(&f);
    let recovered = recover(&f, &retained.id).unwrap();
    assert_eq!(
        public_state(&f),
        before,
        "recovery does not resend or rewrite"
    );
    let Content::Attachments { items, options, .. } =
        serde_json::from_str::<Content>(&recovered.content_json).unwrap()
    else {
        panic!("default attachment must recover as an attachment draft")
    };
    assert!(options.is_default());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].caption, "Private caption a");
    assert_ne!(items[0].mutation_id, "send-a");
    assert_eq!(asset(&f, &recovered, &items[0].asset_id), input("a").bytes);
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let reopened = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
    assert!(reopened.unlock(vault).unwrap());
    assert_eq!(reopened.media_draft(scope.clone()).unwrap(), recovered);
    assert_eq!(
        reopened
            .timeline(scope.space_id)
            .unwrap()
            .into_iter()
            .find(|row| row.id == retained.id)
            .unwrap()
            .delivery,
        crate::Delivery::Failed
    );
    drop(reopened);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_media_recovery_default_ws_refusal_is_atomic_and_keeps_other_pending_files() {
    let f = fixture(":memory:");
    media_contract_confirm_scope(&f);
    let draft = save(
        &f,
        "0",
        &attachments(&["a", "b"], None),
        vec![input("a"), input("b")],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    assert!(sent.entries.iter().all(|entry| entry.media.is_none()));
    media_contract_upload_all(&f);
    let file = f
        .engine
        .lock()
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .find(|row| {
            matches!(&row.event.body, types::EventBody::ItemCreated {
                content: types::ItemContent::File(file), ..
            } if file.name == "a.pdf")
        })
        .unwrap()
        .event;
    let before = public_state(&f);
    f.engine.lock().store.conn().execute_batch(&format!("CREATE TEMP TRIGGER fail_default_refusal BEFORE UPDATE OF failed ON outbox WHEN NEW.client_id = '{}' BEGIN SELECT RAISE(ABORT,'dependent refusal failed'); END;", sent.entries[0].id)).unwrap();
    assert!(matches!(
        f.engine
            .lock()
            .reject(&file.client_id, "permanent File refusal", true, None),
        crate::sync::RejectionOutcome::StorageFailure(_)
    ));
    assert_eq!(public_state(&f), before, "File and Message fail atomically");
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_default_refusal")
        .unwrap();
    assert!(f
        .engine
        .lock()
        .reject(&file.client_id, "permanent File refusal", true, None)
        .user_failed());
    let rows = f.engine.lock().store.outbox().unwrap();
    assert!(
        rows.iter()
            .find(|row| row.event.client_id == sent.entries[0].id)
            .unwrap()
            .failed
    );
    let unrelated = rows
        .iter()
        .find(|row| row.event.client_id == sent.entries[1].id)
        .unwrap();
    assert!(!unrelated.failed);
    assert_eq!(unrelated.attempts, 0);
    assert!(unrelated.last_error.is_none());
    assert!(
        f.engine
            .lock()
            .outbox_envelopes()
            .iter()
            .any(|env| env.client_id() != file.client_id),
        "an independent default attachment still has a publishable File head"
    );
}

#[test]
fn failed_media_recovery_copies_single_or_whole_signed_album_without_sending_or_reusing_intents() {
    for ids in [vec!["a"], vec!["a", "b", "c"]] {
        let f = fixture(":memory:");
        let sent = refused(&f, &ids, if ids.len() > 1 { "grouped" } else { "separate" });
        let before = public_state(&f);
        let counts_before = counts(&f.engine);
        let draft = recover(&f, &sent.entries.last().unwrap().id).unwrap();
        assert!(!draft.attempted);
        assert_eq!(draft.assets.len(), ids.len());
        assert_eq!(
            public_state(&f),
            before,
            "recovery cannot rewrite Failed history, receipts or outbox"
        );
        assert_eq!(
            counts(&f.engine),
            counts_before,
            "only existing encrypted journal rows may be added"
        );
        let Content::Attachments { items, options, .. } =
            serde_json::from_str::<Content>(&draft.content_json).unwrap()
        else {
            panic!()
        };
        assert_eq!(options.presentation, types::AttachmentPresentation::File);
        assert_eq!(
            options.grouping,
            if ids.len() > 1 {
                types::AttachmentGrouping::Grouped
            } else {
                types::AttachmentGrouping::Separate
            }
        );
        for ((item, descriptor), original) in items.iter().zip(&draft.assets).zip(&ids) {
            assert_ne!(item.asset_id, *original);
            assert_ne!(item.mutation_id, format!("send-{original}"));
            assert_eq!(item.caption, format!("Private caption {original}"));
            assert_eq!(asset(&f, &draft, &descriptor.id), input(original).bytes);
            assert!(rows(&f)
                .iter()
                .all(|(_, value)| !value.contains("Private media bytes")));
        }
        assert!(
            recover(&f, &sent.entries[0].id).is_err(),
            "a second recovery cannot replace an occupied draft"
        );
        assert_eq!(read(&f), draft);
        let resent = f
            .engine
            .send_media_draft(f.scope.clone(), draft.revision.clone())
            .unwrap();
        assert_eq!(resent.entries.len(), ids.len());
        for entry in &resent.entries {
            assert!(!sent.entries.iter().any(|old| old.id == entry.id));
        }
        for original in &sent.entries {
            assert_eq!(
                f.engine
                    .timeline(f.scope.space_id.clone())
                    .unwrap()
                    .iter()
                    .find(|row| row.id == original.id)
                    .unwrap()
                    .delivery,
                crate::Delivery::Failed
            );
        }
    }
}

#[test]
fn failed_media_recovery_cas_keeps_normal_and_media_drafts_and_rejects_stale_revisions() {
    for case in [
        "normal",
        "reply",
        "media",
        "normal_revision",
        "media_revision",
    ] {
        let f = fixture(":memory:");
        let sent = refused(&f, &["a"], "separate");
        let expected = read(&f).revision;
        let mut normal_revision = "0".to_string();
        match case {
            "normal" => {
                f.engine
                    .lock()
                    .save_message_draft(&f.scope.normal(), "0", "Keep this typing".into(), None)
                    .unwrap();
            }
            "reply" => {
                let root = f
                    .engine
                    .send_message(f.scope.space_id.clone(), "Root".into())
                    .unwrap();
                f.engine
                    .lock()
                    .save_message_draft(&f.scope.normal(), "0", String::new(), Some(root.id))
                    .unwrap();
            }
            "media" => {
                save(
                    &f,
                    &expected,
                    &attachments(&["new"], None),
                    vec![input("new")],
                );
            }
            "normal_revision" => normal_revision = "stale".into(),
            _ => {}
        }
        let before = public_state(&f);
        let private = rows(&f);
        let expected = if case == "media_revision" {
            "stale"
        } else {
            &expected
        };
        assert!(
            f.engine
                .lock()
                .recover_failed_media_draft(
                    &f.scope,
                    &sent.entries[0].id,
                    expected,
                    &normal_revision
                )
                .is_err(),
            "{case}"
        );
        assert_eq!(rows(&f), private, "{case}");
        assert_eq!(public_state(&f), before, "{case}");
    }
}

#[test]
fn failed_media_recovery_rejects_pending_confirmed_foreign_scope_and_revoked_access() {
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

#[test]
fn failed_media_recovery_revalidates_actual_signed_outbox_device_and_complete_group() {
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

#[test]
fn failed_media_recovery_missing_corrupt_chunks_and_private_storage_failure_rollback_every_copy() {
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

#[test]
fn failed_voice_recovery_imports_only_final_render_as_a_new_source_without_invented_editor() {
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

#[test]
fn failed_media_recovery_preserves_the_signed_inline_or_thread_destination() {
    for thread in [false, true] {
        let mut f = fixture(":memory:");
        media_contract_confirm_scope(&f);
        let root = f
            .engine
            .send_message(f.scope.space_id.clone(), "Synthetic reply root".into())
            .unwrap();
        media_contract_confirm_scope(&f);
        f.scope.thread_root_id = thread.then(|| root.id.clone());
        let mut content: serde_json::Value =
            serde_json::from_str(&media_options(&["a"], "file", "separate")).unwrap();
        content["reply_to"] = (!thread).then_some(root.id.as_str()).into();
        let draft = save(&f, "0", &content.to_string(), vec![input("a")]);
        let sent = f
            .engine
            .send_media_draft(f.scope.clone(), draft.revision)
            .unwrap();
        let file = f
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
            .reject(&file, "synthetic refusal", true, None)
            .user_failed());
        let recovered = recover(&f, &sent.entries[0].id).unwrap();
        let Content::Attachments { reply_to, .. } =
            serde_json::from_str::<Content>(&recovered.content_json).unwrap()
        else {
            panic!()
        };
        assert_eq!(reply_to, (!thread).then_some(root.id));
        assert_eq!(f.scope.thread_root_id.is_some(), thread);
    }
}

#[test]
fn failed_media_recovery_reopens_the_same_encrypted_copy_and_original_failed_history() {
    let dir = temp();
    let path = dir.join("recovery.sqlite");
    let f = fixture(path.to_str().unwrap());
    let sent = refused(&f, &["a", "b"], "grouped");
    let recovered = recover(&f, &sent.entries[0].id).unwrap();
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let reopened = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
    assert!(reopened.unlock(vault).unwrap());
    assert_eq!(reopened.media_draft(scope.clone()).unwrap(), recovered);
    for original in &sent.entries {
        assert_eq!(
            reopened
                .timeline(scope.space_id.clone())
                .unwrap()
                .iter()
                .find(|row| row.id == original.id)
                .unwrap()
                .delivery,
            crate::Delivery::Failed
        );
    }
    for (asset, id) in recovered.assets.iter().zip(["a", "b"]) {
        assert_eq!(
            reopened
                .media_draft_asset(scope.clone(), recovered.revision.clone(), asset.id.clone())
                .unwrap(),
            input(id).bytes
        );
    }
    drop(reopened);
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn failed_media_recovery_json_api_keeps_integer_wire_contract_and_rejects_extra_fields() {
    let f = fixture(":memory:");
    let sent = refused(&f, &["a"], "separate");
    let mut request = serde_json::json!({"op":"recover_failed_media_draft","scope":f.scope,"message_id":sent.entries[0].id,"expected_revision":read(&f).revision,"expected_message_revision":"0"});
    request["send"] = true.into();
    assert!(f.engine.client_request(request.to_string()).await.is_err());
    request.as_object_mut().unwrap().remove("send");
    let result: serde_json::Value =
        serde_json::from_str(&f.engine.client_request(request.to_string()).await.unwrap()).unwrap();
    assert_eq!(
        result["assets"][0]["size"],
        input("a").bytes.len().to_string()
    );
    assert_eq!(result["attempted"], false);
}

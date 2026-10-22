//! Complete custody scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn history_export_remains_only_canonical_events_while_media_draft_is_private() {
    let mut f = fixture(":memory:");
    f.scope.space_id = f
        .engine
        .create_group_with(
            "Actual encrypted history".into(),
            vec![],
            PrivacyDto::EndToEnd,
        )
        .unwrap();
    f.engine
        .send_message(f.scope.space_id.clone(), "Visible canonical history".into())
        .unwrap();
    // Use actual MLS sealing and canonical wire ordering. Plain Closed history is
    // deliberately absent from the linked-device opened-E2E history API.
    for _ in 0..12 {
        let mut engine = f.engine.lock();
        for (sha, _) in engine.pending_uploads(100) {
            engine.upload_done(&sha);
        }
        engine.mls_seal_outbox().unwrap();
        let envelopes = engine
            .outbox_envelopes()
            .into_iter()
            .filter(|env| env.space() == f.scope.space_id)
            .collect::<Vec<_>>();
        if envelopes.is_empty() {
            break;
        }
        for env in envelopes {
            let (seq, prev) = engine
                .logs
                .get(env.space())
                .map(|log| {
                    (
                        log.next_seq(),
                        log.events()
                            .last()
                            .map(|event| event.hash.clone())
                            .unwrap_or_else(|| types::GENESIS_PREV.into()),
                    )
                })
                .unwrap_or_else(|| (0, types::GENESIS_PREV.into()));
            let frame = protocol::Sequenced {
                seq,
                hash: event_log::chain_hash(env.space(), seq, &prev, &env.wire_hash()),
                prev,
                env,
            };
            assert_eq!(engine.ingest(frame), crate::sync::Ingest::Confirmed);
        }
    }
    assert!(
        !f.engine
            .lock()
            .store
            .outbox()
            .unwrap()
            .iter()
            .any(|pending| pending.event.space == f.scope.space_id),
        "encrypted history must fully confirm"
    );
    let before = serde_json::to_string(&f.engine.lock().history_bundle()).unwrap();
    assert!(
        before.contains("Visible canonical history"),
        "the history export must be nonempty"
    );
    save(&f, "0", &attachments(&["a"], None), vec![input("a")]);
    let after = serde_json::to_string(&f.engine.lock().history_bundle()).unwrap();
    assert_eq!(after, before);
    assert!(!after.contains("Private"));
}

pub(super) fn media_contract_failed_local_metadata_remains_visible_after_revocation_and_cold_recovery(
) {
    for (kind, ids) in [("separate", vec!["a"]), ("grouped", vec!["a", "b"])] {
        let dir = temp();
        let path = dir.join("refused.sqlite");
        let f = fixture(path.to_str().unwrap());
        media_contract_confirm_scope(&f);
        let draft = save(
            &f,
            "0",
            &media_options(&ids, "file", kind),
            ids.iter().map(|id| input(id)).collect(),
        );
        let sent = f
            .engine
            .send_media_draft(f.scope.clone(), draft.revision)
            .unwrap();
        assert_eq!(read(&f).content_json, "null");
        media_contract_receive(
            &f,
            types::EventBody::MemberRemoved {
                identity: f.scope.expected_identity_id.clone(),
            },
            false,
        );
        let pending = f.engine.lock().store.outbox().unwrap();
        for row in pending {
            assert!(f
                .engine
                .lock()
                .reject(&row.event.client_id, "permission denied", true, None)
                .user_failed());
        }
        let recovered_scope = f.scope.clone();
        let check = |engine: &Client| {
            let timeline = engine.timeline(recovered_scope.space_id.clone()).unwrap();
            for original in &sent.entries {
                let recovered = timeline
                    .iter()
                    .find(|entry| entry.id == original.id)
                    .expect("accepted draft must not vanish after permission refusal");
                assert_eq!(recovered.delivery, crate::Delivery::Failed);
                assert_eq!(recovered.kind, original.kind);
                assert_eq!(recovered.media, original.media);
                let crate::EntryKind::Message {
                    card: Some(card), ..
                } = &recovered.kind
                else {
                    panic!("missing retained file")
                };
                assert!(engine
                    .file_bytes(card.item_id.clone(), None)
                    .unwrap()
                    .is_some());
            }
            assert!(engine.lock().outbox_envelopes().is_empty());
        };
        check(&f.engine);
        let item = match &sent.entries[0].kind {
            crate::EntryKind::Message {
                card: Some(card), ..
            } => card.item_id.clone(),
            _ => unreachable!(),
        };
        media_contract_receive(
            &f,
            types::EventBody::MessagePosted {
                message: types::new_id("msg"),
                text: "unauthorized remote metadata".into(),
                attaches: Some(item.clone()),
                media: Some(types::MessageMedia {
                    presentation: types::AttachmentPresentation::File,
                    group: None,
                }),
                reply: None,
                forwarded: false,
            },
            true,
        );
        assert!(!f.engine.timeline(recovered_scope.space_id.clone()).unwrap().iter().any(|entry|matches!(&entry.kind,crate::EntryKind::Message{text,..}if text=="unauthorized remote metadata")));
        // Even a later writer must not publish a new claim against a file that
        // exists only in the failed local recovery overlay.
        let before = counts(&f.engine);
        {
            let mut engine = f.engine.lock();
            engine
                .state
                .spaces
                .get_mut(&f.scope.space_id)
                .unwrap()
                .members
                .push((f.scope.expected_identity_id.clone(), types::Role::Member));
            assert!(engine
                .append(
                    &f.scope.space_id,
                    &f.scope.expected_identity_id,
                    types::EventBody::MessagePosted {
                        message: types::new_id("msg"),
                        text: "new claim on failed local bytes".into(),
                        attaches: Some(item),
                        media: Some(types::MessageMedia {
                            presentation: types::AttachmentPresentation::File,
                            group: None
                        }),
                        reply: None,
                        forwarded: false,
                    }
                )
                .is_err());
        }
        assert_eq!(counts(&f.engine), before);
        let scope = f.scope.clone();
        let vault = f.vault.clone();
        drop(f);
        // Close/reopen the real database while preserving only the existing outbox.
        let restored = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
        assert!(restored.unlock(vault).unwrap());
        check(&restored);
        assert!(matches!(
            restored.media_draft(scope),
            Err(CoreError::Forbidden { .. })
        ));
        drop(restored);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

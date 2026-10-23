//! Complete authenticated echo scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn generic_ingest_refuses_colliding_ciphertext_plaintext_pruning_and_changed_header() {
    for case in ["ciphertext", "plaintext", "pruned", "timestamp"] {
        let mut fixture = Fixture::new();
        fixture.grant("echo-original-trust", None);
        let trigger = fixture.message("Original source request");
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        let prepared = fixture
            .core
            .prepare_reply(&original, "Original native result", 500)
            .unwrap();
        let committed = prepared.image().as_bytes().to_vec();
        let expected = prepared.envelope().clone();
        let author = fixture.core.engine.net.author.as_ref().unwrap();
        let envelope = match case {
            "ciphertext" => colliding_native_reply(&fixture, &prepared),
            "plaintext" => Envelope::plain(
                &fixture
                    .core
                    .engine
                    .store
                    .outbox_get(expected.client_id())
                    .unwrap()
                    .unwrap(),
            ),
            "pruned" => expected.pruned().unwrap(),
            "timestamp" => Envelope::sealed(
                author,
                SPACE,
                expected.client_id(),
                expected.at_ms() + 1,
                expected.seen().as_ref(),
                Sealed::new(
                    SealedKind::Application,
                    ::mls::SUITE_ID,
                    expected.sealed_data().unwrap().1.to_vec(),
                ),
            ),
            _ => unreachable!(),
        };
        envelope.verify().unwrap();
        assert_eq!(envelope.client_id(), expected.client_id());
        assert_eq!(envelope.author(), expected.author());
        assert_eq!(envelope.device(), expected.device());
        assert_ne!(envelope, expected);
        if case == "pruned" {
            assert_eq!(envelope.wire_hash(), expected.wire_hash());
        }
        let frame = ordered(&fixture.owner, envelope);
        if case == "ciphertext" {
            assert_eq!(fixture.owner.ingest(frame.clone()), Ingest::Applied);
            let actual = fixture.owner.logs[SPACE].events().last().unwrap();
            assert!(
                matches!(&actual.body, EventBody::MessagePosted { text, .. } if text == "Different native result")
            );
            assert!(crate::mls_opening::opened_leaf(&fixture.owner.store, actual).is_some());
        }
        let before = fixture.core.engine.logs[SPACE].head();
        assert!(
            matches!(fixture.core.ingest(vec![frame]), Err(NativeError::History)),
            "{case}"
        );
        assert!(fixture.core.image().is_err());
        assert_eq!(fixture.core.engine.logs[SPACE].head(), before);
        assert_eq!(fixture.core.engine.store.outbox_len().unwrap(), 1);
        let cold = DeviceCore::restore(
            NativeImage::decode_bounded(committed).unwrap(),
            fixture.credential(),
        )
        .unwrap();
        assert_eq!(cold.engine.outbox_envelopes(), vec![expected]);
    }
    println!("JOURNEY generic facade refuses a different genuinely opened MLS result with valid same CID/device, signed plaintext, same-hash pruned frame and signed changed timestamp before own confirmation");
}

pub(super) fn own_echo_requires_valid_original_inner_and_historical_sealing_copy() {
    for case in [
        "missing-copy",
        "bad-copy",
        "changed-inner",
        "signed-inner",
        "missing-reference",
        "missing-native-pending",
    ] {
        let mut fixture = Fixture::new();
        fixture.grant("copy-original-trust", None);
        let trigger = fixture.message("Retained source request");
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        let prepared = fixture
            .core
            .prepare_reply(&original, "Retained native output", 500)
            .unwrap();
        let committed = prepared.image().as_bytes().to_vec();
        let envelope = prepared.envelope().clone();
        let key = format!("mls.sealed:{}", envelope.client_id());
        match case {
            "missing-copy" => fixture.core.engine.store.meta_delete(&key).unwrap(),
            "bad-copy" => fixture
                .core
                .engine
                .store
                .set_meta(&key, "wrong:copy:signature")
                .unwrap(),
            "changed-inner" => {
                let mut pending = fixture
                    .core
                    .engine
                    .store
                    .outbox_get(envelope.client_id())
                    .unwrap()
                    .unwrap();
                if let EventBody::MessagePosted { text, .. } = &mut pending.body {
                    *text = "Altered retained inner view".into();
                }
                fixture
                    .core
                    .engine
                    .store
                    .outbox_remove(envelope.client_id())
                    .unwrap();
                fixture.core.engine.store.outbox_put(&pending).unwrap();
            }
            "signed-inner" => {
                let pending = fixture.core.engine.net.author.as_ref().unwrap().sign_event(
                    SPACE,
                    envelope.client_id(),
                    envelope.at_ms(),
                    envelope.seen(),
                    EventBody::MessagePosted {
                        message: prepared.message().into(),
                        text: "Another genuinely signed native inner".into(),
                        attaches: None,
                        reply: None,
                        media: None,
                        forwarded: false,
                    },
                );
                event_log::verify_author(&pending).unwrap();
                assert_ne!(
                    event_log::content_hash_of(&pending),
                    prepared.event_digest()
                );
                fixture
                    .core
                    .engine
                    .store
                    .outbox_remove(envelope.client_id())
                    .unwrap();
                fixture.core.engine.store.outbox_put(&pending).unwrap();
                fixture.core =
                    DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential())
                        .unwrap();
                let roundtrip = fixture
                    .core
                    .engine
                    .store
                    .outbox_get(envelope.client_id())
                    .unwrap()
                    .unwrap();
                assert_eq!(roundtrip, pending);
                assert_eq!(
                    fixture.core.engine.sealed_copy(&roundtrip).unwrap().1,
                    envelope
                );
            }
            "missing-reference" => fixture
                .core
                .engine
                .store
                .meta_delete(&super::reference::reference_key(envelope.client_id()))
                .unwrap(),
            "missing-native-pending" => {
                fixture.core.engine.net.pending.remove(envelope.client_id());
            }
            _ => unreachable!(),
        }
        let frame = ordered(&fixture.owner, envelope.clone());
        let before = fixture.core.engine.logs[SPACE].head();
        assert!(
            matches!(fixture.core.ingest(vec![frame]), Err(NativeError::History)),
            "{case}"
        );
        assert!(fixture.core.image().is_err());
        assert_eq!(fixture.core.engine.logs[SPACE].head(), before);
        let cold = DeviceCore::restore(
            NativeImage::decode_bounded(committed).unwrap(),
            fixture.credential(),
        )
        .unwrap();
        assert_eq!(cold.engine.outbox_envelopes(), vec![envelope]);
    }
    println!("JOURNEY missing/malformed copy, altered view, another genuinely signed inner after real store roundtrip, missing original reference and inconsistent pending map refuse before confirmation");
}

pub(super) fn exact_old_epoch_echo_survives_real_commit_cold_restore_and_replay() {
    let mut fixture = Fixture::new();
    fixture.grant("historical-trust", None);
    let trigger = fixture.message("Original history");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Historical native result", 500)
        .unwrap();
    let envelope = prepared.envelope().clone();
    let prior_epoch = fixture.core.engine.ready_epoch(SPACE).unwrap();
    let commit = fixture
        .owner
        .device()
        .unwrap()
        .commit(SPACE, &[], &Default::default())
        .unwrap();
    let commit_envelope = Envelope::sealed(
        fixture.owner.net.author.as_ref().unwrap(),
        SPACE,
        "actual-epoch-advance",
        600,
        fixture.owner.logs[SPACE].head().as_ref(),
        Sealed::new(SealedKind::Commit, ::mls::SUITE_ID, commit.commit),
    );
    let commit_frame = ordered(&fixture.owner, commit_envelope);
    assert_eq!(fixture.owner.ingest(commit_frame.clone()), Ingest::Applied);
    fixture.core.ingest(vec![commit_frame]).unwrap();
    assert_eq!(
        fixture.core.engine.ready_epoch(SPACE),
        Some(prior_epoch + 1)
    );
    assert!(fixture.core.engine.outbox_envelopes().is_empty());
    let pending = fixture
        .core
        .engine
        .store
        .outbox_get(envelope.client_id())
        .unwrap()
        .unwrap();
    assert_eq!(
        fixture.core.engine.sealed_copy(&pending).unwrap(),
        (prior_epoch, envelope.clone())
    );
    let mut cold =
        DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()).unwrap();
    let client_id = envelope.client_id().to_string();
    let reference_key = super::reference::reference_key(&client_id);
    let reference = cold.engine.store.meta(&reference_key).unwrap().unwrap();
    let accepted = ordered(&fixture.owner, envelope);
    assert_eq!(fixture.owner.ingest(accepted.clone()), Ingest::Applied);
    assert!(
        matches!(&fixture.owner.logs[SPACE].events().last().unwrap().body, EventBody::MessagePosted { text, .. } if text == "Historical native result")
    );
    let receipt = cold
        .receive_reply(&original, &client_id, vec![accepted.clone()])
        .unwrap();
    assert_eq!(receipt.original(), &original);
    assert_eq!(receipt.message(), prepared.message());
    assert_eq!(receipt.event_digest(), prepared.event_digest());
    assert_eq!(receipt.reference_digest(), prepared.reference_digest());
    assert_eq!(receipt.publish_digest(), prepared.publish_digest());
    assert_eq!(receipt.accepted().hash, accepted.hash);
    assert_eq!(cold.engine.store.outbox_len().unwrap(), 0);
    assert_eq!(
        cold.engine.store.meta(&reference_key).unwrap().as_deref(),
        Some(reference.as_str())
    );
    let event = cold.engine.logs[SPACE].events().last().unwrap();
    assert!(crate::mls_opening::own_echo(
        &cold.engine.store,
        event,
        cold.engine.net.account.as_ref().unwrap()
    ));
    assert_eq!(event_log::content_hash_of(event), prepared.event_digest());
    let mut replay = DeviceCore::restore(
        NativeImage::decode_bounded(receipt.image().as_bytes().to_vec()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    let repeated = replay
        .receive_reply(&original, &client_id, vec![accepted.clone()])
        .unwrap();
    assert_eq!(repeated.accepted(), receipt.accepted());
    assert_eq!(repeated.reference_digest(), prepared.reference_digest());
    assert_eq!(repeated.publish_digest(), prepared.publish_digest());
    assert_eq!(replay.engine.store.outbox_len().unwrap(), 0);
    assert_eq!(
        replay.retained_head(SPACE).unwrap().unwrap().hash,
        accepted.hash
    );
    println!("JOURNEY real native commit advances epoch without resealing the pending output; historical exact signed copy confirms after cold restore and unchanged replay creates no new outbox");
}

pub(super) fn later_colliding_echo_invalidates_an_entire_partially_ingested_batch() {
    let mut fixture = Fixture::new();
    fixture.grant("batch-original-trust", None);
    let trigger = fixture.message("Original batch source");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Exact batch output", 500)
        .unwrap();
    let committed = prepared.image().as_bytes().to_vec();
    let different = colliding_native_reply(&fixture, &prepared);
    let author = fixture.owner.net.author.clone().unwrap();
    let (epoch, digest) = fixture.owner.device().unwrap().checkpoint(SPACE).unwrap();
    let preceding = author.sign_event(
        SPACE,
        "preceding-native-checkpoint",
        600,
        fixture.owner.logs[SPACE].head(),
        EventBody::Checkpoint {
            upto: fixture.owner.logs[SPACE].head().unwrap(),
            epoch,
            digest,
        },
    );
    let first = ordered(&fixture.owner, Envelope::plain(&preceding));
    assert_eq!(fixture.owner.ingest(first.clone()), Ingest::Applied);
    let second = ordered(&fixture.owner, different);
    assert!(matches!(
        fixture.core.ingest(vec![first.clone(), second]),
        Err(NativeError::History)
    ));
    assert!(fixture.core.image().is_err());
    assert_eq!(
        fixture.core.engine.logs[SPACE].head().unwrap().hash,
        first.hash
    );
    let cold = DeviceCore::restore(
        NativeImage::decode_bounded(committed).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    assert_eq!(
        cold.engine.outbox_envelopes(),
        vec![prepared.envelope().clone()]
    );
    assert_ne!(cold.engine.logs[SPACE].head().unwrap().hash, first.hash);
    println!("JOURNEY a colliding own echo after an applied entry poisons the whole scratch batch; cold prior image keeps its earlier frontier and exact pending output");
}

pub(super) fn own_echo_is_context_only_and_mismatching_mls_sender_cannot_refresh() {
    let mut fixture = Fixture::new();
    fixture.grant("explicit-trust", None);
    let trigger = fixture.message("Owner's original request");
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Agent's retained own reply", 450)
        .unwrap();
    let frame = ordered(&fixture.owner, prepared.envelope().clone());
    assert_eq!(fixture.owner.ingest(frame.clone()), Ingest::Applied);
    fixture.core.ingest(vec![frame.clone()]).unwrap();
    let own = frame.hash;
    assert!(matches!(
        fixture.core.reply(SPACE, &own, 500),
        Err(NativeError::Permission)
    ));
    assert_eq!(
        fixture.core.refresh(&original, 500).unwrap().text(),
        "Owner's original request\n"
    );
    let next = fixture.message("Owner's next request");
    assert!(fixture
        .core
        .reply(SPACE, &next, 500)
        .unwrap()
        .text()
        .contains("Agent's retained own reply"));

    let agent = fixture.core.engine.net.author.as_ref().unwrap();
    let seen = fixture.owner.logs[SPACE].head();
    let inner = agent.sign_event(
        SPACE,
        "wrong-mls-leaf",
        550,
        seen.clone(),
        EventBody::MessagePosted {
            message: new_id("message"),
            text: "Outer agent differs from actual owner MLS leaf".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    );
    let plaintext = event_log::content::InnerEvent {
        content: inner.content,
        sig: inner.sig,
    }
    .encode();
    let ciphertext = fixture
        .owner
        .device()
        .unwrap()
        .seal(SPACE, &plaintext)
        .unwrap();
    let envelope = Envelope::sealed(
        agent,
        SPACE,
        "wrong-mls-leaf",
        550,
        seen.as_ref(),
        Sealed::new(SealedKind::Application, ::mls::SUITE_ID, ciphertext),
    );
    envelope.verify().unwrap();
    let frame = ordered(&fixture.owner, envelope);
    fixture.core.ingest(vec![frame]).unwrap();
    assert!(matches!(
        fixture.core.refresh(&original, 600),
        Err(NativeError::Permission)
    ));
    println!("JOURNEY own retained echo is context, never owner trigger; valid signed outer with different actual MLS sender denies refresh");
}

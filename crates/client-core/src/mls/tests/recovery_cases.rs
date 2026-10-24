//! Existing MLS assertions grouped by behavior; stable entrypoints live in tests.rs.
use super::*;

pub(super) fn a_crash_after_fork_meta_still_reports_the_fork_after_reopen(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    set_crash_after_fork_meta(false);
    let dir = std::env::temp_dir().join(format!("u18-crash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("device.sqlite");
    let _ = std::fs::remove_file(&path);
    let path = path.to_str().unwrap().to_string();
    let space = "prefix-crash";
    let device_secret = {
        let (mut engine, author, device_secret) = member(&path);
        start_space(&mut engine, &author, space)?;
        let peer = Author::root(Signer::generate()?);
        add_member(&mut engine, &author, space, &peer)?;
        let mut stored = BTreeMap::new();
        stored.insert(
            device_key(&peer),
            PendingPrefix {
                seq: 0,
                hash: "11".repeat(32),
                epoch: 1,
                digest: "ab".repeat(32),
            },
        );
        engine
            .store
            .set_meta(
                &pending_prefix_meta(space),
                &serde_json::to_string(&stored).unwrap(),
            )
            .unwrap();
        set_crash_after_fork_meta(true);
        let failed = engine.settle_pending_prefixes(space);
        set_crash_after_fork_meta(false);
        assert!(failed.is_err());
        assert_eq!(
            engine.store.meta(&fork_meta(space)).unwrap().as_deref(),
            Some(FORK)
        );
        assert!(engine
            .store
            .meta(&pending_prefix_meta(space))
            .unwrap()
            .is_some());
        device_secret
    };
    let mut engine = Engine::open(&path).unwrap();
    assert!(engine.unlock(Some(device_secret.to_vec())).unwrap());
    assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
    let _ = std::fs::remove_dir_all(&dir);

    Ok(())
}

pub(super) fn a_pending_claim_settles_through_a_pruned_stub(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let peer = Author::root(Signer::generate()?);
    Ok(
        for (space, mismatch) in [("prefix-pruned-match", false), ("prefix-pruned-fork", true)] {
            start_space(&mut engine, &author, space)?;
            add_member(&mut engine, &author, space, &peer)?;
            let frame = pruned_frame(&engine, &author, space)?;
            let claimed = if mismatch {
                "11".repeat(32)
            } else {
                frame.hash.clone()
            };
            let mut stored = BTreeMap::new();
            stored.insert(
                device_key(&peer),
                PendingPrefix {
                    seq: frame.seq,
                    hash: claimed,
                    epoch: 1,
                    digest: "ab".repeat(32),
                },
            );
            engine
                .store
                .set_meta(
                    &pending_prefix_meta(space),
                    &serde_json::to_string(&stored).unwrap(),
                )
                .unwrap();
            assert!(matches!(
                engine.ingest(frame),
                Ingest::Applied | Ingest::Confirmed
            ));
            assert!(pending(&engine, space).is_empty());
            if mismatch {
                assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
            } else {
                assert_eq!(integrity(&engine, space), None);
                assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
            }
        },
    )
}

pub(super) fn concurrent_sealed_siblings_send_until_a_member_proves_a_fork(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("u18-r6-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("device.sqlite");
    let path = path.to_str().unwrap().to_string();
    let space = "sp-sealed-siblings";
    let device_secret = {
        let (mut engine, author, device_secret) = member(&path);
        start_space(&mut engine, &author, space)?;
        let peer = Author::root(Signer::generate()?);
        add_member(&mut engine, &author, space, &peer)?;

        let first = ordered(
            &engine,
            Envelope::sealed(
                &peer,
                space,
                "peer-only",
                1_700_000_000_050,
                seen(&engine, space).as_ref(),
                Sealed::new(SealedKind::Application, SUITE_ID, vec![1]),
            ),
        )?;
        assert_eq!(engine.ingest(first), Ingest::Applied);
        assert_eq!(integrity(&engine, space), None);

        let head = seen(&engine, space).unwrap();
        let (content, sig) = author.sign_sealed(
            space,
            "own-successor",
            1_700_000_000_100,
            Some(&head),
            Sealed::new(SealedKind::Application, SUITE_ID, vec![2, 2]),
        );
        let own = event_from_content(
            content,
            sig,
            author.cert.clone(),
            0,
            String::new(),
            String::new(),
        )
        .unwrap();
        engine.store.outbox_put(&own).unwrap();
        let before = engine.store.outbox().unwrap().len();
        assert_eq!(before, 1);
        assert!(!engine.store.outbox().unwrap()[0].failed);

        let frame = ordered(
            &engine,
            Envelope::sealed(
                &peer,
                space,
                "peer-successor",
                1_700_000_000_200,
                Some(&head),
                Sealed::new(SealedKind::Application, SUITE_ID, vec![3, 3, 3]),
            ),
        )?;
        let got = engine.ingest(frame.clone());
        assert!(
            matches!(got, Ingest::Applied),
            "canonical sibling was not stored: {got:?}"
        );
        let stored = engine.logs[space].events().last().unwrap();
        assert_eq!(stored.hash, frame.hash);
        assert_eq!(stored.author, peer.identity);
        assert_eq!(stored.seen.as_ref(), Some(&head));

        assert_eq!(integrity(&engine, space), None);
        assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
        let sent = engine.send_message(space, "concurrent send").unwrap();
        assert!(matches!(
            sent.kind,
            crate::dto::EntryKind::Message { ref text, .. } if text == "concurrent send"
        ));
        assert_eq!(engine.store.outbox().unwrap().len(), before + 1);
        assert_eq!(
            engine.store.outbox_get("own-successor").unwrap(),
            Some(own.clone())
        );
        let published = engine.outbox_envelopes();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].client_id(), "own-successor");
        engine.reproject_approval_overlay().unwrap();
        assert_eq!(integrity(&engine, space), None);
        assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);

        let next = engine.next_seq(space);
        let diverged = types::Seen {
            seq: head.seq,
            hash: "00".repeat(32),
        };
        assert_ne!(diverged.hash, head.hash);
        let conflicting = ordered(
            &engine,
            Envelope::sealed(
                &peer,
                space,
                "peer-conflicting-prefix",
                1_700_000_000_300,
                Some(&diverged),
                Sealed::new(SealedKind::Application, SUITE_ID, vec![4, 4, 4, 4]),
            ),
        )?;
        conflicting.env.verify().unwrap();
        let got = engine.ingest(conflicting);
        assert!(matches!(got, Ingest::Invalid(_)), "{got:?}");
        assert_eq!(engine.next_seq(space), next);
        assert!(engine.logs[space]
            .events()
            .iter()
            .all(|event| event.client_id != "peer-conflicting-prefix"));
        let before = engine.store.outbox().unwrap().len();
        let phrase = integrity(&engine, space);
        let sent = engine.send_message(space, "again");
        let after = engine.store.outbox().unwrap().len();
        let published = engine.outbox_envelopes().len();
        assert_eq!(
            (phrase.as_deref(), sent.is_err(), after, published),
            (Some(FORK), true, before, 0),
            "{sent:?}"
        );
        assert_eq!(
            engine.store.outbox().unwrap()[0].event.client_id,
            "own-successor"
        );
        assert!(!engine.store.outbox().unwrap()[0].failed);
        assert_eq!(
            engine.store.meta(&fork_meta(space)).unwrap().as_deref(),
            Some(FORK)
        );
        let queued = engine.store.outbox().unwrap().len();
        engine.reproject_approval_overlay().unwrap();
        let replayed = integrity(&engine, space);
        let resent = engine.send_message(space, "after replay");
        let grown = engine.store.outbox().unwrap().len();
        let refused = match &resent {
            Err(crate::CoreError::Invalid { reason }) => reason.clone(),
            other => format!("{other:?}"),
        };
        assert_eq!(
            (replayed.as_deref(), refused.as_str(), grown),
            (Some(FORK), FORK, queued),
            "{resent:?}"
        );
        device_secret
    };

    let mut engine = Engine::open(&path).unwrap();
    assert!(engine.unlock(Some(device_secret.to_vec())).unwrap());
    assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
    let before = engine.store.outbox().unwrap().len();
    let sent = engine.send_message(space, "after reopen");
    assert!(sent.is_err(), "{sent:?}");
    assert_eq!(sent.unwrap_err().to_string(), FORK);
    assert_eq!(engine.store.outbox().unwrap().len(), before);
    assert!(engine.outbox_envelopes().is_empty());
    let _ = std::fs::remove_dir_all(&dir);

    Ok(())
}

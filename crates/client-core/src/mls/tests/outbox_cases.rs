//! Existing MLS assertions grouped by behavior; stable entrypoints live in tests.rs.
use super::*;

pub(super) fn an_obsolete_stale_rejection_cannot_release_a_current_epoch_block(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::open(":memory:").unwrap();
    engine
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    let author = engine.net.author.clone().unwrap();
    let space = "sp_current_stale_epoch";
    let mut log = SpaceLog::new(space);
    let genesis = log
        .sequence(author.sign_event(
            space,
            "created",
            1,
            None,
            EventBody::SpaceCreated {
                title: "Current epoch".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        )?)?
        .clone();
    assert_eq!(
        engine.ingest(Sequenced {
            seq: genesis.seq,
            prev: genesis.prev.clone(),
            hash: genesis.hash.clone(),
            env: Envelope::plain(&genesis),
        }),
        Ingest::Applied
    );
    engine.create_mls_group(space).unwrap();
    let commit = engine
        .device()
        .unwrap()
        .commit(space, &[], &Default::default())
        .unwrap();
    let env = Envelope::sealed(
        &author,
        space,
        "advance",
        2,
        log.head().as_ref(),
        Sealed::new(SealedKind::Commit, SUITE_ID, commit.commit),
    );
    assert_eq!(
        engine.ingest(Sequenced {
            seq: 1,
            prev: genesis.hash.clone(),
            hash: chain_hash(space, 1, &genesis.hash, &env.wire_hash())?,
            env,
        }),
        Ingest::Applied
    );
    assert_eq!(engine.mls_status(space).unwrap().0, 1);
    let message = engine
        .append_synced(
            space,
            &author,
            3,
            EventBody::MessagePosted {
                message: "pending".into(),
                text: "Waiting for the next commit".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )
        .unwrap();
    assert!(engine.mls_seal_outbox().unwrap());
    let copy = engine
        .store
        .meta(&sealed_meta(&message.client_id))
        .unwrap()
        .unwrap();
    assert_eq!(engine.outbox_envelopes().len(), 1);
    engine.reject(&message.client_id, protocol::STALE_SEAL, true, Some(1));
    assert!(
        engine.outbox_envelopes().is_empty(),
        "a rejected current epoch waits for its missing commit"
    );
    engine.reject(&message.client_id, protocol::STALE_SEAL, true, Some(0));
    assert!(
        engine.outbox_envelopes().is_empty(),
        "an obsolete rejection must not overwrite the current epoch's block"
    );
    assert_eq!(engine.net.mls.behind.get(space), Some(&1));
    assert_eq!(
        engine.store.meta(&sealed_meta(&message.client_id)).unwrap(),
        Some(copy)
    );
    assert_eq!(
        engine.store.outbox_get(&message.client_id).unwrap(),
        Some(message)
    );
    engine.logs[space].verify().unwrap();
    Ok(())
}

pub(super) fn rejected_outer_signature_keeps_the_own_event_pending(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::open(":memory:").unwrap();
    engine
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    let author = engine.net.author.clone().unwrap();
    let space = "sp_own_outer_auth";
    let mut log = SpaceLog::new(space);
    let genesis = log
        .sequence(author.sign_event(
            space,
            "created",
            1,
            None,
            EventBody::SpaceCreated {
                title: "Outer signature".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        )?)?
        .clone();
    assert_eq!(
        engine.ingest(Sequenced {
            seq: genesis.seq,
            prev: genesis.prev.clone(),
            hash: genesis.hash.clone(),
            env: Envelope::plain(&genesis),
        }),
        Ingest::Applied
    );
    engine.create_mls_group(space).unwrap();
    let own = author.sign_event(
        space,
        "pending",
        2,
        log.head(),
        EventBody::MessagePosted {
            message: "message".into(),
            text: "Still pending".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    )?;
    engine.store.outbox_put(&own).unwrap();
    engine
        .net
        .pending
        .insert(own.client_id.clone(), space.into());
    let mut env = Envelope::sealed(
        &author,
        space,
        &own.client_id,
        own.at_ms,
        own.seen.as_ref(),
        Sealed::new(SealedKind::Application, SUITE_ID, vec![1, 2, 3]),
    );
    let hash = chain_hash(space, 1, &genesis.hash, &env.wire_hash())?;
    env.sig = "00".repeat(64);
    let group_before = engine.mls_status(space).unwrap();
    let count_before = engine.store.event_count().unwrap();
    assert!(matches!(
        engine.ingest(Sequenced {
            seq: 1,
            prev: genesis.hash,
            hash,
            env,
        }),
        Ingest::Invalid(_)
    ));
    assert_eq!(
        engine.store.outbox_get(&own.client_id).unwrap(),
        Some(own.clone())
    );
    assert!(engine.net.pending.contains_key(&own.client_id));
    assert_eq!(engine.logs[space].next_seq(), 1);
    assert_eq!(engine.store.event_count().unwrap(), count_before);
    assert_eq!(engine.mls_status(space), Some(group_before));
    Ok(())
}

pub(super) fn rejected_pruned_history_preserves_the_mls_group(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::open(":memory:").unwrap();
    engine
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    let author: Author = engine.net.author.clone().unwrap();
    let space = "sp_pruned_auth";
    let mut log = SpaceLog::new(space);
    let genesis = log
        .sequence(author.sign_event(
            space,
            "created",
            1,
            None,
            EventBody::SpaceCreated {
                title: "Verified history".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        )?)?
        .clone();
    assert_eq!(
        engine.ingest(Sequenced {
            seq: genesis.seq,
            prev: genesis.prev.clone(),
            hash: genesis.hash.clone(),
            env: Envelope::plain(&genesis),
        }),
        Ingest::Applied,
    );
    engine.create_mls_group(space).unwrap();
    let group_before = engine.mls_status(space).unwrap();
    let full = Envelope::sealed(
        &author,
        space,
        "missed-ciphertext",
        2,
        log.head().as_ref(),
        Sealed::new(SealedKind::Application, SUITE_ID, vec![1, 2, 3]),
    );
    let valid = Sequenced {
        seq: 1,
        prev: genesis.hash.clone(),
        hash: chain_hash(space, 1, &genesis.hash, &full.wire_hash())?,
        env: full.pruned().unwrap(),
    };
    let mut bad_signature = valid.clone();
    bad_signature.env.sig = "00".repeat(64);
    let mut bad_chain = valid.clone();
    bad_chain.hash = "00".repeat(32);
    let stored_before = engine.store.event_count().unwrap();
    for invalid in [bad_signature, bad_chain] {
        assert!(matches!(engine.ingest(invalid), Ingest::Invalid(_)));
        assert_eq!(engine.mls_status(space), Some(group_before.clone()));
        assert!(!engine.net.mls.rejoin.contains(space));
        assert_eq!(engine.store.event_count().unwrap(), stored_before);
        assert_eq!(engine.logs[space].next_seq(), 1);
    }
    assert_eq!(engine.ingest(valid), Ingest::Applied);
    assert!(engine.mls_status(space).is_none());
    assert!(engine.net.mls.rejoin.contains(space));
    assert_eq!(engine.store.event_count().unwrap(), stored_before + 1);
    engine.logs[space].verify().unwrap();
    Ok(())
}

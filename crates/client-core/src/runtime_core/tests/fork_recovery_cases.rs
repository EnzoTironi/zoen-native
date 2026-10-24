//! Complete fork recovery scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn failed_opening_record_rolls_back_log_and_ratchet_and_discards_workspace(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.grant("retained-trust", None);
    let trigger = fixture.message("Known original request")?;
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let before = fixture.core.image().unwrap();
    let count = fixture.core.engine.store.event_count().unwrap();
    fixture.core.engine.store.conn().execute_batch(
        "CREATE TRIGGER fail_opening BEFORE INSERT ON meta WHEN NEW.key LIKE 'runtime.mls-opening.v1/%' BEGIN SELECT RAISE(ABORT,'injected opening write failure'); END;"
    ).unwrap();
    let author = fixture.owner.net.author.clone().unwrap();
    fixture
        .owner
        .append_synced(
            SPACE,
            &author,
            450,
            EventBody::MessagePosted {
                message: new_id("message")?,
                text: "Open atomically".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )
        .unwrap();
    fixture.owner.mls_seal_outbox().unwrap();
    let frame = ordered(&fixture.owner, fixture.owner.outbox_envelopes().remove(0));
    assert!(matches!(
        fixture.core.ingest(vec![frame.clone()]),
        Err(NativeError::History)
    ));
    assert_eq!(fixture.core.engine.store.event_count().unwrap(), count);
    assert!(fixture
        .core
        .engine
        .store
        .meta(&format!("runtime.mls-opening.v1/{}", frame.hash))
        .unwrap()
        .is_none());
    assert!(matches!(fixture.core.image(), Err(NativeError::History)));
    assert!(matches!(
        fixture.core.refresh(&original, 500),
        Err(NativeError::History)
    ));
    let mut restored = DeviceCore::restore(before, fixture.credential()).unwrap();
    restored.ingest(vec![frame.clone()]).unwrap();
    assert!(restored.reply(SPACE, &frame.hash, 500).is_ok());
    println!("JOURNEY real opening/log/ratchet rollback on receipt failure; workspace discarded; same message opens from retained image");

    Ok(())
}

pub(super) fn conflicting_actual_member_checkpoint_parks_live_and_cold_core(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.grant("explicit-trust", None);
    let trigger = fixture.message("Original request")?;
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    fixture.clear(EventBody::Checkpoint {
        upto: fixture.owner.logs[SPACE].head().unwrap(),
        epoch: fixture.core.engine.device().unwrap().epoch(SPACE).unwrap(),
        digest: "00".repeat(32),
    })?;
    assert!(matches!(
        fixture.core.refresh(&original, 500),
        Err(NativeError::NotReady)
    ));
    assert!(matches!(
        DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()),
        Err(NativeError::Credential)
    ));
    println!("JOURNEY signed member checkpoint with conflicting group digest parks live core and refuses cold restore");

    Ok(())
}

pub(super) fn diverged_member_checkpoint_parks_live_and_cold_core(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.grant("explicit-trust", None);
    let trigger = fixture.message("Original request")?;
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let author = fixture.owner.net.author.clone().unwrap();
    let shared = fixture.owner.logs[SPACE].head().unwrap();
    assert_eq!(fixture.core.engine.logs[SPACE].head(), Some(shared.clone()));
    let epoch = fixture.core.engine.device().unwrap().epoch(SPACE).unwrap();
    let digest = fixture
        .core
        .engine
        .store
        .meta(&format!("mls.digest:{SPACE}:{epoch}"))
        .unwrap()
        .unwrap_or_else(|| "aa".repeat(32));
    let on_owner = author.sign_event(
        SPACE,
        "diverge-owner",
        460,
        Some(shared.clone()),
        EventBody::Checkpoint {
            upto: shared.clone(),
            epoch,
            digest: digest.clone(),
        },
    )?;
    let on_core = author.sign_event(
        SPACE,
        "diverge-core",
        461,
        Some(shared.clone()),
        EventBody::Checkpoint {
            upto: shared,
            epoch,
            digest,
        },
    )?;
    assert_eq!(
        fixture
            .owner
            .ingest(ordered(&fixture.owner, Envelope::plain(&on_owner))),
        Ingest::Applied
    );
    assert_eq!(
        fixture
            .core
            .engine
            .ingest(ordered(&fixture.core.engine, Envelope::plain(&on_core),)),
        Ingest::Applied
    );
    assert!(fixture.core.engine.state.spaces[SPACE]
        .integrity_error
        .is_none());
    let owner_head = fixture.owner.logs[SPACE].head().unwrap();
    let core_at_owner_seq = fixture.core.engine.logs[SPACE].events()[owner_head.seq as usize]
        .hash
        .clone();
    assert_ne!(owner_head.hash, core_at_owner_seq);
    let checkpoint = author.sign_event(
        SPACE,
        "fork-checkpoint",
        470,
        Some(owner_head),
        EventBody::Checkpoint {
            upto: fixture.owner.logs[SPACE].head().unwrap(),
            epoch: 1,
            digest: "ab".repeat(32),
        },
    )?;
    let got = fixture
        .core
        .engine
        .ingest(ordered(&fixture.core.engine, Envelope::plain(&checkpoint)));
    assert!(matches!(got, Ingest::Invalid(_)));
    assert!(fixture.core.engine.logs[SPACE]
        .events()
        .iter()
        .all(|event| event.client_id != "fork-checkpoint"));
    assert_eq!(
        fixture.core.engine.state.spaces[SPACE]
            .integrity_error
            .as_deref(),
        Some("Members see different groups in this chat (the server may have forked it).")
    );
    assert!(matches!(
        fixture.core.refresh(&original, 500),
        Err(NativeError::NotReady)
    ));
    assert!(matches!(
        DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()),
        Err(NativeError::Credential)
    ));
    println!(
        "JOURNEY honest checkpoint from the other fork parks live send and refuses cold restore"
    );

    Ok(())
}

pub(super) fn sealed_member_seen_ahead_marks_the_fork_and_parks_send(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.grant("sealed-fork-trust", None);
    let trigger = fixture.message("Original request")?;
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let author = fixture.owner.net.author.clone().unwrap();
    let ahead = Seen {
        seq: 9_000,
        hash: "12".repeat(32),
    };
    let next = fixture.core.engine.next_seq(SPACE);
    let mut late = ordered(
        &fixture.core.engine,
        sealed_frame(&author, "sealed-gap", Some(&ahead)),
    );
    late.seq = next + 3;
    let gapped = fixture.core.engine.ingest(late);
    assert!(
        matches!(gapped, Ingest::Gap { next: n } if n == next),
        "{gapped:?}"
    );
    assert!(fixture.core.engine.state.spaces[SPACE]
        .integrity_error
        .is_none());

    let stranger = event_log::Author::root(event_log::Signer::generate()?);
    let refused = fixture.core.engine.ingest(ordered(
        &fixture.core.engine,
        sealed_frame(&stranger, "sealed-stranger", Some(&ahead)),
    ));
    assert!(matches!(refused, Ingest::Invalid(_)), "{refused:?}");
    let mut forged = sealed_frame(&author, "sealed-forged", Some(&ahead));
    forged.sig = "00".repeat(64);
    let forged = fixture
        .core
        .engine
        .ingest(ordered(&fixture.core.engine, forged));
    assert!(matches!(forged, Ingest::Invalid(_)), "{forged:?}");
    assert!(fixture.core.engine.state.spaces[SPACE]
        .integrity_error
        .is_none());
    assert_eq!(
        fixture
            .core
            .engine
            .store
            .meta(&format!("mls.fork:{SPACE}"))
            .unwrap(),
        None
    );
    assert_eq!(fixture.core.engine.next_seq(SPACE), next);

    let got = fixture.core.engine.ingest(ordered(
        &fixture.core.engine,
        sealed_frame(&author, "sealed-ahead", Some(&ahead)),
    ));
    assert!(matches!(got, Ingest::Invalid(_)), "{got:?}");
    assert!(fixture.core.engine.logs[SPACE]
        .events()
        .iter()
        .all(|event| event.client_id != "sealed-ahead"));
    assert_eq!(fixture.core.engine.next_seq(SPACE), next);
    assert_eq!(
        fixture.core.engine.state.spaces[SPACE]
            .integrity_error
            .as_deref(),
        Some("Members see different groups in this chat (the server may have forked it)."),
        "sealed member seen ahead did not mark the fork"
    );
    assert_eq!(
        fixture
            .core
            .engine
            .store
            .meta(&format!("mls.fork:{SPACE}"))
            .unwrap()
            .as_deref(),
        Some("Members see different groups in this chat (the server may have forked it).")
    );
    assert!(matches!(
        fixture.core.refresh(&original, 500),
        Err(NativeError::NotReady)
    ));

    Ok(())
}

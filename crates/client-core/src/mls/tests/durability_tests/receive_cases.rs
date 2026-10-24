//! Existing SQLite crash/confirmation assertions, invoked by stable test entrypoints.
use super::*;

pub(super) fn plain_receive_scope_failure_rolls_back_event_and_delivered_progress_across_reopen(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let database = Database::new()?;
    let path = database.path("receiver.sqlite");
    let (mut receiver, _, secret) = member(&path);
    let (_, peer, _) = member(":memory:");
    let space = "plain-receive-transaction";
    closed_space(&mut receiver, &peer, space);
    let pending = peer.sign_event(
        space,
        "peer-message",
        10,
        seen(&receiver, space),
        post("Retain me only after the complete receipt commits"),
    )?;
    let frame = ordered(&receiver, Envelope::plain(&pending));
    let before = receiver.store.event_count().unwrap();
    let delivered_before = receiver.delivered_to_announce(space);
    receiver
        .store
        .conn()
        .execute_batch(
            "CREATE TEMP TRIGGER fail_scope BEFORE INSERT ON synced_spaces
         BEGIN SELECT RAISE(ABORT, 'injected catch-up scope failure'); END;",
        )
        .unwrap();
    assert!(
        matches!(receiver.ingest(frame.clone()), Ingest::Invalid(reason)
        if reason.contains("injected catch-up scope failure"))
    );
    assert!(receiver.store.conn().is_autocommit());
    assert_eq!(receiver.store.event_count().unwrap(), before);
    assert_eq!(receiver.next_seq(space), frame.seq);
    assert_eq!(receiver.delivered_to_announce(space), delivered_before);
    assert_eq!(message_count(&receiver, space, "peer-message"), 0);
    drop(receiver);

    let mut receiver = reopen(&path, &secret);
    assert_eq!(receiver.store.event_count().unwrap(), before);
    assert_eq!(receiver.next_seq(space), frame.seq);
    assert_eq!(receiver.delivered_to_announce(space), delivered_before);
    assert_eq!(receiver.ingest(frame.clone()), Ingest::Applied);
    assert_eq!(receiver.delivered_to_announce(space), Some(frame.seq));
    assert_eq!(receiver.ingest(frame.clone()), Ingest::Duplicate);
    assert_eq!(receiver.store.event_count().unwrap(), before + 1);
    assert_eq!(message_count(&receiver, space, "peer-message"), 1);
    receiver.logs[space].verify().unwrap();
    drop(receiver);

    let receiver = reopen(&path, &secret);
    assert_eq!(receiver.next_seq(space), frame.seq + 1);
    assert_eq!(receiver.delivered_to_announce(space), Some(frame.seq));

    Ok(())
}

pub(super) fn failed_ciphertext_retention_rolls_back_sender_ratchet_before_cold_retry(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let database = Database::new()?;
    let space = "sealed-send-retention";
    let (mut owner, secret, mut peer, _) = encrypted_pair(&database, space);
    let message = owner
        .send_message(space, "One retained ciphertext per generation")
        .unwrap();
    let queued = owner
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .find(|pending| {
            pending.event.space == space
                && matches!(pending.event.body, EventBody::MessagePosted { .. })
        })
        .unwrap()
        .event;
    let before = provider_rows(&owner);
    owner
        .store
        .conn()
        .execute_batch(
            "CREATE TEMP TRIGGER fail_ciphertext_retention BEFORE INSERT ON meta
         WHEN NEW.key LIKE 'mls.sealed:%'
         BEGIN SELECT RAISE(ABORT, 'injected ciphertext retention failure'); END;",
        )
        .unwrap();
    for _ in 0..2 {
        assert!(owner.mls_seal_outbox().is_err());
        assert!(owner.store.conn().is_autocommit());
        assert_eq!(
            provider_rows(&owner),
            before,
            "no ratchet generation was consumed"
        );
        assert!(owner
            .store
            .meta(&sealed_meta(&queued.client_id))
            .unwrap()
            .is_none());
        assert_eq!(
            owner.store.outbox_get(&queued.client_id).unwrap(),
            Some(queued.clone())
        );
    }
    drop(owner);

    let mut owner = reopen(&database.path("owner.sqlite"), &secret);
    assert_eq!(provider_rows(&owner), before);
    assert!(owner.mls_seal_outbox().unwrap());
    let retained = owner.sealed_copy(&queued).unwrap().1;
    assert!(
        !owner.mls_seal_outbox().unwrap(),
        "retry reuses the exact ciphertext"
    );
    assert_eq!(owner.sealed_copy(&queued).unwrap().1, retained);
    // The transport may have written this ciphertext and lost its response. Its
    // volatile sent set cannot remove the durable outbox or consume another key.
    let sent = [(queued.client_id.clone(), owner.ready_epoch(space))]
        .into_iter()
        .collect();
    assert!(!owner
        .outbox_envelopes_except(&sent)
        .iter()
        .any(|envelope| envelope.client_id() == queued.client_id));
    drop(owner);
    let mut owner = reopen(&database.path("owner.sqlite"), &secret);
    assert_eq!(owner.sealed_copy(&queued).unwrap().1, retained);
    assert!(owner.outbox_envelopes().contains(&retained));
    assert!(!owner.mls_seal_outbox().unwrap());
    let frame = ordered(&owner, retained);
    assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
    assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    assert_eq!(peer.ingest(frame), Ingest::Duplicate);
    assert_eq!(message_count(&peer, space, &message.id), 1);

    Ok(())
}

pub(super) fn sealed_receive_record_failure_rolls_back_ratchet_event_and_readable_receipt(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    assert_sealed_receive_rollback(
        "CREATE TEMP TRIGGER fail_opening_record BEFORE INSERT ON meta
         WHEN NEW.key LIKE 'runtime.mls-opening.v1/%'
         BEGIN SELECT RAISE(ABORT, 'injected opened receipt failure'); END;",
        "injected opened receipt failure",
    )?;

    Ok(())
}

pub(super) fn sealed_receive_scope_failure_rolls_back_ratchet_and_retained_opening(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    assert_sealed_receive_rollback(
        "CREATE TEMP TRIGGER fail_scope BEFORE INSERT ON synced_spaces
         BEGIN SELECT RAISE(ABORT, 'injected sealed scope failure'); END;",
        "injected sealed scope failure",
    )?;

    Ok(())
}

pub(super) fn pruned_receive_scope_failure_preserves_the_group_until_the_whole_receipt_commits(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let database = Database::new()?;
    let path = database.path("pruned-receiver.sqlite");
    let (mut receiver, author, secret) = member(&path);
    let space = "pruned-receive-scope";
    start_space(&mut receiver, &author, space);
    receiver.create_mls_group(space).unwrap();
    let frame = pruned_frame(&receiver, &author, space);
    let before = provider_rows(&receiver);
    let count_before = receiver.store.event_count().unwrap();
    receiver
        .store
        .conn()
        .execute_batch(
            "CREATE TEMP TRIGGER fail_pruned_scope BEFORE INSERT ON synced_spaces
         BEGIN SELECT RAISE(ABORT, 'injected pruned scope failure'); END;",
        )
        .unwrap();
    assert!(
        matches!(receiver.ingest(frame.clone()), Ingest::Invalid(reason)
        if reason.contains("injected pruned scope failure"))
    );
    assert!(receiver.store.conn().is_autocommit());
    assert_eq!(
        provider_rows(&receiver),
        before,
        "a rolled-back stub cannot forget the group"
    );
    assert!(receiver.device().unwrap().has_group(space));
    assert_eq!(receiver.store.event_count().unwrap(), count_before);
    assert_eq!(receiver.next_seq(space), frame.seq);
    assert!(!receiver.net.mls.rejoin.contains(space));
    drop(receiver);

    let mut receiver = reopen(&path, &secret);
    assert_eq!(provider_rows(&receiver), before);
    assert!(receiver.device().unwrap().has_group(space));
    assert_eq!(receiver.next_seq(space), frame.seq);
    assert_eq!(receiver.ingest(frame.clone()), Ingest::Applied);
    assert!(!receiver.device().unwrap().has_group(space));
    assert!(receiver.net.mls.rejoin.contains(space));
    assert_eq!(receiver.ingest(frame.clone()), Ingest::Duplicate);
    assert_eq!(receiver.store.event_count().unwrap(), count_before + 1);
    drop(receiver);

    let receiver = reopen(&path, &secret);
    assert!(!receiver.device().unwrap().has_group(space));
    assert_eq!(receiver.next_seq(space), frame.seq + 1);
    assert_eq!(receiver.store.event_count().unwrap(), count_before + 1);

    Ok(())
}

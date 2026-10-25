//! Preserved processing failures scenarios; stable test entries stay in the parent.
use super::*;

pub(super) fn peer_external_commit_storage_failure_keeps_the_cursor_epoch_and_context_for_retry(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let space = "peer-merge-storage-failure";
    let fixture = staged_recovery(":memory:", space)?;
    let mut recovered = fixture.engine;
    let mut peer = fixture.peer_engine;
    for frame in &fixture.baseline {
        assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    }
    let external = Envelope::plain(&recovered.store.outbox().unwrap()[0].event)?;
    let reference = RecoveryRef::parse(external.recovery().unwrap()).unwrap();
    recovered.upload_done(&reference.blob);
    let frame = ordered(&peer, external)?;
    let before = (
        peer.logs[space].next_seq(),
        seen(&peer, space),
        peer.store.events(space).unwrap().len(),
        peer.device().unwrap().checkpoint(space).unwrap(),
        peer.recovery_context(space),
        peer.device().unwrap().recovery_pending(space),
        peer.device().unwrap().needs_reconciliation(space),
        peer.outbox_len(),
        peer.store.cursors().unwrap(),
    );
    assert!(
        !before.5,
        "the existing peer has no own recovery to confirm"
    );
    for (install, remove, reason) in [
        (
            "CREATE TRIGGER fail_peer_merge BEFORE INSERT ON openmls_group_data
         WHEN NEW.data_type = 'group_state'
         BEGIN SELECT RAISE(ABORT, 'injected peer merge failure'); END;",
            "DROP TRIGGER fail_peer_merge",
            "injected peer merge failure",
        ),
        (
            "ALTER TABLE openmls_group_data RENAME TO unavailable_group_data",
            "ALTER TABLE unavailable_group_data RENAME TO openmls_group_data",
            "no such table",
        ),
        (
            "ALTER TABLE openmls_epoch_keys_pairs RENAME TO unavailable_epoch_keys",
            "ALTER TABLE unavailable_epoch_keys RENAME TO openmls_epoch_keys_pairs",
            "MissingDecryptionKey",
        ),
    ] {
        peer.store.conn().execute_batch(install).unwrap();
        let result = peer.ingest(frame.clone());
        assert!(
            matches!(result, Ingest::Invalid(ref error) if error.contains(reason)),
            "{result:?}"
        );
        peer.store.conn().execute_batch(remove).unwrap();
        assert_eq!(
            (
                peer.logs[space].next_seq(),
                seen(&peer, space),
                peer.store.events(space).unwrap().len(),
                peer.device().unwrap().checkpoint(space).unwrap(),
                peer.recovery_context(space),
                peer.device().unwrap().recovery_pending(space),
                peer.device().unwrap().needs_reconciliation(space),
                peer.outbox_len(),
                peer.store.cursors().unwrap(),
            ),
            before
        );
    }
    assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    assert_eq!(recovered.ingest(frame), Ingest::Confirmed);
    assert_eq!(peer.logs[space].next_seq(), before.0 + 1);
    assert_eq!(
        peer.recovery_context(space),
        recovered.recovery_context(space)
    );
    assert_eq!(
        peer.device().unwrap().checkpoint(space).unwrap(),
        recovered.device().unwrap().checkpoint(space).unwrap()
    );
    for (sender, recipient) in [(&recovered, &peer), (&peer, &recovered)] {
        let sender = sender.device().unwrap();
        let data = sender
            .seal(space, b"readable after the identical retry")
            .unwrap();
        assert!(matches!(
            recipient.device().unwrap().open(space, &data, &recipient.roster(space), sender.leaf()).unwrap(),
            ::mls::Opened::Application { plaintext, .. } if plaintext == b"readable after the identical retry"
        ));
    }
    Ok(())
}

pub(super) fn peer_application_processing_failure_keeps_the_same_frame_readable_on_retry(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let space = "peer-processing-storage-failure";
    let fixture = staged_recovery(":memory:", space)?;
    let mut recovered = fixture.engine;
    let mut peer = fixture.peer_engine;
    for frame in &fixture.baseline {
        assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    }
    let external = Envelope::plain(&recovered.store.outbox().unwrap()[0].event)?;
    let reference = RecoveryRef::parse(external.recovery().unwrap()).unwrap();
    recovered.upload_done(&reference.blob);
    let frame = ordered(&peer, external)?;
    assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    assert_eq!(recovered.ingest(frame), Ingest::Confirmed);
    recovered.send_message(space, "readable on retry").unwrap();
    recovered.mls_seal_outbox().unwrap();
    let application = recovered
        .outbox_envelopes()
        .into_iter()
        .find(|env| env.sealed_kind() == Some(SealedKind::Application))
        .unwrap();
    let frame = ordered(&peer, application)?;
    let before = (
        peer.logs[space].next_seq(),
        peer.store.cursors().unwrap(),
        peer.store.events(space).unwrap().len(),
        peer.device().unwrap().checkpoint(space).unwrap(),
        peer.recovery_context(space),
    );
    peer.store
        .conn()
        .execute_batch(
            "CREATE TRIGGER fail_processing BEFORE INSERT ON openmls_group_data
         WHEN NEW.data_type = 'message_secrets'
         BEGIN SELECT RAISE(ABORT, 'injected processing failure'); END;",
        )
        .unwrap();
    let result = peer.ingest(frame.clone());
    assert!(
        matches!(result, Ingest::Invalid(ref reason) if reason.contains("injected processing failure")),
        "{result:?}"
    );
    assert_eq!(
        (
            peer.logs[space].next_seq(),
            peer.store.cursors().unwrap(),
            peer.store.events(space).unwrap().len(),
            peer.device().unwrap().checkpoint(space).unwrap(),
            peer.recovery_context(space),
        ),
        before
    );
    peer.store
        .conn()
        .execute_batch("DROP TRIGGER fail_processing")
        .unwrap();
    assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    assert_eq!(recovered.ingest(frame), Ingest::Confirmed);
    assert!(matches!(
        &peer.state.spaces[space].entries.last().unwrap().body,
        crate::engine::EntryBody::Message { text, .. } if text == "readable on retry"
    ));

    Ok(())
}

pub(super) fn welcome_storage_failure_rolls_back_the_key_package_and_cursor_for_retry(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let space = "welcome-storage-failure";
    let fixture = staged_recovery(":memory:", space)?;
    let mut peer = fixture.peer_engine;
    let welcome = fixture.baseline.last().unwrap().clone();
    assert_eq!(welcome.env.sealed_kind(), Some(SealedKind::Welcome));
    for frame in &fixture.baseline[..fixture.baseline.len() - 1] {
        assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    }
    assert!(!peer.device().unwrap().has_group(space));
    let before = (
        peer.logs[space].next_seq(),
        peer.store.cursors().unwrap(),
        peer.store.events(space).unwrap().len(),
        peer.recovery_context(space),
    );
    peer.store
        .conn()
        .execute_batch(
            "CREATE TRIGGER fail_welcome BEFORE INSERT ON openmls_group_data
         WHEN NEW.data_type = 'group_state'
         BEGIN SELECT RAISE(ABORT, 'injected Welcome failure'); END;",
        )
        .unwrap();
    let result = peer.ingest(welcome.clone());
    assert!(
        matches!(result, Ingest::Invalid(ref reason) if reason.contains("injected Welcome failure")),
        "{result:?}"
    );
    assert_eq!(
        (
            peer.logs[space].next_seq(),
            peer.store.cursors().unwrap(),
            peer.store.events(space).unwrap().len(),
            peer.recovery_context(space),
        ),
        before
    );
    assert!(!peer.device().unwrap().has_group(space));
    peer.store
        .conn()
        .execute_batch("DROP TRIGGER fail_welcome")
        .unwrap();
    assert_eq!(peer.ingest(welcome), Ingest::Applied);
    assert_eq!(peer.device().unwrap().epoch(space).unwrap(), 1);
    assert_eq!(peer.logs[space].next_seq(), before.0 + 1);

    Ok(())
}

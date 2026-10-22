//! Existing SQLite crash/confirmation assertions, invoked by stable test entrypoints.
use super::*;

pub(super) fn plain_confirmation_cleanup_failure_keeps_exact_outbox_for_cold_retry() {
    let database = Database::new();
    let path = database.path("sender.sqlite");
    let (mut sender, _, secret) = member(&path);
    let (_, peer, _) = member(":memory:");
    let space = "plain-confirmation-transaction";
    closed_space(&mut sender, &peer, space);
    let message = sender
        .send_message(space, "Only one confirmed message")
        .unwrap();
    let envelope = sender.outbox_envelopes().into_iter().next().unwrap();
    let queued = sender
        .store
        .outbox_get(envelope.client_id())
        .unwrap()
        .unwrap();
    let frame = ordered(&sender, envelope.clone());
    let before = sender.store.event_count().unwrap();
    sender
        .store
        .conn()
        .execute_batch(
            "CREATE TEMP TRIGGER fail_outbox_cleanup BEFORE DELETE ON outbox
         BEGIN SELECT RAISE(ABORT, 'injected confirmation cleanup failure'); END;",
        )
        .unwrap();
    assert!(
        matches!(sender.ingest(frame.clone()), Ingest::Invalid(reason)
        if reason.contains("injected confirmation cleanup failure"))
    );
    assert!(sender.store.conn().is_autocommit());
    assert_eq!(sender.store.event_count().unwrap(), before);
    assert_eq!(
        sender.store.outbox_get(envelope.client_id()).unwrap(),
        Some(queued.clone())
    );
    assert!(sender.net.pending.contains_key(envelope.client_id()));
    assert_eq!(
        sender.timeline(space).unwrap()[0].delivery,
        crate::dto::Delivery::Sending
    );
    drop(sender);

    let mut sender = reopen(&path, &secret);
    assert_eq!(sender.outbox_envelopes(), vec![envelope.clone()]);
    assert_eq!(
        sender.store.outbox_get(envelope.client_id()).unwrap(),
        Some(queued)
    );
    assert_eq!(message_count(&sender, space, &message.id), 1);
    assert_eq!(sender.ingest(frame.clone()), Ingest::Confirmed);
    assert_eq!(sender.ingest(frame), Ingest::Duplicate);
    assert_eq!(sender.store.event_count().unwrap(), before + 1);
    assert!(sender.store.outbox().unwrap().is_empty());
    assert_eq!(message_count(&sender, space, &message.id), 1);
    assert_eq!(
        sender.timeline(space).unwrap()[0].delivery,
        crate::dto::Delivery::Sent
    );
    drop(sender);

    let sender = reopen(&path, &secret);
    assert!(sender.store.outbox().unwrap().is_empty());
    assert_eq!(message_count(&sender, space, &message.id), 1);
}

pub(super) fn pending_confirmation_other_member_does_not_ack_or_stamp_our_plain_event() {
    let database = Database::new();
    let path = database.path("member-collision.sqlite");
    let (mut owner, _, secret) = member(&path);
    let (_, peer, _) = member(":memory:");
    let space = "plain-member-collision";
    closed_space(&mut owner, &peer, space);
    let message = owner.send_message(space, "Owner stays queued").unwrap();
    let pending = owner.store.outbox_get(&message.id).unwrap().unwrap();
    let remote = peer.sign_event(
        space,
        &pending.client_id,
        pending.at_ms + 1,
        pending.seen.clone(),
        post("Peer is independently applied"),
    );
    let frame = ordered(&owner, Envelope::plain(&remote));
    assert_eq!(owner.ingest(frame.clone()), Ingest::Applied);
    assert_pending_confirmation_retained(&owner, &pending);
    assert_pending_confirmation_remote(&owner, &frame, &peer);
    owner.reproject_approval_overlay().unwrap();
    assert_pending_confirmation_retained(&owner, &pending);
    assert_pending_confirmation_remote(&owner, &frame, &peer);
    drop(owner);
    let mut owner = reopen(&path, &secret);
    assert_pending_confirmation_retained(&owner, &pending);
    assert_pending_confirmation_remote(&owner, &frame, &peer);
    let own_frame = ordered(&owner, Envelope::plain(&pending));
    assert_eq!(owner.ingest(own_frame.clone()), Ingest::Confirmed);
    assert!(owner
        .store
        .outbox_get(&pending.client_id)
        .unwrap()
        .is_none());
    assert_pending_confirmation_remote(&owner, &frame, &peer);
    let mine = owner.state.spaces[space]
        .entries
        .iter()
        .find(|entry| entry.author == pending.author && entry.client_id == pending.client_id)
        .unwrap();
    assert_eq!(
        mine.hash, own_frame.hash,
        "only the optimistic owner row is stamped"
    );
    assert_eq!(owner.state.spaces[space].entries.len(), 2);
}

pub(super) fn pending_confirmation_another_certified_device_cannot_ack_this_devices_event() {
    let database = Database::new();
    let path = database.path("device-collision.sqlite");
    let mut owner = Engine::open(&path).unwrap();
    let (root, secret, _) = owner
        .create_account("Owner", "owner_devices", "http://relay.test")
        .unwrap();
    let other = Author::device(&Signer::from_secret(&root), Signer::generate());
    let space = "plain-device-collision";
    let (_, peer, _) = member(":memory:");
    closed_space(&mut owner, &peer, space);
    let message = owner
        .send_message(space, "First device stays queued")
        .unwrap();
    let pending = owner.store.outbox_get(&message.id).unwrap().unwrap();
    assert_ne!(pending.device, other.device);
    assert_eq!(pending.author, other.identity);
    let remote = other.sign_event(
        space,
        &pending.client_id,
        pending.at_ms + 1,
        pending.seen.clone(),
        post("Other certified device"),
    );
    event_log::verify_author(&remote).unwrap();
    let frame = ordered(&owner, Envelope::plain(&remote));
    assert_eq!(owner.ingest(frame.clone()), Ingest::Applied);
    assert_pending_confirmation_retained(&owner, &pending);
    assert_pending_confirmation_remote(&owner, &frame, &other);
    drop(owner);
    let owner = reopen(&path, &secret);
    assert_pending_confirmation_retained(&owner, &pending);
}

pub(super) fn pending_confirmation_same_signer_with_different_signed_bytes_is_not_our_receipt() {
    let (mut owner, author, _) = member(":memory:");
    let (_, peer, _) = member(":memory:");
    let space = "plain-commitment-collision";
    closed_space(&mut owner, &peer, space);
    let message = owner
        .send_message(space, "Keep the original signed intent")
        .unwrap();
    let pending = owner.store.outbox_get(&message.id).unwrap().unwrap();
    let changed = author.sign_event(
        space,
        &pending.client_id,
        pending.at_ms,
        pending.seen.clone(),
        post("Different signed intent"),
    );
    assert_ne!(pending.content, changed.content);
    let frame = ordered(&owner, Envelope::plain(&changed));
    assert_eq!(owner.ingest(frame.clone()), Ingest::Applied);
    assert_pending_confirmation_retained(&owner, &pending);
    owner.reproject_approval_overlay().unwrap();
    assert_pending_confirmation_retained(&owner, &pending);
    assert_pending_confirmation_remote(&owner, &frame, &author);
}

pub(super) fn pending_confirmation_other_space_cannot_remove_a_pending_event_on_cold_reopen() {
    let database = Database::new();
    let path = database.path("space-collision.sqlite");
    let (mut owner, _, secret) = member(&path);
    let (_, peer, _) = member(":memory:");
    closed_space(&mut owner, &peer, "pending-space");
    closed_space(&mut owner, &peer, "remote-space");
    let message = owner
        .send_message("pending-space", "Still belongs to the first space")
        .unwrap();
    let pending = owner.store.outbox_get(&message.id).unwrap().unwrap();
    let remote = peer.sign_event(
        "remote-space",
        &pending.client_id,
        pending.at_ms + 1,
        seen(&owner, "remote-space"),
        post("Same label in a different space"),
    );
    let frame = ordered(&owner, Envelope::plain(&remote));
    assert_eq!(owner.ingest(frame.clone()), Ingest::Applied);
    assert_pending_confirmation_retained(&owner, &pending);
    drop(owner);
    let owner = reopen(&path, &secret);
    assert_pending_confirmation_retained(&owner, &pending);
    assert_pending_confirmation_remote(&owner, &frame, &peer);
}

pub(super) fn pending_confirmation_other_mls_member_keeps_pending_overlay_and_cold_outbox() {
    let database = Database::new();
    let space = "sealed-member-collision";
    let (mut owner, secret, mut peer, _) = encrypted_pair(&database, space);
    let message = owner
        .send_message(space, "Owner encrypted intent remains pending")
        .unwrap();
    let pending = owner.store.outbox_get(&message.id).unwrap().unwrap();
    let author = peer.net.author.clone().unwrap();
    let remote = author.sign_event(
        space,
        &pending.client_id,
        pending.at_ms + 1,
        seen(&peer, space),
        post("Authenticated other MLS member"),
    );
    peer.seal_outgoing(&remote, peer.ready_epoch(space).unwrap())
        .unwrap();
    let envelope = peer.sealed_copy(&remote).unwrap().1;
    let frame = ordered(&owner, envelope);
    assert_eq!(owner.ingest(frame.clone()), Ingest::Applied);
    assert_pending_confirmation_retained(&owner, &pending);
    owner.reproject_approval_overlay().unwrap();
    assert_pending_confirmation_retained(&owner, &pending);
    assert_pending_confirmation_remote(&owner, &frame, &author);
    drop(owner);
    let owner = reopen(&database.path("owner.sqlite"), &secret);
    assert_pending_confirmation_retained(&owner, &pending);
    assert_pending_confirmation_remote(&owner, &frame, &author);
}

pub(super) fn pending_confirmation_exact_signed_echo_cleans_a_crash_window_on_cold_reopen() {
    let database = Database::new();
    let path = database.path("true-confirmation.sqlite");
    let (mut owner, _, secret) = member(&path);
    let (_, peer, _) = member(":memory:");
    let space = "exact-pending-confirmation";
    closed_space(&mut owner, &peer, space);
    let message = owner
        .send_message(space, "Only the exact signed receipt")
        .unwrap();
    let pending = owner.store.outbox_get(&message.id).unwrap().unwrap();
    let mut envelope = Envelope::plain(&pending);
    envelope.sig = envelope.sig.to_uppercase();
    envelope.cert = envelope.cert.map(|cert| cert.to_uppercase());
    envelope.verify().unwrap();
    let frame = ordered(&owner, envelope);
    assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
    assert!(owner
        .store
        .outbox_get(&pending.client_id)
        .unwrap()
        .is_none());
    // Model a crash between durable receipt and outbox cleanup, retaining original bytes.
    owner.store.outbox_put(&pending).unwrap();
    drop(owner);
    let owner = reopen(&path, &secret);
    assert!(owner
        .store
        .outbox_get(&pending.client_id)
        .unwrap()
        .is_none());
    assert_eq!(message_count(&owner, space, &message.id), 1);
    owner.logs[space].verify().unwrap();
}

pub(super) fn pending_confirmation_outbox_read_failure_never_appends_or_guesses_an_ack() {
    let (mut owner, _, _) = member(":memory:");
    let (_, peer, _) = member(":memory:");
    let space = "confirmation-storage-read";
    closed_space(&mut owner, &peer, space);
    let message = owner
        .send_message(space, "Retain unreadable pending record")
        .unwrap();
    let pending = owner.store.outbox_get(&message.id).unwrap().unwrap();
    let frame = ordered(&owner, Envelope::plain(&pending));
    let before = owner.store.event_count().unwrap();
    owner
        .store
        .conn()
        .execute(
            "UPDATE outbox SET json='{' WHERE client_id=?1",
            [&pending.client_id],
        )
        .unwrap();
    assert!(matches!(owner.ingest(frame.clone()), Ingest::Invalid(_)));
    assert_eq!(owner.store.event_count().unwrap(), before);
    assert_eq!(owner.next_seq(space), frame.seq);
    assert!(owner
        .store
        .outbox_heads()
        .unwrap()
        .iter()
        .any(|head| head.client_id == pending.client_id));
    assert_eq!(
        owner.net.pending.get(&pending.client_id),
        Some(&pending.space)
    );
    owner
        .store
        .conn()
        .execute(
            "UPDATE outbox SET json=?1 WHERE client_id=?2",
            rusqlite::params![serde_json::to_string(&pending).unwrap(), pending.client_id],
        )
        .unwrap();
    assert_eq!(owner.ingest(frame), Ingest::Confirmed);
}

pub(super) fn pending_confirmation_retained_older_sealed_echo_survives_resealing() {
    let database = Database::new();
    let space = "sealed-original-ack-after-reseal";
    let (mut owner, secret, _, _) = encrypted_pair(&database, space);
    let message = owner
        .send_message(space, "Accept the actual earlier signed echo")
        .unwrap();
    let pending = owner.store.outbox_get(&message.id).unwrap().unwrap();
    let epoch = owner.ready_epoch(space).unwrap();
    owner.seal_outgoing(&pending, epoch).unwrap();
    let original = owner.sealed_copy(&pending).unwrap().1;
    // Exercise the real sealer again, consuming a new sender generation and
    // replacing its cache. No new plaintext intent or client ID is created.
    owner.seal_outgoing(&pending, epoch).unwrap();
    let newer = owner.sealed_copy(&pending).unwrap().1;
    assert_ne!(original.content(), newer.content());
    original.verify().unwrap();
    newer.verify().unwrap();
    let frame = ordered(&owner, original);
    assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
    assert!(owner
        .store
        .outbox_get(&pending.client_id)
        .unwrap()
        .is_none());
    assert!(owner
        .store
        .meta(&sealed_meta(&pending.client_id))
        .unwrap()
        .is_none());
    assert_eq!(owner.ingest(frame), Ingest::Duplicate);
    assert_eq!(message_count(&owner, space, &message.id), 1);
    drop(owner);
    let owner = reopen(&database.path("owner.sqlite"), &secret);
    assert!(owner
        .store
        .outbox_get(&pending.client_id)
        .unwrap()
        .is_none());
    assert_eq!(message_count(&owner, space, &message.id), 1);
    owner.logs[space].verify().unwrap();
}

pub(super) fn pending_confirmation_sealed_outbox_read_failure_keeps_event_and_ratchet_unchanged() {
    let database = Database::new();
    let space = "sealed-confirmation-storage-read";
    let (mut owner, _, _, _) = encrypted_pair(&database, space);
    let message = owner
        .send_message(space, "Do not guess an encrypted ACK")
        .unwrap();
    let pending = owner.store.outbox_get(&message.id).unwrap().unwrap();
    assert!(owner.mls_seal_outbox().unwrap());
    let frame = ordered(&owner, owner.sealed_copy(&pending).unwrap().1);
    let before = owner.store.event_count().unwrap();
    let provider_before = provider_rows(&owner);
    owner
        .store
        .conn()
        .execute(
            "UPDATE outbox SET json='{' WHERE client_id=?1",
            [&pending.client_id],
        )
        .unwrap();
    assert!(matches!(owner.ingest(frame.clone()), Ingest::Invalid(_)));
    assert_eq!(owner.store.event_count().unwrap(), before);
    assert_eq!(owner.next_seq(space), frame.seq);
    assert_eq!(provider_rows(&owner), provider_before);
    assert!(owner
        .store
        .outbox_heads()
        .unwrap()
        .iter()
        .any(|head| head.client_id == pending.client_id));
    assert!(owner.sealed_copy(&pending).is_some());
    owner
        .store
        .conn()
        .execute(
            "UPDATE outbox SET json=?1 WHERE client_id=?2",
            rusqlite::params![serde_json::to_string(&pending).unwrap(), pending.client_id],
        )
        .unwrap();
    assert_eq!(owner.ingest(frame), Ingest::Confirmed);
}

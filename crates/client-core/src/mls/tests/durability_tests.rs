//! Crash/retry boundaries around the existing SQLite log, outbox and MLS provider.
//! These exercise database failures after useful work, not a mocked store.
use super::*;
use std::path::PathBuf;

struct Database {
    directory: PathBuf,
}

impl Database {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(types::new_id("delivery-durability"));
        std::fs::create_dir_all(&directory).unwrap();
        Self { directory }
    }

    fn path(&self, name: &str) -> String {
        self.directory.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).unwrap();
    }
}

fn reopen(path: &str, secret: &[u8; 32]) -> Engine {
    let mut engine = Engine::open(path).unwrap();
    assert!(engine.unlock(Some(secret.to_vec())).unwrap());
    engine
}

fn closed_space(engine: &mut Engine, peer: &Author, space: &str) {
    let owner = engine.net.author.clone().unwrap();
    ingest_signed(
        engine,
        &owner,
        space,
        EventBody::SpaceCreated {
            title: space.into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    );
    add_member(engine, &owner, space, peer);
}

fn encrypted_pair(database: &Database, space: &str) -> (Engine, [u8; 32], Engine, [u8; 32]) {
    let (mut owner, _, owner_secret) = member(&database.path("owner.sqlite"));
    let (mut peer, _, peer_secret) = member(&database.path("peer.sqlite"));
    let initial = claim_group(&mut owner, space, std::slice::from_mut(&mut peer));
    for frame in initial {
        assert_eq!(peer.ingest(frame), Ingest::Applied);
    }
    let peer_identity = peer.me.clone().unwrap();
    let records = claimed_records(
        std::slice::from_mut(&mut peer),
        std::slice::from_ref(&peer_identity),
    );
    owner
        .commit_now(space, &[records[0].data.clone()], &Default::default())
        .unwrap();
    relay_ready_to_peer(&mut owner, &mut peer);
    assert!(owner.ready_epoch(space).is_some());
    assert!(peer.device().unwrap().has_group(space));
    (owner, owner_secret, peer, peer_secret)
}

// Compare the actual sealed provider state, including the sender/receiver ratchets.
fn provider_rows(engine: &Engine) -> Vec<(Vec<u8>, String, Vec<u8>, i64)> {
    let mut query = engine
        .store
        .conn()
        .prepare(
            "SELECT group_id, data_type, group_data, provider_version
             FROM openmls_group_data ORDER BY group_id, data_type",
        )
        .unwrap();
    let rows = query
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    rows
}

fn message_count(engine: &Engine, space: &str, id: &str) -> usize {
    engine
        .timeline(space)
        .unwrap()
        .iter()
        .filter(|message| message.id == id)
        .count()
}

#[test]
fn plain_receive_scope_failure_rolls_back_event_and_delivered_progress_across_reopen() {
    let database = Database::new();
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
    );
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
}

#[test]
fn plain_confirmation_cleanup_failure_keeps_exact_outbox_for_cold_retry() {
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

#[test]
fn failed_ciphertext_retention_rolls_back_sender_ratchet_before_cold_retry() {
    let database = Database::new();
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
}

#[test]
fn sealed_receive_record_failure_rolls_back_ratchet_event_and_readable_receipt() {
    assert_sealed_receive_rollback(
        "CREATE TEMP TRIGGER fail_opening_record BEFORE INSERT ON meta
         WHEN NEW.key LIKE 'runtime.mls-opening.v1/%'
         BEGIN SELECT RAISE(ABORT, 'injected opened receipt failure'); END;",
        "injected opened receipt failure",
    );
}

#[test]
fn sealed_receive_scope_failure_rolls_back_ratchet_and_retained_opening() {
    assert_sealed_receive_rollback(
        "CREATE TEMP TRIGGER fail_scope BEFORE INSERT ON synced_spaces
         BEGIN SELECT RAISE(ABORT, 'injected sealed scope failure'); END;",
        "injected sealed scope failure",
    );
}

fn assert_sealed_receive_rollback(trigger: &str, marker: &str) {
    let database = Database::new();
    let space = "sealed-receive-retention";
    let (mut owner, _, mut peer, secret) = encrypted_pair(&database, space);
    let message = owner
        .send_message(space, "Retry the same encrypted envelope")
        .unwrap();
    assert!(owner.mls_seal_outbox().unwrap());
    let envelope = owner
        .outbox_envelopes()
        .into_iter()
        .find(|envelope| envelope.sealed_kind() == Some(SealedKind::Application))
        .unwrap();
    let frame = ordered(&owner, envelope);
    assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
    let before = provider_rows(&peer);
    let count_before = peer.store.event_count().unwrap();
    let delivered_before = peer.delivered_to_announce(space);
    let opening_key = format!("runtime.mls-opening.v1/{}", frame.hash);
    assert!(peer.store.meta(&opening_key).unwrap().is_none());
    peer.store.conn().execute_batch(trigger).unwrap();
    assert!(matches!(peer.ingest(frame.clone()), Ingest::Invalid(reason)
        if reason.contains(marker)));
    assert!(peer.store.conn().is_autocommit());
    assert_eq!(
        provider_rows(&peer),
        before,
        "receiver ratchet rolls back with its event"
    );
    assert_eq!(peer.store.event_count().unwrap(), count_before);
    assert!(peer.store.meta(&opening_key).unwrap().is_none());
    assert_eq!(peer.next_seq(space), frame.seq);
    assert_eq!(peer.delivered_to_announce(space), delivered_before);
    assert_eq!(message_count(&peer, space, &message.id), 0);
    drop(peer);

    let mut peer = reopen(&database.path("peer.sqlite"), &secret);
    assert_eq!(provider_rows(&peer), before);
    assert!(peer.store.meta(&opening_key).unwrap().is_none());
    assert_eq!(peer.next_seq(space), frame.seq);
    assert_eq!(message_count(&peer, space, &message.id), 0);
    assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    assert!(peer.store.meta(&opening_key).unwrap().is_some());
    assert_eq!(peer.delivered_to_announce(space), Some(frame.seq));
    assert_eq!(peer.ingest(frame.clone()), Ingest::Duplicate);
    assert_eq!(peer.store.event_count().unwrap(), count_before + 1);
    assert_eq!(message_count(&peer, space, &message.id), 1);
    peer.delivered_announced(space, frame.seq);
    assert_eq!(peer.delivered_to_announce(space), None);
    assert!(peer.receipts_to_replay().iter().any(|(id, kind)| id == space
        && matches!(kind, protocol::EphemeralKind::Delivered { seq } if *seq == frame.seq)),
        "a socket-accepted receipt is replayed because socket acceptance is not remote acknowledgement");
    drop(peer);

    let mut peer = reopen(&database.path("peer.sqlite"), &secret);
    assert_eq!(message_count(&peer, space, &message.id), 1);
    assert_eq!(peer.ingest(frame), Ingest::Duplicate);
    assert_eq!(peer.store.event_count().unwrap(), count_before + 1);
}

#[test]
fn pruned_receive_scope_failure_preserves_the_group_until_the_whole_receipt_commits() {
    let database = Database::new();
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
}

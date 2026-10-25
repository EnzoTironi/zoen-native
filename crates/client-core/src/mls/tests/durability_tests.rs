//! Crash/retry boundaries around the existing SQLite log, outbox and MLS provider.
//! These exercise database failures after useful work, not a mocked store.
use super::*;
use std::path::PathBuf;

mod checkpoint_refresh_cases;
mod confirmation_cases;
mod fifo_cases;
mod import_cases;
mod integrity_marker_cases;
mod receive_cases;

struct Database {
    directory: PathBuf,
}

impl Database {
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let directory = std::env::temp_dir().join(types::new_id("delivery-durability")?);
        std::fs::create_dir_all(&directory).unwrap();
        Ok(Self { directory })
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

fn closed_space(
    engine: &mut Engine,
    peer: &Author,
    space: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
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
    )?;
    add_member(engine, &owner, space, peer)?;
    Ok(())
}

fn encrypted_pair(
    database: &Database,
    space: &str,
) -> std::result::Result<(Engine, [u8; 32], Engine, [u8; 32]), Box<dyn std::error::Error>> {
    let (mut owner, _, owner_secret) = member(&database.path("owner.sqlite"));
    let (mut peer, _, peer_secret) = member(&database.path("peer.sqlite"));
    let initial = claim_group(&mut owner, space, std::slice::from_mut(&mut peer))?;
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
    relay_ready_to_peer(&mut owner, &mut peer)?;
    assert!(owner.ready_epoch(space).is_some());
    assert!(peer.device().unwrap().has_group(space));
    Ok((owner, owner_secret, peer, peer_secret))
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
    receive_cases::plain_receive_scope_failure_rolls_back_event_and_delivered_progress_across_reopen().unwrap();
}

#[test]
fn plain_confirmation_cleanup_failure_keeps_exact_outbox_for_cold_retry() {
    confirmation_cases::plain_confirmation_cleanup_failure_keeps_exact_outbox_for_cold_retry()
        .unwrap();
}

#[test]
fn failed_ciphertext_retention_rolls_back_sender_ratchet_before_cold_retry() {
    receive_cases::failed_ciphertext_retention_rolls_back_sender_ratchet_before_cold_retry()
        .unwrap();
}

#[test]
fn sealed_receive_record_failure_rolls_back_ratchet_event_and_readable_receipt() {
    receive_cases::sealed_receive_record_failure_rolls_back_ratchet_event_and_readable_receipt()
        .unwrap();
}

#[test]
fn sealed_receive_scope_failure_rolls_back_ratchet_and_retained_opening() {
    receive_cases::sealed_receive_scope_failure_rolls_back_ratchet_and_retained_opening().unwrap();
}

fn assert_sealed_receive_rollback(
    trigger: &str,
    marker: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let database = Database::new()?;
    let space = "sealed-receive-retention";
    let (mut owner, _, mut peer, secret) = encrypted_pair(&database, space)?;
    let message = owner
        .send_message(space, "Retry the same encrypted envelope")
        .unwrap();
    assert!(owner.mls_seal_outbox().unwrap());
    let envelope = owner
        .outbox_envelopes()
        .into_iter()
        .find(|envelope| envelope.sealed_kind() == Some(SealedKind::Application))
        .unwrap();
    let frame = ordered(&owner, envelope)?;
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

    Ok(())
}

#[test]
fn pruned_receive_scope_failure_preserves_the_group_until_the_whole_receipt_commits() {
    receive_cases::pruned_receive_scope_failure_preserves_the_group_until_the_whole_receipt_commits(
    ).unwrap();
}

// A client ID is a routing/idempotency label, not proof that a signed event is ours.
fn assert_pending_confirmation_retained(engine: &Engine, pending: &Event) {
    assert_eq!(
        engine.store.outbox_get(&pending.client_id).unwrap(),
        Some(pending.clone())
    );
    assert_eq!(
        engine.net.pending.get(&pending.client_id),
        Some(&pending.space)
    );
    let local = engine.state.spaces[&pending.space]
        .entries
        .iter()
        .find(|entry| entry.client_id == pending.client_id && entry.hash.is_empty())
        .expect("the exact optimistic event remains projected");
    assert_eq!(local.author, pending.author);
    assert_eq!(local.at_ms, pending.at_ms);
    match (&pending.body, &local.body) {
        (
            EventBody::MessagePosted { text, .. },
            crate::engine::EntryBody::Message {
                text: projected, ..
            },
        ) => assert_eq!(projected, text),
        _ => panic!("the original message remains projected"),
    }
    assert_eq!(engine.delivery(local), crate::dto::Delivery::Sending);
}

fn assert_pending_confirmation_remote(engine: &Engine, frame: &Sequenced, author: &Author) {
    let remote = engine.state.spaces[frame.env.space()]
        .entries
        .iter()
        .find(|entry| entry.hash == frame.hash)
        .expect("the remote event is independently projected");
    assert_eq!(remote.client_id, frame.env.client_id());
    assert_eq!(remote.author, author.identity);
    let signed = engine.logs[frame.env.space()]
        .events()
        .iter()
        .find(|event| event.hash == frame.hash)
        .unwrap();
    match (&signed.body, &remote.body) {
        (
            EventBody::MessagePosted { text, .. },
            crate::engine::EntryBody::Message {
                text: projected, ..
            },
        ) => assert_eq!(projected, text),
        _ => panic!("the authenticated remote message remains projected"),
    }
    assert_eq!(engine.delivery(remote), crate::dto::Delivery::Sent);
    engine.logs[frame.env.space()].verify().unwrap();
}

#[test]
fn pending_confirmation_other_member_does_not_ack_or_stamp_our_plain_event() {
    confirmation_cases::pending_confirmation_other_member_does_not_ack_or_stamp_our_plain_event()
        .unwrap();
}

#[test]
fn pending_confirmation_another_certified_device_cannot_ack_this_devices_event() {
    confirmation_cases::pending_confirmation_another_certified_device_cannot_ack_this_devices_event(
    ).unwrap();
}

#[test]
fn pending_confirmation_same_signer_with_different_signed_bytes_is_not_our_receipt(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    confirmation_cases::pending_confirmation_same_signer_with_different_signed_bytes_is_not_our_receipt()
}

#[test]
fn pending_confirmation_other_space_cannot_remove_a_pending_event_on_cold_reopen() {
    confirmation_cases::pending_confirmation_other_space_cannot_remove_a_pending_event_on_cold_reopen().unwrap();
}

#[test]
fn pending_confirmation_other_mls_member_keeps_pending_overlay_and_cold_outbox() {
    confirmation_cases::pending_confirmation_other_mls_member_keeps_pending_overlay_and_cold_outbox(
    ).unwrap();
}

#[test]
fn pending_confirmation_exact_signed_echo_cleans_a_crash_window_on_cold_reopen() {
    confirmation_cases::pending_confirmation_exact_signed_echo_cleans_a_crash_window_on_cold_reopen(
    ).unwrap();
}

#[test]
fn pending_confirmation_outbox_read_failure_never_appends_or_guesses_an_ack(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    confirmation_cases::pending_confirmation_outbox_read_failure_never_appends_or_guesses_an_ack()
}

#[test]
fn pending_confirmation_retained_older_sealed_echo_survives_resealing() {
    confirmation_cases::pending_confirmation_retained_older_sealed_echo_survives_resealing()
        .unwrap();
}

#[test]
fn pending_confirmation_sealed_outbox_read_failure_keeps_event_and_ratchet_unchanged() {
    confirmation_cases::pending_confirmation_sealed_outbox_read_failure_keeps_event_and_ratchet_unchanged().unwrap();
}

#[test]
fn cold_outbox_fifo_preserves_signed_intents_ciphertext_and_receiver_order_after_clock_rollback() {
    fifo_cases::cold_outbox_fifo_preserves_signed_intents_ciphertext_and_receiver_order_after_clock_rollback().unwrap();
}

#[test]
fn backward_clock_welcome_stays_behind_original_commit_and_other_space_advances() {
    fifo_cases::backward_clock_welcome_stays_behind_original_commit_and_other_space_advances()
        .unwrap();
}

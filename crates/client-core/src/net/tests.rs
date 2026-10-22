mod catchup_cases;
mod lease_cases;
mod ordering_cases;
mod pickup_cases;
mod profile_clock_cases;
mod queue_phases;
mod queue_scenario;
use queue_scenario::queued_messages_round_trip;

mod cursor_repair;
mod incoming_pipeline;
mod initial_catchup;
mod media_dependencies;
mod media_limits;
mod media_uploads;

#[tokio::test]
async fn pickup_expired_deadline_opens_no_connection() {
    pickup_cases::pickup_expired_deadline_opens_no_connection().await;
}

#[tokio::test]
async fn pickup_deadline_bounds_a_stalled_connection() {
    pickup_cases::pickup_deadline_bounds_a_stalled_connection().await;
}

#[tokio::test]
async fn pickup_reconnects_after_transport_failure_and_is_cancelable() {
    pickup_cases::pickup_reconnects_after_transport_failure_and_is_cancelable().await;
}

#[test]
fn an_unrefreshed_receipt_clock_expires_without_advancing_its_timestamp() {
    profile_clock_cases::an_unrefreshed_receipt_clock_expires_without_advancing_its_timestamp();
}

use super::*;

#[test]
fn agent_definition_configured_http_name_keeps_case_and_default_port_binding() {
    profile_clock_cases::agent_definition_configured_http_name_keeps_case_and_default_port_binding(
    );
}
use ::mls::{sealed::state_key, Device, Opened, SUITE_ID};
use event_log::{
    chain_hash, content::InnerEvent, content::Sealed, content::SealedKind, Author, Signer,
};
use protocol::{Envelope, Sequenced};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc as channel,
};
use tokio_tungstenite::WebSocketStream;
use types::{EventBody, Privacy, Role, Seen, SpaceKind, GENESIS_PREV};

type RelaySocket = WebSocketStream<tokio::net::TcpStream>;

#[test]
fn recovery_download_retries_follow_current_references_and_discard_obsolete_failures() {
    profile_clock_cases::recovery_download_retries_follow_current_references_and_discard_obsolete_failures();
}

#[test]
fn engine_lease_revocation_waits_for_current_work_and_finishes_storage_release() {
    lease_cases::engine_lease_revocation_waits_for_current_work_and_finishes_storage_release();
}

#[test]
fn poisoned_engine_lease_still_revokes_and_releases_storage() {
    lease_cases::poisoned_engine_lease_still_revokes_and_releases_storage();
}

#[test]
fn blocked_media_upload_releases_storage_and_preserves_ciphertext() {
    lease_cases::blocked_media_upload_releases_storage_and_preserves_ciphertext();
}

async fn request(socket: &mut RelaySocket) -> ClientFrame {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("client stalled")
            .expect("client closed")
            .expect("client socket");
        if let Message::Binary(bytes) = message {
            return ClientFrame::decode(&bytes).expect("client frame");
        }
    }
}

async fn reply(socket: &mut RelaySocket, frame: ServerFrame) {
    socket
        .send(Message::Binary(frame.encode().into()))
        .await
        .unwrap();
}

/// An authenticated socket that is waiting for the initial catch-up result.
async fn catch_up_socket(listener: &tokio::net::TcpListener) -> RelaySocket {
    let (socket, _) = listener.accept().await.unwrap();
    let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
    let ClientFrame::Hello { identity, .. } = request(&mut socket).await else {
        panic!("hello")
    };
    reply(
        &mut socket,
        ServerFrame::Challenge {
            nonce: "catch-up-retry".into(),
            relay: "catch-up-retry".into(),
            protocol: PROTOCOL_VERSION,
            capabilities: Vec::new(),
            server_time_ms: None,
        },
    )
    .await;
    assert!(matches!(
        request(&mut socket).await,
        ClientFrame::Auth { .. }
    ));
    reply(
        &mut socket,
        ServerFrame::Ready {
            identity,
            registered: true,
        },
    )
    .await;
    assert!(matches!(
        request(&mut socket).await,
        ClientFrame::Sync { all: true, .. }
    ));
    socket
}

#[tokio::test(flavor = "current_thread")]
async fn live_event_overtaking_final_replay_page_reconciles_once_without_later_message() {
    catchup_cases::live_event_overtaking_final_replay_page_reconciles_once_without_later_message()
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn initial_catch_up_unavailable_retries_without_resetting_backoff() {
    catchup_cases::initial_catch_up_unavailable_retries_without_resetting_backoff().await;
}

#[test]
fn catch_up_retry_keeps_handshake_refusals_terminal_and_rate_hints_bounded() {
    profile_clock_cases::catch_up_retry_keeps_handshake_refusals_terminal_and_rate_hints_bounded();
}

fn sequence(env: Envelope, head: &mut Option<Seen>) -> Sequenced {
    let seq = head.as_ref().map_or(0, |h| h.seq + 1);
    let prev = head
        .as_ref()
        .map_or_else(|| GENESIS_PREV.to_string(), |h| h.hash.clone());
    let hash = chain_hash(env.space(), seq, &prev, &env.wire_hash());
    *head = Some(Seen {
        seq,
        hash: hash.clone(),
    });
    Sequenced {
        seq,
        prev,
        hash,
        env,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn queued_messages_wait_for_catch_up_and_use_the_current_epoch() {
    queued_messages_round_trip(CatchUp::Initial).await;
}

#[tokio::test(flavor = "current_thread")]
async fn late_stale_rejections_preserve_resealed_messages_at_the_current_epoch() {
    queued_messages_round_trip(CatchUp::Incremental).await;
}

#[tokio::test(flavor = "current_thread")]
async fn handled_commit_collision_reconciles_without_listener_error() {
    queued_messages_round_trip(CatchUp::HandshakeCollision).await;
}
#[tokio::test(flavor = "current_thread")]
async fn terminal_application_refusal_keeps_failed_delivery_and_listener_notice() {
    queued_messages_round_trip(CatchUp::PermissionRefusal).await;
}
#[tokio::test(flavor = "current_thread")]
async fn handshake_storage_failure_notifies_and_retains_retry_before_reconciliation() {
    queued_messages_round_trip(CatchUp::StorageRefusal).await;
}
#[derive(Default)]
struct RejectionListener {
    errors: Mutex<Vec<String>>,
    changed: tokio::sync::Notify,
}
impl CoreListener for RejectionListener {
    fn on_change(&self, _: Vec<String>) {}
    fn on_ephemeral(&self, _: String, _: String, _: String, _: String) {}
    fn on_presence(&self, _: String, _: bool) {}
    fn on_connection(&self, _: crate::ConnectionDto) {}
    fn on_error(&self, message: String) {
        self.errors.lock().unwrap().push(message);
        self.changed.notify_one();
    }
    fn on_profile_changed(&self, _: String) {}
}
enum CatchUp {
    Initial,
    Incremental,
    HandshakeCollision,
    PermissionRefusal,
    StorageRefusal,
}

#[tokio::test(flavor = "current_thread")]
async fn a_control_notice_is_dispatched_between_slow_maintenance_passes() {
    ordering_cases::a_control_notice_is_dispatched_between_slow_maintenance_passes().await;
}

#[tokio::test(flavor = "current_thread")]
async fn member_seen_beyond_the_next_sequence_is_fetched_once_then_forks() {
    ordering_cases::member_seen_beyond_the_next_sequence_is_fetched_once_then_forks().await;
}
#[test]
fn superseded_profile_query_cannot_consume_current_batch_or_its_backoff() {
    profile_clock_cases::superseded_profile_query_cannot_consume_current_batch_or_its_backoff();
}

//! Lifecycle regressions for the private pickup owner.
use super::*;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Condvar,
    },
    task::Waker,
    time::Duration,
};

const REGISTERED_MODULE: &str = module_path!();

const BOUND: Duration = Duration::from_secs(3);

fn pending(relay: String) -> Result<Pending, CoreError> {
    let device = Signer::generate()?;
    let (secret, public) = crate::link::key_pair()?;
    Ok(Pending {
        device_secret: hex::encode(device.secret()),
        hpke_secret: hex::encode(secret),
        hpke_public: hex::encode(public),
        secret: hex::encode(crate::link::random::<32>()?),
        relay,
    })
}

fn budget() -> Duration {
    Duration::from_secs(10)
}

fn assert_pending<F: Future>(future: Pin<&mut F>) {
    let mut context = Context::from_waker(Waker::noop());
    assert!(future.poll(&mut context).is_pending());
}

fn assert_typed_cancel<T>(result: Result<T, CoreError>) {
    assert!(matches!(result, Err(CoreError::Invalid { .. })));
}

fn held(
    control: &PickupControl,
) -> std::result::Result<(PickupWaiter, oneshot::Receiver<PickupKey>), Box<dyn std::error::Error>> {
    let (send, receive) = oneshot::channel();
    let waiter = control
        .begin_with_factory(
            || Ok(pending("http://127.0.0.1:1".into())?),
            budget(),
            move |key, _, _, _| {
                Box::new(move || {
                    assert!(send.send(key).is_ok());
                    Box::pin(std::future::pending())
                })
            },
        )
        .unwrap();
    Ok((waiter, receive))
}

mod lifecycle_cases;
mod socket_cases;

#[tokio::test]
async fn retire_before_first_worker_poll_never_constructs_transport() {
    lifecycle_cases::retire_before_first_worker_poll_never_constructs_transport()
        .await
        .unwrap();
}

#[tokio::test]
async fn production_pickup_retired_before_first_poll_opens_no_tcp_connection() {
    lifecycle_cases::production_pickup_retired_before_first_poll_opens_no_tcp_connection()
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revoked_signing_handle_cannot_read_id_sign_or_install() {
    lifecycle_cases::revoked_signing_handle_cannot_read_id_sign_or_install()
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn finished_worker_keeps_admission_occupied_until_consumer_retires() {
    lifecycle_cases::finished_worker_keeps_admission_occupied_until_consumer_retires()
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abort_and_waiter_drop_do_not_fake_worker_drain() {
    lifecycle_cases::abort_and_waiter_drop_do_not_fake_worker_drain()
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn consumer_drop_cancels_an_actual_running_worker() {
    lifecycle_cases::consumer_drop_cancels_an_actual_running_worker()
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_panic_wakes_typed_waiter_and_retires_after_both_drops() {
    lifecycle_cases::worker_panic_wakes_typed_waiter_and_retires_after_both_drops()
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accepted_install_finishes_before_terminal_retire_returns() {
    lifecycle_cases::accepted_install_finishes_before_terminal_retire_returns()
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn panicking_install_fails_closed_without_forgetting_live_owners() {
    lifecycle_cases::panicking_install_fails_closed_without_forgetting_live_owners()
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_while_real_websocket_holds_box_has_no_late_install_or_retry() {
    socket_cases::cancellation_while_real_websocket_holds_box_has_no_late_install_or_retry()
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_hpke_box_arrival_does_not_override_later_install_revocation() {
    socket_cases::real_hpke_box_arrival_does_not_override_later_install_revocation()
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_stop_sync_cancels_real_pickup_preserving_pending_until_retry_after_drain() {
    socket_cases::client_stop_sync_cancels_real_pickup_preserving_pending_until_retry_after_drain()
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_erase_retires_held_pickup_deletes_pending_and_refuses_fresh_link_requests() {
    socket_cases::client_erase_retires_held_pickup_deletes_pending_and_refuses_fresh_link_requests(
    )
    .await
    .unwrap();
}

#[test]
fn spawning_on_an_actual_closed_runtime_cannot_deadlock_completion_under_the_gate() {
    socket_cases::spawning_on_an_actual_closed_runtime_cannot_deadlock_completion_under_the_gate()
        .unwrap();
}

#[tokio::test]
async fn agent_purpose_uses_distinct_mailbox_and_same_revocable_install_lease() {
    socket_cases::agent_purpose_uses_distinct_mailbox_and_same_revocable_install_lease()
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lost_transport_response_retries_the_original_pending_and_installs_only_once() {
    socket_cases::lost_transport_response_retries_the_original_pending_and_installs_only_once()
        .await
        .unwrap();
}

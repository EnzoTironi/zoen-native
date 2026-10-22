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

const BOUND: Duration = Duration::from_secs(3);

fn pending(relay: String) -> Pending {
    let device = Signer::generate();
    let (secret, public) = crate::link::key_pair();
    Pending {
        device_secret: hex::encode(device.secret()),
        hpke_secret: hex::encode(secret),
        hpke_public: hex::encode(public),
        secret: hex::encode(crate::link::random::<32>()),
        relay,
    }
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

fn held(control: &PickupControl) -> (PickupWaiter, oneshot::Receiver<PickupKey>) {
    let (send, receive) = oneshot::channel();
    let waiter = control
        .begin_with_factory(
            || Ok(pending("http://127.0.0.1:1".into())),
            budget(),
            move |key, _, _, _| {
                Box::new(move || {
                    assert!(send.send(key).is_ok());
                    Box::pin(std::future::pending())
                })
            },
        )
        .unwrap();
    (waiter, receive)
}

#[tokio::test]
async fn retire_before_first_worker_poll_never_constructs_transport() {
    let control = PickupControl::default();
    let constructed = Arc::new(AtomicUsize::new(0));
    let observed = constructed.clone();
    let mut waiter = control
        .begin_with_factory(
            || Ok(pending("http://127.0.0.1:1".into())),
            budget(),
            move |_, _, _, _| {
                Box::new(move || {
                    observed.fetch_add(1, Ordering::SeqCst);
                    Box::pin(std::future::pending())
                })
            },
        )
        .unwrap();
    // This single-thread runtime has not yielded since spawn: abort must also
    // retire a factory which never produced its concrete connect/TLS future.
    control.retire();
    assert_typed_cancel(waiter.wait().await);
    drop(waiter);
    tokio::time::timeout(BOUND, control.drain()).await.unwrap();
    assert_eq!(constructed.load(Ordering::SeqCst), 0);
    assert!(control.with_request(|| Ok(())).is_err());
}

#[tokio::test]
async fn production_pickup_retired_before_first_poll_opens_no_tcp_connection() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control = PickupControl::default();
    let mut waiter = control
        .begin(
            || {
                Ok(pending(format!(
                    "http://{}",
                    listener.local_addr().unwrap()
                )))
            },
            budget(),
            i18n::Lang::En,
        )
        .unwrap();
    control.retire();
    assert_typed_cancel(waiter.wait().await);
    drop(waiter);
    tokio::time::timeout(BOUND, control.drain()).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revoked_signing_handle_cannot_read_id_sign_or_install() {
    let control = PickupControl::default();
    let (mut waiter, started) = held(&control);
    let key = tokio::time::timeout(BOUND, started).await.unwrap().unwrap();
    let id = key.id().unwrap();
    assert!(event_log::verify_sig(
        &id,
        b"before revoke",
        &key.sign(b"before revoke").unwrap()
    ));
    control.cancel();
    assert!(key.check().is_err());
    assert!(key.id().is_err());
    assert!(key.sign(b"after revoke").is_err());
    let installed = AtomicBool::new(false);
    assert_typed_cancel(waiter.with_install(|_, _| {
        installed.store(true, Ordering::SeqCst);
        Ok(())
    }));
    assert!(!installed.load(Ordering::SeqCst));
    assert_typed_cancel(waiter.wait().await);
    drop(waiter);
    tokio::time::timeout(BOUND, control.drain()).await.unwrap();
    // stop_sync-style cancellation is retryable only after actual retirement.
    assert_eq!(control.with_request(|| Ok(7)).unwrap(), 7);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn finished_worker_keeps_admission_occupied_until_consumer_retires() {
    let control = PickupControl::default();
    let mut waiter = control
        .begin_with_factory(
            || Ok(pending("http://127.0.0.1:1".into())),
            budget(),
            |_, _, _, _| Box::new(|| Box::pin(async { Ok(vec![4, 5, 6]) })),
        )
        .unwrap();
    assert_eq!(
        tokio::time::timeout(BOUND, waiter.wait())
            .await
            .unwrap()
            .unwrap(),
        vec![4, 5, 6]
    );
    let writes = AtomicUsize::new(0);
    assert!(control
        .with_request(|| {
            writes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .is_err());
    assert!(control
        .begin_with_factory(
            || {
                writes.fetch_add(1, Ordering::SeqCst);
                Ok(pending("http://127.0.0.1:1".into()))
            },
            budget(),
            |_, _, _, _| Box::new(|| Box::pin(std::future::pending())),
        )
        .is_err());
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    let mut drain = Box::pin(control.drain());
    assert_pending(drain.as_mut());
    control.cancel();
    assert_typed_cancel(waiter.with_install(|_, _| {
        writes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }));
    drop(waiter);
    tokio::time::timeout(BOUND, drain).await.unwrap();
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert!(control.with_request(|| Ok(())).is_ok());
}

/// Blocks the ACTUAL inner Future destructor, rather than faking an unfinished
/// completion flag. Its bounded wait and independent release guard avoid a
/// failing assertion stranding a Tokio runtime thread forever.
struct DropGate {
    opened: Mutex<bool>,
    changed: Condvar,
    exited: AtomicBool,
}

impl DropGate {
    fn release(&self) {
        *self.opened.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

struct ReleaseOnDrop(Arc<DropGate>);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}

struct HeldDrop {
    gate: Arc<DropGate>,
    entered: Option<oneshot::Sender<()>>,
}

impl Future for HeldDrop {
    type Output = Result<Vec<u8>, String>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}

impl Drop for HeldDrop {
    fn drop(&mut self) {
        let _ = self.entered.take().unwrap().send(());
        let opened = self.gate.opened.lock().unwrap();
        let (opened, timeout) = self
            .gate
            .changed
            .wait_timeout_while(opened, BOUND, |value| !*value)
            .unwrap();
        drop(opened);
        self.gate.exited.store(true, Ordering::SeqCst);
        assert!(
            !timeout.timed_out(),
            "test failed to release the real child Drop"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abort_and_waiter_drop_do_not_fake_worker_drain() {
    let control = PickupControl::default();
    let gate = Arc::new(DropGate {
        opened: Mutex::new(false),
        changed: Condvar::new(),
        exited: AtomicBool::new(false),
    });
    let release = ReleaseOnDrop(gate.clone());
    let (started_send, started) = oneshot::channel();
    let (entered_send, entered) = oneshot::channel();
    let inner_gate = gate.clone();
    let waiter = control
        .begin_with_factory(
            || Ok(pending("http://127.0.0.1:1".into())),
            budget(),
            move |_, _, _, _| {
                Box::new(move || {
                    let _ = started_send.send(());
                    Box::pin(HeldDrop {
                        gate: inner_gate,
                        entered: Some(entered_send),
                    })
                })
            },
        )
        .unwrap();
    tokio::time::timeout(BOUND, started).await.unwrap().unwrap();
    control.cancel();
    drop(waiter);
    tokio::time::timeout(BOUND, entered).await.unwrap().unwrap();
    assert!(!gate.exited.load(Ordering::SeqCst));
    let mut drain = Box::pin(control.drain());
    assert_pending(drain.as_mut());
    let loaded = AtomicBool::new(false);
    assert!(control
        .with_request(|| {
            loaded.store(true, Ordering::SeqCst);
            Ok(())
        })
        .is_err());
    assert!(!loaded.load(Ordering::SeqCst));
    drop(release);
    tokio::time::timeout(BOUND, drain).await.unwrap();
    assert!(gate.exited.load(Ordering::SeqCst));
    assert!(control.with_request(|| Ok(())).is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn consumer_drop_cancels_an_actual_running_worker() {
    let control = PickupControl::default();
    let (waiter, started) = held(&control);
    let key = tokio::time::timeout(BOUND, started).await.unwrap().unwrap();
    drop(waiter);
    assert!(key.sign(b"late caller").is_err());
    tokio::time::timeout(BOUND, control.drain()).await.unwrap();
    assert!(control.with_request(|| Ok(())).is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_panic_wakes_typed_waiter_and_retires_after_both_drops() {
    let control = PickupControl::default();
    let mut waiter = control
        .begin_with_factory(
            || Ok(pending("http://127.0.0.1:1".into())),
            budget(),
            |_, _, _, _| Box::new(|| panic!("owned test worker panic")),
        )
        .unwrap();
    assert_typed_cancel(tokio::time::timeout(BOUND, waiter.wait()).await.unwrap());
    assert!(control.with_request(|| Ok(())).is_err());
    drop(waiter);
    tokio::time::timeout(BOUND, control.drain()).await.unwrap();
    assert!(control.with_request(|| Ok(())).is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accepted_install_finishes_before_terminal_retire_returns() {
    let control = PickupControl::default();
    let mut waiter = control
        .begin_with_factory(
            || Ok(pending("http://127.0.0.1:1".into())),
            budget(),
            |_, _, _, _| Box::new(|| Box::pin(async { Ok(vec![1]) })),
        )
        .unwrap();
    waiter.wait().await.unwrap();
    let (inside_send, inside) = oneshot::channel();
    let (release_send, release) = mpsc::channel();
    let installed = Arc::new(AtomicBool::new(false));
    let observed = installed.clone();
    let installer = std::thread::spawn(move || {
        waiter.with_install(|_, _| {
            let _ = inside_send.send(());
            release.recv_timeout(BOUND).unwrap();
            observed.store(true, Ordering::SeqCst);
            Ok(())
        })
    });
    tokio::time::timeout(BOUND, inside).await.unwrap().unwrap();
    assert!(matches!(
        control.0.state.try_lock(),
        Err(std::sync::TryLockError::WouldBlock)
    ));
    let retiring = control.clone();
    let (retired_send, retired) = mpsc::channel();
    let retire_thread = std::thread::spawn(move || {
        retiring.retire();
        let _ = retired_send.send(());
    });
    assert!(matches!(
        retired.recv_timeout(Duration::from_millis(40)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    release_send.send(()).unwrap();
    installer.join().unwrap().unwrap();
    retired.recv_timeout(BOUND).unwrap();
    retire_thread.join().unwrap();
    assert!(installed.load(Ordering::SeqCst));
    tokio::time::timeout(BOUND, control.drain()).await.unwrap();
    assert!(control.with_request(|| Ok(())).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn panicking_install_fails_closed_without_forgetting_live_owners() {
    let control = PickupControl::default();
    let mut waiter = control
        .begin_with_factory(
            || Ok(pending("http://127.0.0.1:1".into())),
            budget(),
            |_, _, _, _| Box::new(|| Box::pin(async { Ok(vec![1]) })),
        )
        .unwrap();
    waiter.wait().await.unwrap();
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = waiter.with_install::<()>(|_, _| panic!("owned test install panic"));
    }));
    assert!(panicked.is_err());
    assert_typed_cancel(waiter.with_install(|_, _| Ok(())));
    assert!(control.with_request(|| Ok(())).is_err());
    let mut drain = Box::pin(control.drain());
    assert_pending(drain.as_mut());
    drop(waiter);
    tokio::time::timeout(BOUND, drain).await.unwrap();
    // A poisoned install gate is terminal, unlike ordinary stop_sync cancel.
    assert!(control.with_request(|| Ok(())).is_err());
}

fn sealed_identity(pending: &Pending) -> Vec<u8> {
    let seed: [u8; 32] = hex::decode(&pending.device_secret)
        .unwrap()
        .try_into()
        .unwrap();
    let device = Signer::from_secret(&seed);
    let identity = Signer::generate();
    let (_, primary_hpke) = crate::link::key_pair();
    let boxed = crate::link::IdentityBox {
        identity_secret: hex::encode(identity.secret()),
        agreement_secret: hex::encode(crate::link::random::<32>()),
        cert: identity.sign(&event_log::device_cert_message(&device.id())),
        name: "Owned pickup regression".into(),
        handle: "pickup_regression".into(),
        relay: pending.relay.clone(),
        primary: Signer::generate().id(),
        primary_hpke: hex::encode(primary_hpke),
        profile_key: String::new(),
        profile_key_version: 0,
        profile_seen_version: 0,
        profile_fields: String::new(),
    };
    let secret: [u8; 32] = hex::decode(&pending.secret).unwrap().try_into().unwrap();
    crate::link::seal(
        &hex::decode(&pending.hpke_public).unwrap(),
        &secret,
        "identity",
        &serde_json::to_vec(&boxed).unwrap(),
    )
    .unwrap()
}

async fn pickup_peer(
    listener: tokio::net::TcpListener,
    box_id: String,
    fetched: oneshot::Sender<()>,
    response: oneshot::Receiver<Vec<u8>>,
) {
    use futures_util::{SinkExt, StreamExt};
    use protocol::{
        auth_message, ClientFrame, Op, Reply, ServerFrame, CAPABILITIES, PROTOCOL_VERSION,
    };
    use tokio_tungstenite::tungstenite::Message;

    let (stream, _) = tokio::time::timeout(BOUND, listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
    let Message::Binary(hello) = socket.next().await.unwrap().unwrap() else {
        panic!("binary Hello")
    };
    let ClientFrame::Hello {
        identity,
        device,
        cert,
        protocol,
        capabilities,
    } = ClientFrame::decode(&hello).unwrap()
    else {
        panic!("Hello")
    };
    assert_eq!(identity, device);
    assert_eq!(protocol, PROTOCOL_VERSION);
    assert_eq!(
        capabilities,
        CAPABILITIES
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    );
    assert!(event_log::verify_sig(
        &device,
        &event_log::device_cert_message(&device),
        &cert
    ));
    let nonce = "owned-pickup-regression-challenge";
    let relay = "owned-pickup-regression-relay";
    socket
        .send(Message::Binary(
            ServerFrame::Challenge {
                nonce: nonce.into(),
                relay: relay.into(),
                protocol: PROTOCOL_VERSION,
                capabilities: Vec::new(),
                server_time_ms: None,
            }
            .encode()
            .into(),
        ))
        .await
        .unwrap();
    let Message::Binary(auth) = socket.next().await.unwrap().unwrap() else {
        panic!("binary Auth")
    };
    let ClientFrame::Auth { sig } = ClientFrame::decode(&auth).unwrap() else {
        panic!("Auth")
    };
    assert!(event_log::verify_sig(
        &device,
        &auth_message(nonce, relay),
        &sig
    ));
    socket
        .send(Message::Binary(
            ServerFrame::Ready {
                identity,
                registered: false,
            }
            .encode()
            .into(),
        ))
        .await
        .unwrap();
    let Message::Binary(request) = socket.next().await.unwrap().unwrap() else {
        panic!("binary FetchLink")
    };
    let ClientFrame::Req {
        id: request_id,
        op: Op::FetchLink { id },
    } = ClientFrame::decode(&request).unwrap()
    else {
        panic!("FetchLink")
    };
    assert_eq!(request_id, 1);
    assert_eq!(id, box_id);
    let _ = fetched.send(());
    let response = tokio::time::timeout(BOUND, response)
        .await
        .unwrap()
        .unwrap();
    // Cancellation can already have closed this socket. A failed late send is
    // expected; a successful admitted TCP flush is not installation authority.
    let _ = socket
        .send(Message::Binary(
            ServerFrame::Res {
                id: request_id,
                result: Ok(Reply::Link(Some(response))),
            }
            .encode()
            .into(),
        ))
        .await;
    if let Ok(Some(Ok(Message::Binary(_)))) =
        tokio::time::timeout(Duration::from_millis(80), socket.next()).await
    {
        panic!("pickup emitted a new request after its real worker drain");
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_while_real_websocket_holds_box_has_no_late_install_or_retry() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let material = pending(format!("http://{}", listener.local_addr().unwrap()));
    let sealed = sealed_identity(&material);
    let secret: [u8; 32] = hex::decode(&material.secret).unwrap().try_into().unwrap();
    let (fetched_send, fetched) = oneshot::channel();
    let (response_send, response) = oneshot::channel();
    let peer = tokio::spawn(pickup_peer(
        listener,
        crate::link::box_id(&secret, "identity"),
        fetched_send,
        response,
    ));
    let control = PickupControl::default();
    let mut waiter = control
        .begin(|| Ok(material), budget(), i18n::Lang::En)
        .unwrap();
    tokio::time::timeout(BOUND, fetched).await.unwrap().unwrap();
    control.retire();
    assert_typed_cancel(waiter.wait().await);
    let installed = AtomicBool::new(false);
    assert_typed_cancel(waiter.with_install(|_, _| {
        installed.store(true, Ordering::SeqCst);
        Ok(())
    }));
    drop(waiter);
    tokio::time::timeout(BOUND, control.drain()).await.unwrap();
    assert!(response_send.send(sealed).is_ok());
    tokio::time::timeout(BOUND, peer).await.unwrap().unwrap();
    assert!(!installed.load(Ordering::SeqCst));
    assert!(control.with_request(|| Ok(())).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_hpke_box_arrival_does_not_override_later_install_revocation() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let material = pending(format!("http://{}", listener.local_addr().unwrap()));
    let sealed = sealed_identity(&material);
    let secret: [u8; 32] = hex::decode(&material.secret).unwrap().try_into().unwrap();
    let (fetched_send, fetched) = oneshot::channel();
    let (response_send, response) = oneshot::channel();
    let peer = tokio::spawn(pickup_peer(
        listener,
        crate::link::box_id(&secret, "identity"),
        fetched_send,
        response,
    ));
    let control = PickupControl::default();
    let mut waiter = control
        .begin(|| Ok(material), budget(), i18n::Lang::En)
        .unwrap();
    tokio::time::timeout(BOUND, fetched).await.unwrap().unwrap();
    assert!(response_send.send(sealed.clone()).is_ok());
    let bytes = tokio::time::timeout(BOUND, waiter.wait())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bytes, sealed);
    // Prove this is genuine decryptable device-link content, not fake bytes.
    waiter
        .with_install(|pending, device| {
            let secret: [u8; 32] = hex::decode(&pending.secret).unwrap().try_into().unwrap();
            let opened = crate::link::open(
                &hex::decode(&pending.hpke_secret).unwrap(),
                &secret,
                "identity",
                &bytes,
            )
            .unwrap();
            let boxed: crate::link::IdentityBox = serde_json::from_slice(&opened).unwrap();
            let identity_seed: [u8; 32] = hex::decode(boxed.identity_secret)
                .unwrap()
                .try_into()
                .unwrap();
            assert!(event_log::verify_sig(
                &Signer::from_secret(&identity_seed).id(),
                &event_log::device_cert_message(&device.id()),
                &boxed.cert
            ));
            Ok(())
        })
        .unwrap();
    let installed = AtomicBool::new(false);
    control.cancel();
    assert_typed_cancel(waiter.with_install(|_, _| {
        installed.store(true, Ordering::SeqCst);
        Ok(())
    }));
    assert!(!installed.load(Ordering::SeqCst));
    drop(waiter);
    tokio::time::timeout(BOUND, control.drain()).await.unwrap();
    tokio::time::timeout(BOUND, peer).await.unwrap().unwrap();
}

#[derive(Default)]
struct RecordingVault {
    rows: Mutex<std::collections::BTreeMap<String, Vec<u8>>>,
    saves: Mutex<Vec<String>>,
    deletes: Mutex<Vec<String>>,
}

impl crate::SecretVault for RecordingVault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.rows.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.saves.lock().unwrap().push(key.clone());
        self.rows.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.deletes.lock().unwrap().push(key.clone());
        self.rows.lock().unwrap().remove(&key);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_stop_sync_cancels_real_pickup_preserving_pending_until_retry_after_drain() {
    use crate::SecretVault;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!("http://{}", listener.local_addr().unwrap());
    let core = crate::Client::open(":memory:".into(), "en-US".into()).unwrap();
    let vault = Arc::new(RecordingVault::default());
    let initial = core.link_request(relay.clone(), vault.clone()).unwrap();
    let original_pending = vault.load(crate::link::VAULT_PENDING.into()).unwrap();
    let material: Pending = serde_json::from_slice(&original_pending).unwrap();
    let secret: [u8; 32] = hex::decode(&material.secret).unwrap().try_into().unwrap();
    let sealed = sealed_identity(&material);
    let (fetched_send, mut fetched) = oneshot::channel();
    let (response_send, response) = oneshot::channel();
    let peer = tokio::spawn(pickup_peer(
        listener,
        crate::link::box_id(&secret, "identity"),
        fetched_send,
        response,
    ));
    let mut wait = Box::pin(core.link_wait(vault.clone(), 5_000));
    tokio::select! {
        result = wait.as_mut() => panic!("pickup completed before held response: {}", result.is_ok()),
        result = &mut fetched => result.unwrap(),
        _ = tokio::time::sleep(BOUND) => panic!("actual Client did not issue FetchLink"),
    }
    core.stop_sync();
    // Do not poll the caller yet: its actual waiter lease is still alive.
    assert!(core.link_request(relay.clone(), vault.clone()).is_err());
    assert_eq!(
        vault.load(crate::link::VAULT_PENDING.into()).unwrap(),
        original_pending
    );
    assert_eq!(
        *vault.saves.lock().unwrap(),
        vec![crate::link::VAULT_PENDING.to_string()]
    );
    assert_typed_cancel(tokio::time::timeout(BOUND, wait.as_mut()).await.unwrap());
    drop(wait);
    tokio::time::timeout(BOUND, core.drain_link_pickup())
        .await
        .unwrap();
    assert!(response_send.send(sealed).is_ok());
    tokio::time::timeout(BOUND, peer).await.unwrap().unwrap();
    assert!(core.account().is_none());
    assert_eq!(vault.rows.lock().unwrap().len(), 1);
    let retry = core.link_request(relay, vault.clone()).unwrap();
    assert_ne!(retry.code, initial.code);
    assert_ne!(
        vault.load(crate::link::VAULT_PENDING.into()).unwrap(),
        original_pending
    );
    assert_eq!(vault.saves.lock().unwrap().len(), 2);
    assert!(vault.deletes.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_erase_retires_held_pickup_deletes_pending_and_refuses_fresh_link_requests() {
    use crate::SecretVault;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!("http://{}", listener.local_addr().unwrap());
    let core = crate::Client::open(":memory:".into(), "en-US".into()).unwrap();
    let vault = Arc::new(RecordingVault::default());
    core.link_request(relay.clone(), vault.clone()).unwrap();
    let material: Pending =
        serde_json::from_slice(&vault.load(crate::link::VAULT_PENDING.into()).unwrap()).unwrap();
    let secret: [u8; 32] = hex::decode(&material.secret).unwrap().try_into().unwrap();
    let sealed = sealed_identity(&material);
    let (fetched_send, mut fetched) = oneshot::channel();
    let (response_send, response) = oneshot::channel();
    let peer = tokio::spawn(pickup_peer(
        listener,
        crate::link::box_id(&secret, "identity"),
        fetched_send,
        response,
    ));
    let mut wait = Box::pin(core.link_wait(vault.clone(), 5_000));
    tokio::select! {
        result = wait.as_mut() => panic!("pickup completed before held response: {}", result.is_ok()),
        result = &mut fetched => result.unwrap(),
        _ = tokio::time::sleep(BOUND) => panic!("actual Client did not issue FetchLink"),
    }
    core.erase_device(vault.clone()).unwrap();
    assert!(vault.load(crate::link::VAULT_PENDING.into()).is_none());
    assert!(vault
        .deletes
        .lock()
        .unwrap()
        .iter()
        .any(|key| key == crate::link::VAULT_PENDING));
    assert!(core.account().is_none());
    assert!(core.link_request(relay.clone(), vault.clone()).is_err());
    assert_typed_cancel(tokio::time::timeout(BOUND, wait.as_mut()).await.unwrap());
    drop(wait);
    tokio::time::timeout(BOUND, core.drain_link_pickup())
        .await
        .unwrap();
    assert!(response_send.send(sealed).is_ok());
    tokio::time::timeout(BOUND, peer).await.unwrap().unwrap();
    assert!(core.account().is_none());
    assert!(vault.rows.lock().unwrap().is_empty());
    assert_eq!(
        *vault.saves.lock().unwrap(),
        vec![crate::link::VAULT_PENDING.to_string()]
    );
    assert!(core.link_request(relay, vault.clone()).is_err());
    assert!(core.link_wait(vault, 1_000).await.is_err());
}

#[test]
fn spawning_on_an_actual_closed_runtime_cannot_deadlock_completion_under_the_gate() {
    const CHILD: &str = "CLIENT_PICKUP_CLOSED_RUNTIME_REGRESSION_CHILD";
    const MARKER: &str = "actual_closed_runtime_child_completed";
    if std::env::var(CHILD).as_deref() == Ok("1") {
        let closed = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let handle = closed.handle().clone();
        drop(closed);
        let control = PickupControl::default();
        let constructed = Arc::new(AtomicUsize::new(0));
        let observed = constructed.clone();
        let enter = handle.enter();
        // Pinned Tokio bind_inner shuts an incoming task down synchronously
        // when the runtime is closed. Completion Drop must not reenter a held
        // pickup gate. A parent process deadline contains the old deadlock.
        let mut waiter = control
            .begin_with_factory(
                || Ok(pending("http://127.0.0.1:1".into())),
                budget(),
                move |_, _, _, _| {
                    Box::new(move || {
                        observed.fetch_add(1, Ordering::SeqCst);
                        Box::pin(std::future::pending())
                    })
                },
            )
            .unwrap();
        drop(enter);
        let live = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        live.block_on(async {
            assert_typed_cancel(waiter.wait().await);
            drop(waiter);
            tokio::time::timeout(BOUND, control.drain()).await.unwrap();
        });
        assert_eq!(constructed.load(Ordering::SeqCst), 0);
        println!("{MARKER}");
        return;
    }
    let test = format!(
        "{}::spawning_on_an_actual_closed_runtime_cannot_deadlock_completion_under_the_gate",
        module_path!()
    );
    let test = test.split_once("::").unwrap().1;
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env(CHILD, "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let bound = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().unwrap().is_some() {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "closed-runtime child failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                String::from_utf8_lossy(&output.stdout).contains(MARKER),
                "exact child test did not execute"
            );
            return;
        }
        if std::time::Instant::now() >= bound {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!(
                "closed-runtime spawn/completion deadlocked; owned child killed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[tokio::test]
async fn agent_purpose_uses_distinct_mailbox_and_same_revocable_install_lease() {
    let control = PickupControl::default();
    let material = pending("https://relay.example".into());
    let secret: [u8; 32] = hex::decode(&material.secret).unwrap().try_into().unwrap();
    let digest = "a".repeat(64);
    let context = format!("agent-enrollment-owner-v1:{digest}");
    let expected = crate::link::box_id(&secret, &context);
    assert_ne!(expected, crate::link::box_id(&secret, "identity"));
    let (send, receive) = oneshot::channel();
    let mut waiter = control
        .begin_with_purpose(
            || Ok(material),
            budget(),
            context,
            move |key, _, box_id, _| {
                assert_eq!(box_id, expected);
                Box::new(move || {
                    send.send(key).ok().unwrap();
                    Box::pin(std::future::pending())
                })
            },
        )
        .unwrap();
    let key = receive.await.unwrap();
    assert!(!control.cancel_request(|| Ok(false)).unwrap());
    assert!(key.check().is_ok());
    assert!(control.cancel_request(|| Ok(true)).unwrap());
    assert!(key.sign(b"revoked agent pickup").is_err());
    assert_typed_cancel(waiter.with_install(|_, _| Ok(())));
    assert_typed_cancel(waiter.wait().await);
    drop(waiter);
    tokio::time::timeout(BOUND, control.drain()).await.unwrap();
}

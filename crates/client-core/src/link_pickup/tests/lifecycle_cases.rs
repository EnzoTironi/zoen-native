//! Complete original test bodies; registration remains in the parent.
use super::*;

pub(super) async fn retire_before_first_worker_poll_never_constructs_transport(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let control = PickupControl::default();
    let constructed = Arc::new(AtomicUsize::new(0));
    let observed = constructed.clone();
    let mut waiter = control
        .begin_with_factory(
            || pending("http://127.0.0.1:1".into()),
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

    Ok(())
}

pub(super) async fn production_pickup_retired_before_first_poll_opens_no_tcp_connection(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control = PickupControl::default();
    let mut waiter = control
        .begin(
            || pending(format!("http://{}", listener.local_addr().unwrap())),
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

    Ok(())
}

pub(super) async fn revoked_signing_handle_cannot_read_id_sign_or_install(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let control = PickupControl::default();
    let (mut waiter, started) = held(&control)?;
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

    Ok(())
}

pub(super) async fn finished_worker_keeps_admission_occupied_until_consumer_retires(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let control = PickupControl::default();
    let mut waiter = control
        .begin_with_factory(
            || pending("http://127.0.0.1:1".into()),
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
                pending("http://127.0.0.1:1".into())
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

    Ok(())
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

pub(super) async fn abort_and_waiter_drop_do_not_fake_worker_drain(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
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
            || pending("http://127.0.0.1:1".into()),
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

    Ok(())
}

pub(super) async fn consumer_drop_cancels_an_actual_running_worker(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let control = PickupControl::default();
    let (waiter, started) = held(&control)?;
    let key = tokio::time::timeout(BOUND, started).await.unwrap().unwrap();
    drop(waiter);
    assert!(key.sign(b"late caller").is_err());
    tokio::time::timeout(BOUND, control.drain()).await.unwrap();
    assert!(control.with_request(|| Ok(())).is_ok());

    Ok(())
}

pub(super) async fn worker_panic_wakes_typed_waiter_and_retires_after_both_drops(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let control = PickupControl::default();
    let mut waiter = control
        .begin_with_factory(
            || pending("http://127.0.0.1:1".into()),
            budget(),
            |_, _, _, _| Box::new(|| panic!("owned test worker panic")),
        )
        .unwrap();
    assert_typed_cancel(tokio::time::timeout(BOUND, waiter.wait()).await.unwrap());
    assert!(control.with_request(|| Ok(())).is_err());
    drop(waiter);
    tokio::time::timeout(BOUND, control.drain()).await.unwrap();
    assert!(control.with_request(|| Ok(())).is_ok());

    Ok(())
}

pub(super) async fn accepted_install_finishes_before_terminal_retire_returns(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let control = PickupControl::default();
    let mut waiter = control
        .begin_with_factory(
            || pending("http://127.0.0.1:1".into()),
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

    Ok(())
}

pub(super) async fn panicking_install_fails_closed_without_forgetting_live_owners(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let control = PickupControl::default();
    let mut waiter = control
        .begin_with_factory(
            || pending("http://127.0.0.1:1".into()),
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

    Ok(())
}

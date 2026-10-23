//! Preserved transport deadlines scenarios; stable test entries stay in the parent.
use super::*;

pub(super) async fn the_actual_dispatch_guard_cancels_a_stalled_await_and_drops_its_owner() {
    struct Held(Arc<AtomicUsize>);
    impl Drop for Held {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let started = Arc::new(AtomicUsize::new(0));
    let held = Held(drops.clone());
    let (deadline, receiver) = watch::channel(None);
    let observed = started.clone();
    let work = async move {
        let _held = held;
        observed.fetch_add(1, Ordering::SeqCst);
        let _ = deadline.send_replace(Some(
            tokio::time::Instant::now() + Duration::from_millis(20),
        ));
        // Models any dispatcher write/flush future that never yields a result.
        std::future::pending::<Exit>().await
    };
    let (exit, expired) = tokio::time::timeout(
        Duration::from_secs(1),
        super::super::super::cursor_repair::drive(work, receiver),
    )
    .await
    .expect("the deadline guard must run outside the suspended dispatcher");
    assert!(matches!(exit, Exit::Retry(reason) if reason == "cursor reconciliation timed out"));
    assert!(expired);
    assert_eq!(started.load(Ordering::SeqCst), 1);
    assert_eq!(
        drops.load(Ordering::SeqCst),
        1,
        "the suspended owner's future was dropped exactly once"
    );
}

pub(super) fn a_ready_work_exit_cannot_reenter_socket_close_after_the_repair_budget() {
    let at = tokio::time::Instant::now();
    let policy = policy();
    let mut repair = CursorRepair::new(policy, at);
    // An unfinished initial catch-up also drops rather than awaits socket close.
    assert!(repair.abandon_transport(false, at));
    repair.completed(at);
    let until = at + policy.interval + policy.timeout;
    // Deterministically model work winning the fair select, so expired=false
    // and begin() has not run, while the watchdog can also already be ready.
    assert!(!repair.in_flight());
    assert!(!repair.abandon_transport(false, until - Duration::from_nanos(1)));
    assert!(repair.abandon_transport(false, until));
    assert!(repair.abandon_transport(false, until + Duration::from_nanos(1)));
    assert!(repair.abandon_transport(true, at));
    // If that work processed the real full SyncDone instead, the new deadline
    // is authoritative; an obsolete old timer must not abort the refreshed scope.
    assert!(repair.begin(at + policy.interval));
    assert!(repair.abandon_transport(false, at + policy.interval));
    repair.completed(until + Duration::from_nanos(1));
    assert!(!repair.in_flight());
    assert_eq!(
        repair.abort_deadline(),
        Some(until + Duration::from_nanos(1) + policy.interval + policy.timeout)
    );
    assert!(!repair.abandon_transport(false, until + Duration::from_nanos(1)));
}

pub(super) async fn a_close_started_before_the_deadline_cannot_outlive_the_same_repair_budget() {
    struct Held(Arc<AtomicUsize>);
    impl Drop for Held {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let starts = Arc::new(AtomicUsize::new(0));
    let held = Held(drops.clone());
    let observed = starts.clone();
    let until = tokio::time::Instant::now() + Duration::from_millis(20);
    let close = async move {
        let _held = held;
        observed.fetch_add(1, Ordering::SeqCst);
        std::future::pending::<()>().await
    };
    tokio::time::timeout(
        Duration::from_secs(1),
        super::super::super::cursor_repair::close_transport(close, Some(until), false),
    )
    .await
    .expect("post-dispatch close must remain inside the existing repair budget");
    assert_eq!(
        starts.load(Ordering::SeqCst),
        1,
        "the close really began before expiry"
    );
    assert_eq!(
        drops.load(Ordering::SeqCst),
        1,
        "expiry drops its exact transport owner once"
    );
}

pub(super) async fn expired_budget_and_stop_never_poll_another_transport_close() {
    struct Held(Arc<AtomicUsize>);
    impl Drop for Held {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let now = tokio::time::Instant::now();
    for (deadline, abandon) in [
        (Some(now - Duration::from_millis(1)), false),
        (Some(now + Duration::from_secs(60)), true),
        (None, true),
    ] {
        let drops = Arc::new(AtomicUsize::new(0));
        let starts = Arc::new(AtomicUsize::new(0));
        let held = Held(drops.clone());
        let observed = starts.clone();
        let close = async move {
            let _held = held;
            observed.fetch_add(1, Ordering::SeqCst);
            std::future::pending::<()>().await
        };
        tokio::time::timeout(
            Duration::from_secs(1),
            super::super::super::cursor_repair::close_transport(close, deadline, abandon),
        )
        .await
        .expect("Stop or an expired budget cannot start another write");
        assert_eq!(starts.load(Ordering::SeqCst), 0);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
}

pub(super) async fn legitimate_full_completion_retargets_the_cleanup_budget_before_close() {
    let now = tokio::time::Instant::now();
    let policy = policy();
    let old = now - policy.interval - policy.timeout - Duration::from_millis(10);
    let mut repair = CursorRepair::new(policy, old);
    repair.completed(old);
    assert!(repair.begin(old + policy.interval));
    let obsolete = repair.abort_deadline().unwrap();
    assert!(obsolete < now);
    repair.completed(now);
    let current = repair.abort_deadline();
    assert_eq!(current, Some(now + policy.interval + policy.timeout));
    let abandon = repair.abandon_transport(false, now);
    assert!(!abandon);
    let starts = Arc::new(AtomicUsize::new(0));
    let observed = starts.clone();
    super::super::super::cursor_repair::close_transport(
        async move {
            observed.fetch_add(1, Ordering::SeqCst);
        },
        current,
        abandon,
    )
    .await;
    assert_eq!(
        starts.load(Ordering::SeqCst),
        1,
        "current full completion permits the normal ready close"
    );
}

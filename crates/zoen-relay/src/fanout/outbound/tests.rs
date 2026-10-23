use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn frame(bytes: usize) -> ServerFrame {
    ServerFrame::DeviceMessage {
        from: "from".into(),
        to: "to".into(),
        sealed: vec![7; bytes],
    }
}

#[tokio::test]
async fn byte_pressure_rejects_before_count_capacity_and_never_evicts_an_admitted_frame() {
    let (tx, mut rx) = mpsc::channel(65_536);
    let budget = Arc::new(Semaphore::new(1024 * 1024));
    let payload = frame(256 * 1024);
    let mut accepted = 0;
    while enqueue(&tx, &budget, &["recipient".into()], &payload, Vec::new) {
        accepted += 1;
    }
    assert_eq!(accepted, 3);
    assert_eq!(tx.capacity(), 65_536 - accepted);
    assert!(budget.available_permits() < 256 * 1024);
    for _ in 0..accepted {
        let item = rx.recv().await.unwrap();
        assert_eq!(item.to, ["recipient"]);
        assert_eq!(ServerFrame::decode(&item.bytes).unwrap(), payload);
        drop(item);
    }
    assert_eq!(budget.available_permits(), 1024 * 1024);
}

#[tokio::test]
async fn an_active_publish_remains_charged_after_dequeue_until_its_owner_retires() {
    let (tx, mut rx) = mpsc::channel(2);
    let budget = Arc::new(Semaphore::new(1024));
    assert!(enqueue(
        &tx,
        &budget,
        &["alice".into()],
        &frame(100),
        Vec::new
    ));
    let queued = budget.available_permits();
    let active = rx.recv().await.unwrap();
    assert_eq!(
        budget.available_permits(),
        queued,
        "dequeue must not retire byte credits"
    );
    assert_eq!(
        tx.capacity(),
        2,
        "count capacity is already free while byte ownership remains"
    );
    let (started, observing) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _owned = active;
        started.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    observing.await.unwrap();
    assert_eq!(budget.available_permits(), queued);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(
        budget.available_permits(),
        1024,
        "cancellation retires the active payload"
    );
}

#[test]
fn queued_payloads_release_credits_on_receiver_drop_and_refusals_do_not_leak_slots() {
    let (tx, rx) = mpsc::channel(2);
    let budget = Arc::new(Semaphore::new(1024));
    assert!(!enqueue(
        &tx,
        &budget,
        &["alice".repeat(1000)],
        &frame(0),
        Vec::new
    ));
    assert_eq!(tx.capacity(), 2);
    assert_eq!(budget.available_permits(), 1024);
    assert!(enqueue(
        &tx,
        &budget,
        &["alice".into()],
        &frame(10),
        Vec::new
    ));
    assert!(budget.available_permits() < 1024);
    drop(rx);
    assert_eq!(budget.available_permits(), 1024);
}

#[test]
fn both_recipient_and_trace_bytes_are_charged_before_retention() {
    let (tx, rx) = mpsc::channel(2);
    let budget = Arc::new(Semaphore::new(1024));
    assert!(!enqueue(&tx, &budget, &["a".into()], &frame(0), || vec![(
        "trace".into(),
        "x".repeat(1024)
    )]));
    assert!(!enqueue(
        &tx,
        &budget,
        &vec!["a".into(); 100],
        &frame(0),
        Vec::new
    ));
    assert_eq!(budget.available_permits(), 1024);
    assert_eq!(tx.capacity(), 2);
    drop(rx);
}

#[test]
fn count_or_exhausted_byte_capacity_refuses_before_trace_capture() {
    let calls = AtomicUsize::new(0);
    let trace = || {
        calls.fetch_add(1, Ordering::Relaxed);
        Vec::new()
    };
    let (tx, _rx) = mpsc::channel(1);
    let budget = Arc::new(Semaphore::new(1024));
    let occupied = tx.try_reserve().unwrap();
    assert!(!enqueue(&tx, &budget, &["a".into()], &frame(0), trace));
    drop(occupied);
    let exhausted = budget.clone().try_acquire_many_owned(1024).unwrap();
    assert!(!enqueue(&tx, &budget, &["a".into()], &frame(0), trace));
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    drop(exhausted);
    assert!(enqueue(&tx, &budget, &["a".into()], &frame(0), trace));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

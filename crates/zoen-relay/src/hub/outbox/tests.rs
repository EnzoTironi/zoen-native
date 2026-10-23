use super::*;

fn frame(size: usize) -> ServerFrame {
    ServerFrame::DeviceMessage {
        from: "sender".into(),
        to: "recipient".into(),
        sealed: vec![7; size],
    }
}

fn charge(bytes: &Bytes) -> usize {
    bytes.len() + std::mem::size_of::<Envelope>()
}

#[tokio::test]
async fn bytes_refuse_before_frame_count_and_preserve_accepted_wire_payloads() {
    let bytes = Bytes::from(frame(256 * 1024).encode());
    let limit = charge(&bytes) * 2;
    let kick = Arc::new(Notify::new());
    let (out, mut rx) = channel_with_limits(1024, limit, kick.clone());
    for _ in 0..2 {
        out.try_send_encoded(|| bytes.clone()).unwrap();
    }
    assert!(matches!(
        out.try_send_encoded(|| panic!("zero credit must refuse before encoding")),
        Err(Refusal::Full(()))
    ));
    assert_eq!(out.tx.capacity(), 1022);
    assert_eq!(out.budget.available_permits(), 0);
    tokio::time::timeout(std::time::Duration::from_millis(100), kick.notified())
        .await
        .unwrap();
    for _ in 0..2 {
        let active = rx.recv().await.unwrap();
        assert_eq!(active.bytes, bytes);
        drop(active);
    }
    assert!(rx.try_recv().is_err());
    assert_eq!(out.budget.available_permits(), limit);
}

#[tokio::test]
async fn active_writer_keeps_credits_until_cancelled_task_has_retired() {
    let bytes = Bytes::from(frame(1024).encode());
    let limit = charge(&bytes);
    let (out, mut rx) = channel_with_limits(8, limit, Arc::new(Notify::new()));
    out.try_send_encoded(|| bytes.clone()).unwrap();
    let (active, observed) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let frame = rx.recv().await.unwrap();
        active.send(()).unwrap();
        std::future::pending::<()>().await;
        drop(frame);
    });
    observed.await.unwrap();
    assert_eq!(out.tx.capacity(), 8);
    assert_eq!(out.budget.available_permits(), 0);
    assert!(matches!(
        out.try_send_encoded(|| bytes),
        Err(Refusal::Full(()))
    ));
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(out.budget.available_permits(), limit);
}

#[tokio::test]
async fn receiver_drop_and_refused_oversize_release_all_credits_and_slots() {
    let bytes = Bytes::from(frame(1024).encode());
    let limit = charge(&bytes);
    let (out, rx) = channel_with_limits(2, limit, Arc::new(Notify::new()));
    assert!(out.send(frame(limit * 2)).await.is_err());
    assert_eq!(out.tx.capacity(), 2);
    assert_eq!(out.budget.available_permits(), limit);
    out.send(frame(1024)).await.unwrap();
    assert_eq!(out.budget.available_permits(), 0);
    drop(rx);
    assert_eq!(out.budget.available_permits(), limit);
    assert!(matches!(
        out.try_send_encoded(|| panic!("closed receiver must refuse before encoding")),
        Err(Refusal::Closed(()))
    ));
}

#[tokio::test]
async fn count_backpressure_cancellation_never_reserves_extra_bytes() {
    let bytes = Bytes::from(frame(1024).encode());
    let limit = charge(&bytes) * 2;
    let (out, mut rx) = channel_with_limits(1, limit, Arc::new(Notify::new()));
    out.send(frame(1024)).await.unwrap();
    assert!(matches!(
        out.try_send_encoded(|| panic!("full count must refuse before encoding")),
        Err(Refusal::Full(()))
    ));
    let sender = out.clone();
    let mut waiting = Box::pin(sender.send(frame(2048)));
    assert!(matches!(
        futures_util::poll!(&mut waiting),
        std::task::Poll::Pending
    ));
    assert_eq!(out.budget.available_permits(), charge(&bytes));
    drop(waiting);
    drop(rx.recv().await.unwrap());
    assert_eq!(out.budget.available_permits(), limit);
    out.send(frame(1024)).await.unwrap();
}

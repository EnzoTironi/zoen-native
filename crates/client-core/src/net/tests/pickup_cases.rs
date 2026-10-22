use super::*;

pub(super) async fn pickup_expired_deadline_opens_no_connection() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let result = fetch_link_box(
        &format!("http://{}", listener.local_addr().unwrap()),
        &Signer::generate(),
        "fixture",
        tokio::time::Instant::now(),
    )
    .await;
    assert!(result.unwrap_err().contains("prazo"));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}

pub(super) async fn pickup_deadline_bounds_a_stalled_connection() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(80);
    let result = fetch_link_box(
        &format!("http://{address}"),
        &Signer::generate(),
        "fixture",
        deadline,
    )
    .await;
    assert!(result.unwrap_err().contains("prazo"));
    assert!(tokio::time::Instant::now() < deadline + Duration::from_millis(500));
}

pub(super) async fn pickup_reconnects_after_transport_failure_and_is_cancelable() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        fetch_link_box(
            &format!("http://{address}"),
            &Signer::generate(),
            "fixture",
            tokio::time::Instant::now() + Duration::from_secs(5),
        )
        .await
    });
    let (first, _) = listener.accept().await.unwrap();
    drop(first);
    let (second, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
        .await
        .unwrap()
        .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    drop(second);
    assert!(
        tokio::time::timeout(Duration::from_millis(900), listener.accept())
            .await
            .is_err()
    );
}

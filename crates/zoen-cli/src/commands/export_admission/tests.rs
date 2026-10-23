use super::{
    export_all,
    test_fixture::{Fixture, MARKDOWN},
};
use std::{future::Future, task::Poll, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn original_budget_waits_for_an_ack_after_initial_ready_before_exporting() {
    let fixture = Fixture::new().await;
    fixture.hold_top_up().await;
    assert!(tokio::time::Instant::now() < fixture.deadline);
    let output = fixture.directory.join("export");
    let mut admission = Box::pin(export_all(
        &fixture.client,
        fixture.space.clone(),
        output.clone(),
        5000,
        false,
        fixture.deadline,
    ));
    // Poll the actual production export branch exactly once, with the ACK held.
    // The legacy zero-budget recheck returns Err now even though the original
    // command deadline is still in the future. No timed scheduling guess.
    std::future::poll_fn(|cx| match admission.as_mut().poll(cx) {
        Poll::Pending => Poll::Ready(()),
        Poll::Ready(result) => panic!("export abandoned its remaining original budget: {result:?}"),
    })
    .await;
    assert!(!output.exists(), "nothing is written before MLS is ready");
    fixture.release_top_up().await;
    let summary = tokio::time::timeout(Duration::from_secs(5), admission)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        summary,
        format!("1 pages, {} bytes, 0 still arriving", MARKDOWN.len())
    );
    assert_eq!(
        std::fs::read(output.join("page.md")).unwrap(),
        MARKDOWN.as_bytes()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_ack_after_the_original_deadline_cannot_write_or_restart_export() {
    let fixture = Fixture::new().await;
    fixture.hold_top_up().await;
    let output = fixture.directory.join("late");
    let deadline = tokio::time::Instant::now() + Duration::from_millis(120);
    let export = export_all(
        &fixture.client,
        fixture.space.clone(),
        output.clone(),
        5000,
        false,
        deadline,
    );
    let late_ack = async {
        tokio::time::sleep_until(deadline + Duration::from_millis(10)).await;
        fixture.release_top_up().await;
    };
    let (result, ()) = tokio::join!(export, late_ack);
    assert!(result.is_err());
    assert!(!output.exists());
    fixture.client.wait_until_settled(1000).await.unwrap();
    assert!(
        !output.exists(),
        "a later ACK cannot resume a failed export"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_ack_keeps_mls_guard_and_writes_zero_files() {
    let fixture = Fixture::new().await;
    fixture.hold_top_up().await;
    let output = fixture.directory.join("missing-ack");
    let deadline = tokio::time::Instant::now() + Duration::from_millis(120);
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        export_all(
            &fixture.client,
            fixture.space.clone(),
            output.clone(),
            5000,
            false,
            deadline,
        ),
    )
    .await
    .expect("the per-page timeout cannot replace the remaining startup budget");
    assert!(result.is_err());
    assert!(!output.exists());
    assert!(fixture.client.wait_until_settled(0).await.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_expired_original_budget_cannot_admit_even_an_already_ready_client() {
    let fixture = Fixture::new().await;
    fixture.client.wait_until_settled(0).await.unwrap();
    let output = fixture.directory.join("expired");
    let result = export_all(
        &fixture.client,
        fixture.space.clone(),
        output.clone(),
        5000,
        false,
        tokio::time::Instant::now(),
    )
    .await;
    assert!(result.is_err());
    assert!(!output.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_offline_export_remains_a_separate_cached_snapshot() {
    let fixture = Fixture::new().await;
    fixture.hold_top_up().await;
    let output = fixture.directory.join("cache");
    let result = export_all(
        &fixture.client,
        fixture.space.clone(),
        output.clone(),
        5000,
        true,
        tokio::time::Instant::now(),
    )
    .await
    .unwrap();
    assert_eq!(
        result,
        format!("1 pages, {} bytes, 0 still arriving", MARKDOWN.len())
    );
    assert_eq!(
        std::fs::read(output.join("page.md")).unwrap(),
        MARKDOWN.as_bytes()
    );
    assert!(
        fixture.client.wait_until_settled(0).await.is_err(),
        "offline export never grants MLS readiness"
    );
}

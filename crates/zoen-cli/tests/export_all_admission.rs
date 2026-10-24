//! A successful online export must not mistake an incomplete local projection
//! for a caught-up folder. No PostgreSQL, FDB, providers or retained homes.
mod export_fixture;

use export_fixture::Fixture;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn online_export_refuses_an_unknown_page_before_sync_done(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new().await?;
    fixture.assert_cached_subset();
    let output = fixture.export("incomplete", false).await;
    fixture.assert_sync_without_completion();
    assert!(
        !output.status.success(),
        "an incomplete online projection must fail, not claim completion: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("export-all:"));
    assert!(!fixture.output("incomplete").exists());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn caught_up_export_includes_the_previously_unknown_page_exactly(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new().await?;
    fixture.assert_cached_subset();
    fixture.release();
    let output = fixture.export("complete", false).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("2 pages,"));
    assert!(String::from_utf8_lossy(&output.stdout).ends_with("0 still arriving\n"));
    fixture.assert_exact_export("complete", true);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_offline_export_keeps_the_cached_subset_without_connecting(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new().await?;
    let output = fixture.export("cached", true).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("1 pages,"));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("offline cache export: remote catch-up was not checked"));
    fixture.assert_exact_export("cached", false);
    fixture.assert_no_connection();
    Ok(())
}

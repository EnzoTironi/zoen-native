use super::super::MediaDownloadRetries;
use std::time::Duration;
use tokio::time::Instant;

#[test]
fn exhausted_media_retry_counter_keeps_background_deadline_and_explicit_completion() {
    let now = Instant::now();
    let mut retries = MediaDownloadRetries::default();
    retries.failed("fresh", now);
    assert!(!retries.ready("fresh", false, now + Duration::from_secs(1)));
    assert!(retries.ready("fresh", false, now + Duration::from_secs(2)));

    retries.0.insert("exhausted".into(), (now, u32::MAX));
    retries.failed("exhausted", now);
    assert!(retries.ready("exhausted", true, now));
    assert!(!retries.ready("exhausted", false, now + Duration::from_secs(255)));
    assert!(retries.ready("exhausted", false, now + Duration::from_secs(256)));
    assert!(retries.ready("independent", false, now));

    let later = now + Duration::from_secs(256);
    retries.failed("exhausted", later);
    assert!(!retries.ready("exhausted", false, later + Duration::from_secs(255)));
    assert!(retries.ready("exhausted", false, later + Duration::from_secs(256)));
    retries.completed("exhausted");
    assert!(retries.ready("exhausted", false, later));
    retries.failed("exhausted", later);
    assert!(!retries.ready("exhausted", false, later + Duration::from_secs(1)));
    assert!(retries.ready("exhausted", false, later + Duration::from_secs(2)));
}

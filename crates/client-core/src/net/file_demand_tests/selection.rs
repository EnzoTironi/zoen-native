//! Historical download selection contracts using the shared real HTTP fixture.
use super::*;

#[tokio::test]
async fn historical_file_demand_downloads_only_selected_version_and_coalesces_waiters() {
    let f = Fixture::new();
    let mut server = BlobServer::start(f.uploads(), f.old_blob.clone(), false);
    let kick = Arc::new(tokio::sync::Notify::new());
    let task = worker(&f.reader, &server, kick.clone());
    until(|| {
        f.reader
            .file_info_at(f.item.clone(), Some(2))
            .unwrap()
            .ready
    })
    .await;
    assert_eq!(
        f.reader
            .file_bytes_bounded(f.item.clone(), Some(1), 1024)
            .unwrap(),
        None
    );
    assert_eq!(f.reader.item(f.item.clone()).unwrap().version, 2);
    let before = (
        f.reader.item(f.item.clone()).unwrap(),
        f.reader.timeline(f.space.clone()).unwrap(),
        outbox(&f.reader),
    );
    let first = ensure(&f.reader, &f.item, HTTP_JOURNEY_TIMEOUT_MS);
    let second = ensure(&f.reader, &f.item, HTTP_JOURNEY_TIMEOUT_MS);
    until(|| f.reader.lock().net.file_demands.waiter_count() == 2).await;
    assert_eq!(f.reader.lock().net.file_demands.active().unwrap().len(), 1);
    kick.notify_one();
    until(|| server.requests.lock().unwrap().contains(&f.old_blob)).await;
    first.abort();
    let _ = first.await;
    assert_eq!(
        f.reader.lock().net.file_demands.active().unwrap().len(),
        1,
        "one canceled waiter cannot cancel another"
    );
    server.allow_old.store(true, Ordering::SeqCst);
    let ready = second.await.unwrap().unwrap();
    assert!(ready.ready);
    assert_eq!(ready.sha256, hex::encode(Sha256::digest(&f.old)));
    assert_eq!(
        f.reader
            .file_bytes_bounded(f.item.clone(), Some(1), 1024)
            .unwrap(),
        Some(f.old)
    );
    assert_eq!(
        (
            f.reader.item(f.item.clone()).unwrap(),
            f.reader.timeline(f.space).unwrap(),
            outbox(&f.reader)
        ),
        before
    );
    assert!(f
        .reader
        .lock()
        .net
        .file_demands
        .active()
        .unwrap()
        .is_empty());
    {
        let requests = server.requests.lock().unwrap();
        assert_eq!(
            requests.iter().filter(|blob| *blob == &f.old_blob).count(),
            1
        );
        assert!(
            !requests.contains(&f.unrelated_old_blob),
            "never eagerly fetch unrelated historical bytes"
        );
    }
    task.abort();
    let _ = task.await;
    server
        .stop()
        .expect("fixture HTTP worker must finish cleanly");
}

#[tokio::test]
async fn mixed_historical_downloads_preserve_item_routing_and_retry_after_corruption() {
    for corrupt_first in [true, false] {
        let f = Fixture::new();
        let other = f
            .reader
            .items()
            .into_iter()
            .find(|item| item.id != f.item)
            .expect("second signed file");
        let ids = [f.item.clone(), other.id];
        let bodies = [f.old.clone(), b"unrelated old bytes".to_vec()];
        let bad = usize::from(!corrupt_first);
        let good = 1 - bad;
        let bad_blob = if bad == 0 {
            f.old_blob.clone()
        } else {
            f.unrelated_old_blob.clone()
        };
        let mut corrupt = f.uploads();
        corrupt.get_mut(&bad_blob).unwrap()[0] ^= 1;
        let mut server = BlobServer::start(corrupt, String::new(), false);
        let kick = Arc::new(tokio::sync::Notify::new());
        let task = worker(&f.reader, &server, kick.clone());
        until(|| {
            ids.iter()
                .all(|id| f.reader.file_info_at(id.clone(), Some(2)).unwrap().ready)
        })
        .await;
        for id in &ids {
            assert!(!f.reader.file_info_at(id.clone(), Some(1)).unwrap().ready);
        }
        let before = (
            ids.iter()
                .map(|id| f.reader.item(id.clone()).unwrap())
                .collect::<Vec<_>>(),
            f.reader.timeline(f.space.clone()).unwrap(),
            outbox(&f.reader),
        );
        let first = ensure(&f.reader, &ids[0], HTTP_JOURNEY_TIMEOUT_MS);
        let second = ensure(&f.reader, &ids[1], HTTP_JOURNEY_TIMEOUT_MS);
        until(|| f.reader.lock().net.file_demands.waiter_count() == 2).await;
        let admitted_blobs: Vec<_> = f
            .reader
            .lock()
            .wanted_media()
            .into_iter()
            .map(|w| w.blob)
            .collect();
        assert!(admitted_blobs.contains(&f.old_blob), "old blob absent before wake; corrupt_first={corrupt_first}; admitted={admitted_blobs:?}");
        assert!(admitted_blobs.contains(&f.unrelated_old_blob), "neighbor blob absent before wake; corrupt_first={corrupt_first}; admitted={admitted_blobs:?}");
        kick.notify_one();
        let results = [first.await.unwrap(), second.await.unwrap()];
        let ready = results[good].as_ref().expect("unrelated version succeeds");
        assert!(ready.ready);
        assert_eq!(ready.sha256, hex::encode(Sha256::digest(&bodies[good])));
        assert!(matches!(results[bad], Err(CoreError::Invalid { .. })));
        assert_eq!(
            f.reader
                .file_bytes_bounded(ids[good].clone(), Some(1), 1024)
                .unwrap(),
            Some(bodies[good].clone())
        );
        assert_eq!(
            f.reader
                .file_bytes_bounded(ids[bad].clone(), Some(1), 1024)
                .unwrap(),
            None
        );
        assert!(f
            .reader
            .lock()
            .net
            .file_demands
            .active()
            .unwrap()
            .is_empty());
        let requested = server.requests.lock().unwrap().clone();
        assert!(requested.contains(&f.old_blob), "old blob not requested; corrupt_first={corrupt_first}; admitted={admitted_blobs:?}; requests={requested:?}; served={:?}", server.served.lock().unwrap());
        assert!(requested.contains(&f.unrelated_old_blob), "neighbor blob not requested; corrupt_first={corrupt_first}; admitted={admitted_blobs:?}; requests={requested:?}; served={:?}", server.served.lock().unwrap());
        task.abort();
        let _ = task.await;
        server.stop().expect("corrupt HTTP worker finishes");

        // Restore only the transport bytes, retaining the same account, signed
        // history, Item IDs and successful cache. A failure must stay retryable.
        let mut healthy = BlobServer::start(f.uploads(), String::new(), false);
        let retry = ensure(&f.reader, &ids[bad], HTTP_JOURNEY_TIMEOUT_MS);
        until(|| {
            !f.reader
                .lock()
                .net
                .file_demands
                .active()
                .unwrap()
                .is_empty()
        })
        .await;
        let task = worker(&f.reader, &healthy, Arc::new(tokio::sync::Notify::new()));
        assert!(retry.await.unwrap().unwrap().ready);
        assert_eq!(healthy.requests.lock().unwrap().as_slice(), [bad_blob]);
        for (id, body) in ids.iter().zip(&bodies) {
            assert_eq!(
                f.reader
                    .file_bytes_bounded(id.clone(), Some(1), 1024)
                    .unwrap(),
                Some(body.clone())
            );
        }
        assert_eq!(
            (
                ids.iter()
                    .map(|id| f.reader.item(id.clone()).unwrap())
                    .collect::<Vec<_>>(),
                f.reader.timeline(f.space.clone()).unwrap(),
                outbox(&f.reader),
            ),
            before,
            "downloads cannot change canonical items, timeline or outgoing intent"
        );
        assert!(f.reader.verify_log(f.space).valid);
        assert!(f
            .reader
            .lock()
            .net
            .file_demands
            .active()
            .unwrap()
            .is_empty());
        task.abort();
        let _ = task.await;
        healthy.stop().expect("healthy HTTP worker finishes");
    }
}

#[tokio::test]
async fn new_foreground_demand_retries_same_worker_and_coalesces_before_backoff() {
    let f = Fixture::new();
    let mut corrupt = f.uploads();
    corrupt.get_mut(&f.old_blob).unwrap()[0] ^= 1;
    let mut server = BlobServer::start(corrupt, String::new(), false);
    let kick = Arc::new(tokio::sync::Notify::new());
    let task = worker(&f.reader, &server, kick.clone());
    until(|| {
        f.reader.items().iter().all(|item| {
            f.reader
                .file_info_at(item.id.clone(), Some(2))
                .unwrap()
                .ready
        })
    })
    .await;
    let before = (
        f.reader.item(f.item.clone()).unwrap(),
        f.reader.timeline(f.space.clone()).unwrap(),
        outbox(&f.reader),
    );
    server.requests.lock().unwrap().clear();
    server.served.lock().unwrap().clear();

    let first = ensure(&f.reader, &f.item, HTTP_JOURNEY_TIMEOUT_MS);
    until(|| f.reader.lock().net.file_demands.waiter_count() == 1).await;
    kick.notify_one();
    until(|| server.served.lock().unwrap().contains(&f.old_blob)).await;
    // End only the first foreground demand. The exact same HTTP server, worker,
    // account and signed version survive its real corrupt-ciphertext response.
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    assert!(f
        .reader
        .lock()
        .net
        .file_demands
        .active()
        .unwrap()
        .is_empty());
    assert!(
        !f.reader
            .file_info_at(f.item.clone(), Some(1))
            .unwrap()
            .ready
    );
    server
        .blobs
        .lock()
        .unwrap()
        .insert(f.old_blob.clone(), f.uploads()[&f.old_blob].clone());

    // This is a deadline/backoff contract, rather than a generic HTTP journey:
    // the new episode must make one fresh attempt before the old 2s backoff.
    let retry = ensure(&f.reader, &f.item, 1_000);
    let joined = ensure(&f.reader, &f.item, 1_000);
    until(|| f.reader.lock().net.file_demands.waiter_count() == 2).await;
    kick.notify_one();
    for result in [retry.await.unwrap(), joined.await.unwrap()] {
        assert!(
            result
                .expect("new foreground episode bypasses old backoff once")
                .ready
        );
    }
    assert_eq!(
        server.requests.lock().unwrap().as_slice(),
        [f.old_blob.clone(), f.old_blob.clone()]
    );
    assert_eq!(
        f.reader
            .file_bytes_bounded(f.item.clone(), Some(1), 1024)
            .unwrap(),
        Some(f.old.clone())
    );
    assert_eq!(
        (
            f.reader.item(f.item.clone()).unwrap(),
            f.reader.timeline(f.space.clone()).unwrap(),
            outbox(&f.reader),
        ),
        before,
        "foreground retry cannot change canonical content or outgoing intent"
    );
    assert!(f.reader.verify_log(f.space).valid);
    assert!(f
        .reader
        .lock()
        .net
        .file_demands
        .active()
        .unwrap()
        .is_empty());
    task.abort();
    let _ = task.await;
    server.stop().expect("same HTTP worker finishes");
}

#[test]
fn foreground_attempt_does_not_reset_background_deadline_or_failure_count() {
    let now = tokio::time::Instant::now();
    let mut retries = MediaDownloadRetries::default();
    retries.failed("selected", now);
    retries.failed("neighbor", now);
    let first_retry = now + Duration::from_secs(1);
    assert!(!retries.ready("selected", false, first_retry));
    assert!(retries.ready("selected", true, first_retry));
    assert!(!retries.ready("selected", false, first_retry));
    assert!(!retries.ready("neighbor", false, first_retry));
    retries.failed("selected", first_retry);
    assert!(!retries.ready("selected", false, first_retry + Duration::from_secs(3)));
    assert!(retries.ready("selected", false, first_retry + Duration::from_secs(4)));
    retries.completed("selected");
    assert!(retries.ready("selected", false, first_retry));
    assert!(!retries.ready("neighbor", false, first_retry));
}

#[test]
fn foreground_attempt_revalidates_scope_and_retains_unsynced_demand() {
    let mut f = Fixture::new();
    {
        let e = f.reader.lock();
        assert!(!e.wanted_media().is_empty());
        for _ in 0..3 {
            assert!(
                e.take_foreground_file_attempts().is_empty(),
                "background snapshots cannot create an attempt"
            );
        }
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let key = f.reader.lock().file_demand_key(&f.item, 1, 1024).unwrap();
    let lease = f
        .reader
        .lock()
        .net
        .file_demands
        .acquire(key.clone(), 1024, deadline)
        .unwrap()
        .unwrap();
    {
        let mut e = f.reader.lock();
        e.net.synced.remove(&f.space);
        assert!(e.take_foreground_file_attempts().is_empty());
        assert_eq!(
            e.net.file_demands.foreground_attempts().unwrap(),
            [(key.clone(), 1024)]
        );
        e.net.synced.insert(f.space.clone());
        let attempts = e.take_foreground_file_attempts();
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts.iter().next().unwrap().blob, f.old_blob);
        assert!(e.take_foreground_file_attempts().is_empty());
    }
    drop(lease);
    // These are stale local demand keys, not altered canonical events.
    for field in ["identity", "device", "version", "sha256"] {
        let mut stale = key.clone();
        match field {
            "identity" => stale.identity = "foreign".into(),
            "device" => stale.device = "foreign".into(),
            "version" => stale.version = 2,
            "sha256" => stale.sha256 = "00".repeat(32),
            _ => unreachable!(),
        }
        let e = f.reader.lock();
        let stale_lease = e
            .net
            .file_demands
            .acquire(stale, 1024, deadline)
            .unwrap()
            .unwrap();
        assert!(
            e.take_foreground_file_attempts().is_empty(),
            "stale {field}"
        );
        assert!(!stale_lease.active().unwrap());
    }
    let lease = f
        .reader
        .lock()
        .net
        .file_demands
        .acquire(key, 1024, deadline)
        .unwrap()
        .unwrap();
    f.sender
        .remove_member(f.space.clone(), f.reader.account().unwrap().identity_id)
        .unwrap();
    f.catch_up();
    assert!(f.reader.lock().take_foreground_file_attempts().is_empty());
    assert!(!lease.active().unwrap());
    assert!(f.reader.verify_log(f.space).valid);
}

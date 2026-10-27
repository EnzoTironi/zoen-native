//! Historical download lifecycle contracts using the shared real HTTP fixture.
use super::*;

#[tokio::test]
async fn historical_file_demand_timeout_cancel_and_invalid_requests_leave_no_demand(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = Fixture::new()?;
    assert!(f
        .reader
        .ensure_file_version(f.item.clone(), 1, 1, 100)
        .await
        .is_err());
    assert!(f
        .reader
        .ensure_file_version(f.item.clone(), 99, 1024, 100)
        .await
        .is_err());
    assert!(f
        .reader
        .ensure_file_version(f.item.clone(), 1, u64::MAX, 100)
        .await
        .is_err());
    assert!(f
        .reader
        .ensure_file_version(f.item.clone(), 1, 1024, 0)
        .await
        .is_err());
    assert!(f
        .reader
        .ensure_file_version(f.item.clone(), 1, 1024, 60_001)
        .await
        .is_err());
    assert!(f
        .reader
        .lock()
        .net
        .file_demands
        .active()
        .unwrap()
        .is_empty());
    assert!(f
        .reader
        .ensure_file_version(f.item.clone(), 1, 1024, 60)
        .await
        .is_err());
    assert!(
        f.reader
            .lock()
            .net
            .file_demands
            .active()
            .unwrap()
            .is_empty(),
        "expired demand removed"
    );
    let waiting = ensure(&f.reader, &f.item, 2000);
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
    waiting.abort();
    let _ = waiting.await;
    assert!(
        f.reader
            .lock()
            .net
            .file_demands
            .active()
            .unwrap()
            .is_empty(),
        "canceled future releases lease"
    );
    Ok(())
}

/// Original C17 adaptation: immutable item/version IDs replace Signal's positional
/// backfill pointers. A bad historical item must not poison its successful neighbor.
#[tokio::test]
async fn historical_file_demand_rechecks_membership_before_accepting_inflight_bytes(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let mut server = BlobServer::start(f.uploads(), f.old_blob.clone(), false);
    let waiting = ensure(&f.reader, &f.item, HTTP_JOURNEY_TIMEOUT_MS);
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
    let task = worker(&f.reader, &server, Arc::new(tokio::sync::Notify::new()));
    until(|| server.requests.lock().unwrap().contains(&f.old_blob)).await;
    f.sender
        .remove_member(f.space.clone(), f.reader.account().unwrap().identity_id)
        .unwrap();
    f.catch_up()?;
    server.allow_old.store(true, Ordering::SeqCst);
    assert!(matches!(
        waiting.await.unwrap(),
        Err(CoreError::Forbidden { .. })
    ));
    assert!(f
        .reader
        .lock()
        .net
        .file_demands
        .active()
        .unwrap()
        .is_empty());
    assert_eq!(
        f.reader
            .file_bytes_bounded(f.item.clone(), Some(1), 1024)
            .unwrap(),
        None
    );
    assert!(f
        .reader
        .ensure_file_version(f.item, 1, 1024, 100)
        .await
        .is_err());
    task.abort();
    let _ = task.await;
    server
        .stop()
        .expect("fixture HTTP worker must finish cleanly");
    Ok(())
}

#[tokio::test]
async fn historical_file_demand_rejects_corrupt_ciphertext_and_clears_on_deadline(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = Fixture::new()?;
    let mut server = BlobServer::start(f.uploads(), f.old_blob.clone(), true);
    server.allow_old.store(true, Ordering::SeqCst);
    let waiting = ensure(&f.reader, &f.item, HTTP_JOURNEY_TIMEOUT_MS);
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
    let task = worker(&f.reader, &server, Arc::new(tokio::sync::Notify::new()));
    until(|| server.requests.lock().unwrap().contains(&f.old_blob)).await;
    assert!(waiting.await.unwrap().is_err());
    assert!(server.requests.lock().unwrap().contains(&f.old_blob));
    assert_eq!(
        f.reader.file_bytes_bounded(f.item, Some(1), 1024).unwrap(),
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
    task.abort();
    let _ = task.await;
    server
        .stop()
        .expect("fixture HTTP worker must finish cleanly");
    Ok(())
}

#[tokio::test]
async fn historical_file_demand_sync_stop_cancels_waiters_and_account_change_is_rejected(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = Fixture::new()?;
    f.reader.start_sync(None).unwrap();
    let waiting = ensure(&f.reader, &f.item, 2000);
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
    f.reader.stop_sync();
    assert!(waiting.await.unwrap().is_err());
    assert!(f
        .reader
        .lock()
        .net
        .file_demands
        .active()
        .unwrap()
        .is_empty());
    let waiting = ensure(&f.reader, &f.item, 2000);
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
    f.reader.lock().net.author = None; // A real locked-account state, not download completion.
    assert!(matches!(
        waiting.await.unwrap(),
        Err(CoreError::Forbidden { .. })
    ));
    assert!(f
        .reader
        .lock()
        .net
        .file_demands
        .active()
        .unwrap()
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn historical_file_demand_stop_without_a_connection_cancels_pending_open(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = Fixture::new()?;
    assert!(f.reader.net.lock().unwrap().is_none());
    let waiting = ensure(&f.reader, &f.item, 2000);
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
    f.reader.stop_sync();
    assert!(tokio::time::timeout(Duration::from_millis(250), waiting)
        .await
        .expect("stop must cancel promptly, not merely reach the original two-second deadline")
        .unwrap()
        .is_err());
    assert!(f
        .reader
        .lock()
        .net
        .file_demands
        .active()
        .unwrap()
        .is_empty());
    assert_eq!(
        f.reader.file_bytes_bounded(f.item, Some(1), 1024).unwrap(),
        None
    );
    Ok(())
}

#[tokio::test]
async fn poisoned_demand_refuses_inflight_history_but_background_media_continues(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
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
    let waiting = ensure(&f.reader, &f.item, HTTP_JOURNEY_TIMEOUT_MS);
    until(|| f.reader.lock().net.file_demands.waiter_count() == 1).await;
    kick.notify_one();
    until(|| server.requests.lock().unwrap().contains(&f.old_blob)).await;
    let demands = f.reader.lock().net.file_demands.clone();
    demands.poison_for_test();
    assert!(
        matches!(waiting.await.unwrap(), Err(CoreError::Invalid { reason })
        if reason.contains("Downloads are unavailable"))
    );
    assert!(
        matches!(f.reader.ensure_file_version(f.item.clone(), 1, 1024, 100).await,
        Err(CoreError::Invalid { reason }) if reason.contains("Downloads are unavailable"))
    );

    // A subsequent canonical version supplies an observable next-pass barrier:
    // its HTTP request cannot start until the held history response was handled.
    let marker = b"%PDF-1.4 background after retired historical demand".to_vec();
    f.sender
        .file_new_version(
            f.item.clone(),
            marker.clone(),
            None,
            "Next canonical version".into(),
        )
        .unwrap();
    let marker_blob = {
        let engine = f.sender.lock();
        let types::ItemContent::File(doc) = &engine.state.items[&f.item].versions.current().content
        else {
            panic!("file");
        };
        doc.chunks[0].blob.clone()
    };
    f.catch_up()?;
    server.blobs.lock().unwrap().extend(f.uploads());
    let before = (
        f.reader.item(f.item.clone()).unwrap().version,
        f.reader.timeline(f.space.clone()).unwrap(),
        outbox(&f.reader),
    );
    server.allow_old.store(true, Ordering::SeqCst);
    kick.notify_one();
    until(|| server.requests.lock().unwrap().contains(&marker_blob)).await;
    until(|| {
        f.reader
            .file_info_at(f.item.clone(), Some(3))
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
    assert_eq!(
        f.reader
            .file_bytes_bounded(f.item.clone(), Some(3), 1024)
            .unwrap(),
        Some(marker)
    );
    assert_eq!(
        (
            f.reader.item(f.item.clone()).unwrap().version,
            f.reader.timeline(f.space.clone()).unwrap(),
            outbox(&f.reader)
        ),
        before
    );
    assert!(demands.active().is_err());
    assert!(demands.foreground_attempts().is_err());
    task.abort();
    let _ = task.await;
    server
        .stop()
        .expect("fixture HTTP worker must finish cleanly");
    Ok(())
}

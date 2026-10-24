//! Real Net retirement must fence an HTTP completion even beyond runtime cancellation.
use super::*;

fn current_chunk(f: &Fixture) -> types::ChunkRef {
    let engine = f.sender.lock();
    let types::ItemContent::File(file) = &engine.state.items[&f.item].current().content else {
        panic!("fixture canonical file");
    };
    file.chunks[0].clone()
}

fn session_lease(reader: &Client) -> EngineLease {
    reader.net.lock().unwrap().as_ref().unwrap().engine.clone()
}

fn independent_http_task(
    reader: &Client,
    lease: EngineLease,
    server: &BlobServer,
    kick: Arc<tokio::sync::Notify>,
) -> tokio::task::JoinHandle<()> {
    let (_, key, _, _, _, _) = reader.lock().net_credentials().unwrap();
    // Use the actual Net's lease on a separate test runtime so shutdown_background
    // alone cannot satisfy this test. No fake lease or account/device is minted.
    // This isolates lifecycle admission; authenticated WebSocket is not tested here.
    tokio::spawn(media_worker(
        lease,
        None,
        server.base.clone(),
        "retirement-fixture".into(),
        key,
        kick,
    ))
}

async fn retired_http_completion(replace_account: bool) {
    let f = Fixture::new();
    let chunk = current_chunk(&f);
    let mut server = BlobServer::start(f.uploads(), chunk.blob.clone(), false);
    f.reader.start_sync(None).unwrap();
    let old_lease = session_lease(&f.reader);
    let (reader, item) = (f.reader.clone(), f.item.clone());
    let waiting = tokio::spawn(async move {
        reader
            .ensure_file_version(item, 2, 1024, HTTP_JOURNEY_TIMEOUT_MS)
            .await
    });
    until(|| f.reader.lock().net.file_demands.waiter_count() == 1).await;
    let kick = Arc::new(tokio::sync::Notify::new());
    let old_http = independent_http_task(&f.reader, old_lease.clone(), &server, kick.clone());
    until(|| server.requests.lock().unwrap().contains(&chunk.blob)).await;

    let old_account = f.reader.account().unwrap();
    let old_keys = f.reader_vault.0.lock().unwrap().clone();
    if replace_account {
        f.reader.erase_device(f.reader_vault.clone()).unwrap();
        assert!(f.reader_vault.0.lock().unwrap().is_empty());
        let next = f
            .reader
            .create_account(
                "Next account".into(),
                "next_account".into(),
                "http://127.0.0.1:1".into(),
                f.reader_vault.clone(),
            )
            .unwrap();
        assert_ne!(old_account.identity_id, next.identity_id);
        assert_ne!(old_keys, *f.reader_vault.0.lock().unwrap());
        assert!(f.reader.items().is_empty());
    } else {
        f.reader.stop_sync();
        assert_eq!(
            f.reader.account().unwrap().identity_id,
            old_account.identity_id
        );
        assert_eq!(old_keys, *f.reader_vault.0.lock().unwrap());
    }
    assert!(waiting.await.unwrap().is_err());
    assert!(old_lease.with(|_| ()).is_none());
    // The same Engine is now owned by a genuine new Net, not by the old HTTP.
    f.reader.start_sync(None).unwrap();
    let fresh_lease = session_lease(&f.reader);
    assert!(fresh_lease.with(|_| ()).is_some());
    let before = (
        f.reader.account().unwrap(),
        f.reader.items(),
        outbox(&f.reader),
    );
    if !replace_account {
        assert!(
            f.reader
                .lock()
                .wanted_media()
                .iter()
                .any(|w| w.blob == chunk.blob),
            "the new session still legitimately wants exactly the same current bytes"
        );
    }
    server.allow_old.store(true, Ordering::SeqCst);
    kick.notify_one();
    tokio::time::timeout(Duration::from_secs(5), old_http)
        .await
        .expect("retired HTTP task must terminate without abort")
        .unwrap();
    assert!(server.served.lock().unwrap().contains(&chunk.blob));
    assert!(
        !f.reader.lock().store.has_media(&chunk.sha256).unwrap(),
        "old session cannot write into the restarted/replacement account cache"
    );
    assert_eq!(
        (
            f.reader.account().unwrap(),
            f.reader.items(),
            outbox(&f.reader)
        ),
        before
    );

    if !replace_account {
        let fresh = independent_http_task(
            &f.reader,
            fresh_lease,
            &server,
            Arc::new(tokio::sync::Notify::new()),
        );
        until(|| {
            f.reader
                .file_info_at(f.item.clone(), Some(2))
                .unwrap()
                .ready
        })
        .await;
        assert_eq!(
            f.reader
                .file_bytes_bounded(f.item.clone(), Some(2), 1024)
                .unwrap(),
            Some(b"%PDF-1.4 current bytes".to_vec())
        );
        assert_eq!(
            server
                .requests
                .lock()
                .unwrap()
                .iter()
                .filter(|b| *b == &chunk.blob)
                .count(),
            2,
            "only the fresh session may fetch and accept its own exact copy"
        );
        fresh.abort();
        let _ = fresh.await;
    }
    f.reader.stop_sync();
    server.stop().expect("owned HTTP fixture must terminate");
}

#[tokio::test]
async fn stop_restart_retires_inflight_http_even_when_new_session_wants_same_file() {
    retired_http_completion(false).await;
}

#[tokio::test]
async fn erase_recreate_account_retires_inflight_http_before_new_identity_owns_engine() {
    retired_http_completion(true).await;
}

#[tokio::test]
async fn canceling_only_foreground_waiter_preserves_authorized_current_background_download() {
    let f = Fixture::new();
    let chunk = current_chunk(&f);
    let mut server = BlobServer::start(f.uploads(), chunk.blob.clone(), false);
    let (reader, item) = (f.reader.clone(), f.item.clone());
    let waiting = tokio::spawn(async move {
        reader
            .ensure_file_version(item, 2, 1024, HTTP_JOURNEY_TIMEOUT_MS)
            .await
    });
    until(|| f.reader.lock().net.file_demands.waiter_count() == 1).await;
    let task = worker(&f.reader, &server, Arc::new(tokio::sync::Notify::new()));
    until(|| server.requests.lock().unwrap().contains(&chunk.blob)).await;
    waiting.abort();
    assert!(waiting.await.unwrap_err().is_cancelled());
    assert_eq!(f.reader.lock().net.file_demands.waiter_count(), 0);
    server.allow_old.store(true, Ordering::SeqCst);
    until(|| {
        f.reader
            .file_info_at(f.item.clone(), Some(2))
            .unwrap()
            .ready
    })
    .await;
    assert_eq!(
        f.reader.file_bytes_bounded(f.item, Some(2), 1024).unwrap(),
        Some(b"%PDF-1.4 current bytes".to_vec())
    );
    assert_eq!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|b| *b == &chunk.blob)
            .count(),
        1
    );
    task.abort();
    let _ = task.await;
    server.stop().expect("owned HTTP fixture must terminate");
}

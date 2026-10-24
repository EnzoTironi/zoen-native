//! Late HTTP completion cannot borrow authority from the current-version background path.
use super::*;

fn current_chunk(f: &Fixture) -> types::ChunkRef {
    let engine = f.sender.lock();
    let types::ItemContent::File(file) = &engine.state.items[&f.item].current().content else {
        panic!("fixture must contain a canonical file");
    };
    file.chunks[0].clone()
}

fn remove_reader(f: &mut Fixture) {
    let identity = f.reader.account().unwrap().identity_id;
    f.sender
        .remove_member(f.space.clone(), identity.clone())
        .unwrap();
    f.catch_up();
    assert!(!f.reader.lock().is_member(&f.space, &identity));
}

#[test]
fn revoked_current_file_is_not_background_download_authority() {
    let mut f = Fixture::new();
    let chunk = current_chunk(&f);
    assert!(f
        .reader
        .lock()
        .wanted_media()
        .iter()
        .any(|w| w.blob == chunk.blob));
    remove_reader(&mut f);
    assert!(
        !f.reader
            .lock()
            .wanted_media()
            .iter()
            .any(|w| w.space == f.space),
        "a synced but revoked chat must not retain background download authority"
    );
}

/// S06-inspired own-protocol contract: revocation while a real fetch is pending
/// must prevent late completion from creating a cached plaintext resource.
#[tokio::test]
async fn revoked_current_file_response_does_not_cache_or_block_an_authorized_chat() {
    let mut f = Fixture::new();
    let chunk = current_chunk(&f);
    let mut server = BlobServer::start(f.uploads(), chunk.blob.clone(), false);
    let (reader, item) = (f.reader.clone(), f.item.clone());
    let waiting = tokio::spawn(async move {
        reader
            .ensure_file_version(item, 2, 1024, HTTP_JOURNEY_TIMEOUT_MS)
            .await
    });
    until(|| f.reader.lock().net.file_demands.waiter_count() == 1).await;
    let kick = Arc::new(tokio::sync::Notify::new());
    let task = worker(&f.reader, &server, kick.clone());
    until(|| server.requests.lock().unwrap().contains(&chunk.blob)).await;
    remove_reader(&mut f);
    assert!(matches!(
        waiting.await.unwrap(),
        Err(CoreError::Forbidden { .. })
    ));
    assert_eq!(f.reader.lock().net.file_demands.waiter_count(), 0);

    // A distinct, still-authorized signed chat supplies a real next-pass barrier.
    // Its request cannot start before this worker handles the held HTTP result.
    let marker_space = f
        .sender
        .create_group_with(
            "Still authorized".into(),
            vec![f.reader.account().unwrap().identity_id],
            PrivacyDto::Closed,
        )
        .unwrap();
    let marker_bytes = b"authorized next-pass media".to_vec();
    let marker = f
        .sender
        .file_add(
            marker_space.clone(),
            "marker.txt".into(),
            "marker.txt".into(),
            "text/plain".into(),
            marker_bytes.clone(),
            None,
        )
        .unwrap();
    let mut relay = event_log::SpaceLog::new(marker_space.clone());
    for pending in f
        .sender
        .lock()
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .filter(|p| p.event.space == marker_space)
    {
        let event = relay.sequence(pending.event);
        assert!(matches!(
            f.reader.lock().ingest(Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: Envelope::plain(event),
            }),
            Ingest::Applied
        ));
    }
    server.blobs.lock().unwrap().extend(f.uploads());
    let before = (
        f.reader.timeline(f.space.clone()).unwrap(),
        outbox(&f.reader),
    );
    server.allow_old.store(true, Ordering::SeqCst);
    kick.notify_one();
    until(|| {
        f.reader
            .file_info_at(marker.id.clone(), Some(1))
            .unwrap()
            .ready
    })
    .await;
    assert!(server.served.lock().unwrap().contains(&chunk.blob));
    assert!(
        !f.reader.lock().store.has_media(&chunk.sha256).unwrap(),
        "late ciphertext must not enter plaintext cache after current membership is revoked"
    );
    assert_eq!(
        f.reader
            .file_bytes_bounded(marker.id, Some(1), 1024)
            .unwrap(),
        Some(marker_bytes)
    );
    assert_eq!(
        (
            f.reader.timeline(f.space.clone()).unwrap(),
            outbox(&f.reader)
        ),
        before
    );
    task.abort();
    let _ = task.await;
    server.stop().expect("owned HTTP fixture must terminate");
}

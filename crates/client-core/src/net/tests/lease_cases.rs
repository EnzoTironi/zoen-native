use super::*;

pub(super) fn engine_lease_revocation_waits_for_current_work_and_finishes_storage_release(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(types::new_id("lease-revoke")?);
    let path = path.to_str().unwrap();
    let lease = EngineLease::new(Arc::new(Mutex::new(Engine::open(path).unwrap())));
    let (entered, active) = channel::channel();
    let (release, released) = channel::channel();
    let worker_lease = lease.clone();
    let worker = std::thread::spawn(move || {
        worker_lease
            .with(|engine| {
                engine.store.set_meta("lease-proof", "committed").unwrap();
                entered.send(()).unwrap();
                released.recv_timeout(Duration::from_secs(5)).unwrap();
            })
            .unwrap();
    });
    active.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        lease.0.try_lock(),
        Err(std::sync::TryLockError::WouldBlock)
    ));
    let (revoking, began) = channel::channel();
    let (completed, done) = channel::channel();
    let revoke_lease = lease.clone();
    let revoker = std::thread::spawn(move || {
        revoking.send(()).unwrap();
        revoke_lease.revoke();
        completed.send(()).unwrap();
    });
    began.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(done.try_recv(), Err(channel::TryRecvError::Empty)));
    release.send(()).unwrap();
    done.recv_timeout(Duration::from_secs(5)).unwrap();
    let reopened = Engine::open(path).unwrap();
    assert_eq!(
        reopened.store.meta("lease-proof").unwrap().as_deref(),
        Some("committed")
    );
    assert!(lease.with(|_| ()).is_none());
    worker.join().unwrap();
    revoker.join().unwrap();
    drop(reopened);
    std::fs::remove_file(path).unwrap();

    Ok(())
}

pub(super) fn poisoned_engine_lease_still_revokes_and_releases_storage(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(types::new_id("lease-poison")?);
    let path = path.to_str().unwrap();
    let lease = EngineLease::new(Arc::new(Mutex::new(Engine::open(path).unwrap())));
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        lease.with(|engine| {
            engine.store.set_meta("poison-proof", "committed").unwrap();
            panic!("controlled Engine closure panic");
        });
    }));
    assert!(panic.is_err());
    assert_eq!(
        lease
            .with(|engine| engine.store.meta("poison-proof").unwrap())
            .flatten()
            .as_deref(),
        Some("committed")
    );
    lease.revoke();
    let reopened = Engine::open(path).unwrap();
    assert_eq!(
        reopened.store.meta("poison-proof").unwrap().as_deref(),
        Some("committed")
    );
    drop(reopened);
    std::fs::remove_file(path).unwrap();

    Ok(())
}

pub(super) fn blocked_media_upload_releases_storage_and_preserves_ciphertext(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let path = std::env::temp_dir().join(types::new_id("media-stop")?);
    let path = path.to_str().unwrap();
    let mut engine = Engine::open(path).unwrap();
    engine
        .create_account("Media stop", "media_stop", &url)
        .unwrap();
    engine.set_registered(true).unwrap();
    let media = engine
        .put_media(b"actual attachment", "image/png", 1, 1)
        .unwrap();
    let mut media = types::MediaRef {
        sha256: media.sha256,
        mime: media.mime,
        width: media.width,
        height: media.height,
        bytes: media.bytes,
        key: None,
        blob: None,
    };
    engine.seal_for_relay(&mut media).unwrap();
    let queued = engine.pending_uploads(8);
    assert_eq!(queued.len(), 1);
    let (started, uploading) = channel::channel();
    let (release, released) = channel::channel();
    let relay = std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut socket = tokio_tungstenite::tungstenite::accept(socket).unwrap();
        let Message::Binary(hello) = socket.read().unwrap() else {
            panic!("hello")
        };
        let ClientFrame::Hello {
            identity, device, ..
        } = ClientFrame::decode(&hello).unwrap()
        else {
            panic!("hello")
        };
        let nonce = "lease-media-nonce";
        let relay_name = "lease-media-local-relay";
        socket
            .send(Message::Binary(
                ServerFrame::Challenge {
                    nonce: nonce.into(),
                    relay: relay_name.into(),
                    protocol: PROTOCOL_VERSION,
                    capabilities: Vec::new(),
                    server_time_ms: None,
                }
                .encode()
                .into(),
            ))
            .unwrap();
        let Message::Binary(auth) = socket.read().unwrap() else {
            panic!("auth")
        };
        let ClientFrame::Auth { sig } = ClientFrame::decode(&auth).unwrap() else {
            panic!("auth")
        };
        assert!(event_log::verify_sig(
            &device,
            &auth_message(nonce, relay_name),
            &sig
        ));
        socket
            .send(Message::Binary(
                ServerFrame::Ready {
                    identity,
                    registered: true,
                }
                .encode()
                .into(),
            ))
            .unwrap();
        let (mut http, _) = listener.accept().unwrap();
        http.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut headers = Vec::new();
        let mut byte = [0u8; 1];
        while !headers.ends_with(b"\r\n\r\n") {
            http.read_exact(&mut byte).unwrap();
            headers.push(byte[0]);
            assert!(headers.len() < 16 * 1024);
        }
        let headers = String::from_utf8(headers).unwrap();
        assert!(headers.starts_with("PUT /v1/blobs/"));
        assert!(headers.contains(&format!("x-zoen-device: {device}")));
        started.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let net = Net::start(Arc::new(Mutex::new(engine)), None, crate::i18n::Lang::En).unwrap();
    uploading.recv_timeout(Duration::from_secs(5)).unwrap();
    let began = std::time::Instant::now();
    drop(net);
    let reopened = Engine::open(path).unwrap();
    eprintln!(
        "blocked_media stop_reopen_us={}",
        began.elapsed().as_micros()
    );
    assert_eq!(reopened.pending_uploads(8), queued);
    assert!(reopened.account().is_some());
    release.send(()).unwrap();
    relay.join().unwrap();
    drop(reopened);
    std::fs::remove_file(path).unwrap();

    Ok(())
}

//! HTTP upload, interrupted retry and cold persistence scenario.
use super::*;

pub(super) async fn terminal_upload(
    status: u16,
    file_presentation: bool,
    voice: bool,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let retrying = status == 503;
    let grouped = status == 413;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let directory = Directory(std::env::temp_dir().join(types::new_id("upload-refusal")?));
    std::fs::create_dir(&directory.0).unwrap();
    let receiver_path = directory.0.join("receiver.sqlite");
    let mut receiver = Engine::open(receiver_path.to_str().unwrap()).unwrap();
    let (_, receiver_secret, _) = receiver.create_account("Reader", "reader", &url).unwrap();
    let sender_path = directory.0.join("sender.sqlite");
    let mut sender = Client::open(sender_path.to_str().unwrap().into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    sender
        .create_account("Sender".into(), "sender".into(), url.clone(), vault.clone())
        .unwrap();
    let space = sender
        .create_group_with(
            "Upload refusal".into(),
            vec![receiver.me.clone().unwrap()],
            PrivacyDto::Closed,
        )
        .unwrap();
    let mut head = None;
    let setup = sender.lock().store.outbox().unwrap();
    for row in setup {
        let frame = sequence(Envelope::plain(&row.event), &mut head);
        assert_eq!(sender.lock().ingest(frame.clone()), Ingest::Confirmed);
        assert_eq!(receiver.ingest(frame), Ingest::Applied);
    }
    let account = sender.account().unwrap();
    let scope = MediaDraftScope {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: space.clone(),
        thread_root_id: None,
    };
    let plain = if voice {
        pcm_wave()
    } else {
        b"%PDF-1.4 private upload refusal fixture".to_vec()
    };
    let ids = if voice {
        vec!["original", "edited"]
    } else if grouped {
        vec!["a", "b"]
    } else {
        vec!["a"]
    };
    let mut content = serde_json::json!({"kind":"attachments", "options":{"presentation":"file", "grouping":if grouped {"grouped"} else {"separate"}},
        "items":ids.iter().map(|id| serde_json::json!({"asset_id":id, "caption":format!("Keep caption {id}"), "mutation_id":format!("upload-{id}")})).collect::<Vec<_>>()});
    if !file_presentation {
        content.as_object_mut().unwrap().remove("options");
    }
    if voice {
        content = serde_json::json!({"kind":"voice", "source_asset_id":"original", "editor":{"trim":[0,250]},
            "render":{"asset_id":"edited","mime":"audio/wav","duration_ms":250.0,"levels":[0.0,0.0],"transcript":"Keep voice transcript"}});
    }
    let content = content.to_string();
    let saved = sender
        .save_media_draft(
            scope.clone(),
            "0".into(),
            content,
            ids.iter()
                .map(|id| MediaDraftAssetInput {
                    id: (*id).into(),
                    name: format!("{id}.{}", if voice { "wav" } else { "pdf" }),
                    mime: if voice {
                        "audio/wav".into()
                    } else {
                        "application/pdf".into()
                    },
                    bytes: if voice || *id == "a" {
                        plain.clone()
                    } else {
                        b"%PDF-1.4 group second file".to_vec()
                    },
                })
                .collect(),
        )
        .unwrap();
    let sent = sender
        .send_media_draft(scope.clone(), saved.draft.revision.clone())
        .unwrap();
    let later = (!retrying).then(|| {
        sender
            .send_message(space.clone(), "Later ordinary message".into())
            .unwrap()
            .id
    });
    let message = sent.entries[0].id.clone();
    let queued = sender.lock().pending_uploads(8);
    assert_eq!(queued.len(), if voice { 1 } else { ids.len() });
    let (blob, ciphertext) = queued[0].clone();
    assert_eq!(hex::encode(Sha256::digest(&ciphertext)), blob);
    let http = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", http.local_addr().unwrap());
    let expected_blob = blob.clone();
    let expected_ciphertext = ciphertext.clone();
    let expected_uploads = queued.clone();
    let (observed_tx, mut observed) = mpsc::unbounded_channel();
    let (release_http, released_http) = std::sync::mpsc::channel();
    let (release_interrupted, released_interrupted) = std::sync::mpsc::channel();
    let http_server = std::thread::spawn(move || {
        for attempt in 0..if retrying { 3 } else { expected_uploads.len() } {
            let (mut socket, _) = http.accept().unwrap();
            let (headers, body) = http_request(&mut socket);
            let expected = &expected_uploads[if retrying { 0 } else { attempt }];
            assert!(headers.starts_with(&format!("PUT /v1/blobs/{} ", expected.0)));
            assert_eq!(body, expected.1, "retry keeps exact ciphertext");
            observed_tx.send(attempt).unwrap();
            if retrying && attempt == 1 {
                // This second PUT is proof that the SAME worker consumed the
                // prior 503 and took its retry branch. Interrupt it in flight.
                released_interrupted
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap();
                continue;
            }
            let code = if attempt != 0 {
                if retrying {
                    released_http.recv_timeout(Duration::from_secs(10)).unwrap();
                }
                200
            } else {
                status
            };
            write!(
                socket,
                "HTTP/1.1 {code} Fixture\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        }
        if retrying {
            let (mut socket, _) = http.accept().unwrap();
            let (headers, _) = http_request(&mut socket);
            assert!(headers.starts_with(&format!("GET /v1/blobs/{expected_blob} ")));
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                expected_ciphertext.len()
            )
            .unwrap();
            socket.write_all(&expected_ciphertext).unwrap();
        }
    });
    let mut lease = EngineLease::new(sender.inner.clone());
    let (_, key, _, _, _, _) = sender.lock().net_credentials().unwrap();
    let retry_kick = Arc::new(tokio::sync::Notify::new());
    let mut worker = tokio::spawn(media_worker(
        lease.clone(),
        None,
        base.clone(),
        "upload-fixture".into(),
        key.clone(),
        retry_kick.clone(),
    ));
    if retrying {
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), observed.recv())
                .await
                .unwrap(),
            Some(0)
        );
        // Notify retains its permit until the production loop reaches the idle
        // wait; no elapsed-time guess establishes that 503 was processed.
        retry_kick.notify_one();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), observed.recv())
                .await
                .unwrap(),
            Some(1)
        );
        worker.abort();
        let _ = worker.await;
        lease.revoke();
        release_interrupted.send(()).unwrap();
        assert_eq!(sender.lock().pending_uploads(8), queued);
        drop(sender);
        sender = Client::open(sender_path.to_str().unwrap().into(), "en".into()).unwrap();
        assert!(sender.unlock(vault.clone()).unwrap());
        assert_eq!(
            sender.lock().pending_uploads(8),
            queued,
            "transient ciphertext survives cold restart"
        );
        let replay = sender
            .send_media_draft(scope.clone(), saved.draft.revision.clone())
            .unwrap();
        assert_eq!(
            replay
                .entries
                .iter()
                .map(|entry| &entry.id)
                .collect::<Vec<_>>(),
            sent.entries
                .iter()
                .map(|entry| &entry.id)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            sender.lock().store.outbox().unwrap().len(),
            2,
            "receipt replay does not duplicate intents"
        );
        lease = EngineLease::new(sender.inner.clone());
        worker = tokio::spawn(media_worker(
            lease.clone(),
            None,
            base.clone(),
            "upload-fixture".into(),
            key,
            Arc::new(tokio::sync::Notify::new()),
        ));
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), observed.recv())
                .await
                .unwrap(),
            Some(2)
        );
    }
    if !retrying {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !sender.lock().pending_uploads(8).is_empty() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("upload response was not processed");
    }

    // Drive actual session/Accepted/Event handling to quiescence. The ordered
    // request is a wire barrier; a sleep is not evidence of absent publication.
    let (commands_tx, mut commands_rx) = mpsc::unbounded_channel();
    let wake = commands_tx.clone();
    sender.lock().net.poke = Some(Box::new(move || {
        let _ = wake.send(Cmd::Flush);
    }));
    let (status_tx, _) = watch::channel(NetStatus {
        state: ConnState::Connecting,
        synced: false,
        error: None,
        registered: true,
        authenticated_relay: None,
    });
    sender.lock().set_registered(true).unwrap();
    let ctx = Ctx {
        engine: EngineLease::new(sender.inner.clone()),
        listener: None,
        status: status_tx,
        lang: crate::i18n::Lang::En,
        maintenance_work: None,
    };
    let (finished, close_socket) = oneshot::channel();
    let client = async {
        let mut backoff = Duration::from_millis(500);
        let exit = session(&ctx, &mut commands_rx, &mut backoff).await;
        finished.send(()).unwrap();
        exit
    };
    let relay = session::drive(session::Relay {
        listener: &listener,
        retrying,
        commands_tx: &commands_tx,
        sender: &sender,
        queued: queued.clone(),
        release_http,
        head,
        receiver: &mut receiver,
        space: space.clone(),
        close_socket,
    });
    let (exit, ()) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(client, relay)
    })
    .await
    .expect("upload refusal did not reach durable quiescence");
    assert!(matches!(exit, Exit::Stop));
    worker.abort();
    let _ = worker.await;
    lease.revoke();
    ctx.engine.revoke();
    drop(ctx);
    drop(sender);
    drop(receiver);
    let mut receiver = Engine::open(receiver_path.to_str().unwrap()).unwrap();
    assert!(receiver.unlock(Some(receiver_secret.to_vec())).unwrap());
    let sender = Client::open(sender_path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(sender.unlock(vault).unwrap());
    let local = sender
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == message)
        .unwrap();
    let remote = receiver.timeline(&space).unwrap();
    eprintln!(
        "HTTP {status}: cold_sender={:?} cold_receiver_messages={} retained_uploads={}",
        local.delivery,
        remote.iter().filter(|entry| entry.id == message).count(),
        sender.lock().uploads_pending()
    );
    if retrying {
        assert_eq!(local.delivery, crate::Delivery::Sent);
        assert_eq!(remote.iter().filter(|entry| entry.id == message).count(), 1);
        let wanted = receiver
            .wanted_media()
            .into_iter()
            .find(|wanted| wanted.blob == blob)
            .unwrap();
        let downloaded = crate::https::client()
            .build()
            .unwrap()
            .get(format!("{base}/v1/blobs/{blob}"))
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert_eq!(downloaded.as_ref(), ciphertext);
        assert!(receiver.media_arrived(&wanted, &downloaded));
        let item = receiver.state.items.values().find(|item| matches!(&item.current().content, types::ItemContent::File(file) if file.sha256 == hex::encode(Sha256::digest(&plain)))).unwrap().id.clone();
        assert_eq!(receiver.file_bytes(&item, None).unwrap(), Some(plain));
        http_server.join().unwrap();
        return Ok(());
    }
    http_server.join().unwrap();
    assert!(
        remote
            .iter()
            .all(|entry| sent.entries.iter().all(|original| entry.id != original.id)),
        "refused bytes must not acquire confirmed remote message delivery"
    );
    assert_eq!(
        remote
            .iter()
            .filter(|entry| Some(&entry.id) == later.as_ref())
            .count(),
        1,
        "terminal group must not park later ordinary messages"
    );
    let timeline = sender.timeline(space.clone()).unwrap();
    for original in &sent.entries {
        let retained = timeline
            .iter()
            .find(|entry| entry.id == original.id)
            .unwrap();
        assert_eq!(retained.delivery, crate::Delivery::Failed);
        assert_eq!(retained.kind, original.kind);
        assert_eq!(retained.media, original.media);
    }
    let receipt = sender
        .send_media_draft(scope.clone(), saved.draft.revision)
        .unwrap();
    assert_eq!(
        receipt
            .entries
            .iter()
            .map(|entry| &entry.id)
            .collect::<Vec<_>>(),
        sent.entries
            .iter()
            .map(|entry| &entry.id)
            .collect::<Vec<_>>(),
        "terminal receipt replay cannot produce new successful sends"
    );
    assert_eq!(
        local.delivery,
        crate::Delivery::Failed,
        "terminal upload must retain a readable durable failure"
    );
    assert_eq!(local.kind, sent.entries[0].kind);
    assert_eq!(local.media, sent.entries[0].media);
    assert_eq!(local.voice, sent.entries[0].voice);
    if voice {
        let voice = local
            .voice
            .as_ref()
            .expect("failed voice must keep its playback metadata");
        assert_eq!(voice.ms, 250);
        assert_eq!(voice.transcript, "Keep voice transcript");
        assert_eq!(
            sender.file_bytes(voice.item_id.clone(), None).unwrap(),
            Some(plain.clone())
        );
    }
    if !file_presentation && !voice {
        let crate::EntryKind::Message {
            card: Some(card), ..
        } = &local.kind
        else {
            panic!("default attachment lost its recovery card")
        };
        let mut engine = sender.lock();
        assert!(engine.state.is_recovery_item(&card.item_id));
        let before = engine.store.conn().total_changes();
        assert!(engine
            .post_as(
                &space,
                &scope.expected_identity_id,
                "new claim",
                Some(card.item_id.clone())
            )
            .is_err());
        assert!(engine
            .file_new_version_typed(
                &card.item_id,
                b"must not write",
                None,
                "new version",
                "new.pdf",
                "application/pdf"
            )
            .is_err());
        assert!(engine
            .restore_version(&card.item_id, 1, "new revert")
            .is_err());
        assert_eq!(
            engine.store.conn().total_changes(),
            before,
            "display-only default recovery cannot authorize writes"
        );
    }
    let engine = sender.lock();
    let retained: Vec<u8> = engine
        .store
        .conn()
        .query_row(
            "SELECT bytes FROM blob_uploads WHERE blob_sha = ?1 AND failed = 1",
            [&blob],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        retained, ciphertext,
        "terminal refusal retains original encrypted intent"
    );
    assert!(
        engine.pending_uploads(8).is_empty(),
        "terminal refusal does not retry forever"
    );
    drop(engine);
    assert_eq!(
        sender
            .lock()
            .media(&hex::encode(Sha256::digest(&plain)))
            .unwrap(),
        Some(plain.clone())
    );
    if voice {
        return Ok(());
    }
    let fresh = sender.save_media_draft(scope.clone(), receipt.draft.revision, serde_json::json!({"kind":"attachments", "items":[{"asset_id":"fresh", "caption":"Explicit fresh send", "mutation_id":"fresh-send"}]}).to_string(), vec![MediaDraftAssetInput { id:"fresh".into(), name:"fresh.pdf".into(), mime:"application/pdf".into(), bytes:plain }]).unwrap();
    let fresh = sender
        .send_media_draft(scope, fresh.draft.revision)
        .unwrap();
    assert!(sent.entries.iter().all(|old| old.id != fresh.entries[0].id));
    let uploads = sender.lock().pending_uploads(8);
    assert_eq!(uploads.len(), 1);
    assert_ne!(
        uploads[0].0, blob,
        "explicit new send uses a fresh File/key rather than reviving a refused signed intent"
    );

    Ok(())
}

mod session;

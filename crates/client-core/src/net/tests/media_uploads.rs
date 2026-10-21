//! Public draft sends through the real HTTP upload worker and WebSocket session.
//! Only the relay is a fixture; both clients persist their canonical state normally.
use super::*;
use crate::{Client, MediaDraftAssetInput, MediaDraftScope, PrivacyDto, SecretVault};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}
struct Directory(std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn public_media_upload_400_never_confirms_unrecoverable_attachment() {
    terminal_upload(400).await;
}

#[tokio::test(flavor = "current_thread")]
async fn public_media_upload_413_never_confirms_unrecoverable_attachment() {
    terminal_upload(413).await;
}

#[tokio::test(flavor = "current_thread")]
async fn public_media_upload_transient_cold_retry_keeps_exact_intent_and_remote_bytes() {
    terminal_upload(503).await;
}

async fn terminal_upload(status: u16) {
    let retrying = status == 503;
    let grouped = status == 413;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let directory = Directory(std::env::temp_dir().join(types::new_id("upload-refusal")));
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
    let plain = b"%PDF-1.4 private upload refusal fixture".to_vec();
    let ids = if grouped { vec!["a", "b"] } else { vec!["a"] };
    let content = serde_json::json!({"kind":"attachments", "options":{"presentation":"file", "grouping":if grouped {"grouped"} else {"separate"}},
        "items":ids.iter().map(|id| serde_json::json!({"asset_id":id, "caption":format!("Keep caption {id}"), "mutation_id":format!("upload-{id}")})).collect::<Vec<_>>()}).to_string();
    let saved = sender
        .save_media_draft(
            scope.clone(),
            "0".into(),
            content,
            ids.iter()
                .map(|id| MediaDraftAssetInput {
                    id: (*id).into(),
                    name: format!("{id}.pdf"),
                    mime: "application/pdf".into(),
                    bytes: if *id == "a" {
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
    assert_eq!(queued.len(), ids.len());
    let (blob, ciphertext) = queued[0].clone();
    assert_eq!(hex::encode(Sha256::digest(&ciphertext)), blob);
    let http = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", http.local_addr().unwrap());
    let expected_blob = blob.clone();
    let expected_ciphertext = ciphertext.clone();
    let expected_uploads = queued.clone();
    let (observed_tx, mut observed) = mpsc::unbounded_channel();
    let (release_http, released_http) = std::sync::mpsc::channel();
    let http_server = std::thread::spawn(move || {
        for attempt in 0..if retrying { 2 } else { expected_uploads.len() } {
            let (mut socket, _) = http.accept().unwrap();
            let (headers, body) = http_request(&mut socket);
            let expected = &expected_uploads[if retrying { 0 } else { attempt }];
            assert!(headers.starts_with(&format!("PUT /v1/blobs/{} ", expected.0)));
            assert_eq!(body, expected.1, "retry keeps exact ciphertext");
            observed_tx.send(attempt).unwrap();
            let code = if attempt == 1 {
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
    let mut worker = tokio::spawn(media_worker(
        lease.clone(),
        None,
        base.clone(),
        "upload-fixture".into(),
        key.clone(),
        Arc::new(tokio::sync::Notify::new()),
    ));
    if retrying {
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), observed.recv())
                .await
                .unwrap(),
            Some(0)
        );
        worker.abort();
        let _ = worker.await;
        lease.revoke();
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
            Some(1)
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
    let relay = async {
        let mut socket = catch_up_socket(&listener).await;
        reply(&mut socket, ServerFrame::SyncDone).await;
        if retrying {
            let (answer, answered) = oneshot::channel();
            commands_tx.send(Cmd::Flush).unwrap();
            commands_tx
                .send(Cmd::Req {
                    op: Op::FetchLink {
                        id: "upload-not-confirmed".into(),
                    },
                    reply: answer,
                })
                .unwrap();
            loop {
                match request(&mut socket).await {
                    ClientFrame::Req {
                        id,
                        op: Op::FetchLink { id: marker },
                    } if marker == "upload-not-confirmed" => {
                        reply(
                            &mut socket,
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::Link(None)),
                            },
                        )
                        .await;
                        break;
                    }
                    ClientFrame::Req { id, .. } => {
                        reply(
                            &mut socket,
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::Done),
                            },
                        )
                        .await
                    }
                    ClientFrame::Ping => {
                        reply(
                            &mut socket,
                            ServerFrame::Pong {
                                server_time_ms: None,
                            },
                        )
                        .await
                    }
                    ClientFrame::Ephemeral { .. } => {}
                    other => panic!("publication before actual upload completion: {other:?}"),
                }
            }
            assert!(matches!(answered.await.unwrap(), Ok(Reply::Link(None))));
            assert_eq!(sender.lock().pending_uploads(8), queued);
            release_http.send(()).unwrap();
        }
        let marker = async {
            while sender
                .lock()
                .store
                .outbox()
                .unwrap()
                .iter()
                .any(|row| !row.failed)
            {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            let (answer, answered) = oneshot::channel();
            commands_tx
                .send(Cmd::Req {
                    op: Op::FetchLink {
                        id: "upload-quiescent".into(),
                    },
                    reply: answer,
                })
                .unwrap();
            assert!(matches!(answered.await.unwrap(), Ok(Reply::Link(None))));
            commands_tx.send(Cmd::Stop).unwrap();
        };
        let wire = async {
            loop {
                match request(&mut socket).await {
                    ClientFrame::Publish { env } => {
                        let cid = env.client_id().to_string();
                        let ev = sequence(env, &mut head);
                        assert_eq!(receiver.ingest(ev.clone()), Ingest::Applied);
                        reply(
                            &mut socket,
                            ServerFrame::Accepted {
                                space: space.clone(),
                                client_id: cid,
                                seq: ev.seq,
                            },
                        )
                        .await;
                        reply(&mut socket, ServerFrame::Event { ev }).await;
                    }
                    ClientFrame::Req {
                        id,
                        op: Op::FetchLink { id: marker },
                    } if marker == "upload-quiescent" => {
                        reply(
                            &mut socket,
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::Link(None)),
                            },
                        )
                        .await;
                        break;
                    }
                    ClientFrame::Req { id, .. } => {
                        reply(
                            &mut socket,
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::Done),
                            },
                        )
                        .await
                    }
                    ClientFrame::Ping => {
                        reply(
                            &mut socket,
                            ServerFrame::Pong {
                                server_time_ms: None,
                            },
                        )
                        .await
                    }
                    ClientFrame::Ephemeral { .. } => {}
                    other => panic!("unexpected relay request: {other:?}"),
                }
            }
        };
        tokio::join!(marker, wire);
        close_socket.await.unwrap();
    };
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
        return;
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
}

#[test]
fn terminal_upload_rolls_back_outbox_and_blob_together() {
    let sender = Client::open(":memory:".into(), "en".into()).unwrap();
    sender
        .create_account(
            "Rollback".into(),
            "rollback".into(),
            "http://127.0.0.1:1".into(),
            Arc::new(Vault::default()),
        )
        .unwrap();
    let space = sender
        .create_group_with("Atomic upload failure".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let mut head = None;
    let setup = sender.lock().store.outbox().unwrap();
    for row in setup {
        assert_eq!(
            sender
                .lock()
                .ingest(sequence(Envelope::plain(&row.event), &mut head)),
            Ingest::Confirmed
        );
    }
    let account = sender.account().unwrap();
    let scope = MediaDraftScope {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: space.clone(),
        thread_root_id: None,
    };
    let saved = sender.save_media_draft(scope.clone(), "0".into(), serde_json::json!({"kind":"attachments", "options":{"presentation":"file", "grouping":"grouped"}, "items":[{"asset_id":"a", "caption":"Retain me", "mutation_id":"atomic-a"},{"asset_id":"b", "caption":"Second member", "mutation_id":"atomic-b"}]}).to_string(), vec![MediaDraftAssetInput { id:"a".into(), name:"a.pdf".into(), mime:"application/pdf".into(), bytes:b"private atomic PDF".to_vec() }, MediaDraftAssetInput { id:"b".into(), name:"b.pdf".into(), mime:"application/pdf".into(), bytes:b"second pending blob".to_vec() }]).unwrap();
    let sent = sender
        .send_media_draft(scope, saved.draft.revision)
        .unwrap();
    let later = sender
        .send_message(space.clone(), "Ordinary after aborted album".into())
        .unwrap();
    let mut engine = sender.lock();
    let uploads = engine.pending_uploads(8);
    assert_eq!(uploads.len(), 2);
    let blob = &uploads[0].0;
    assert!(
        engine.outbox_envelopes().is_empty(),
        "a queued File must wait for real upload success"
    );
    engine.store.conn().execute_batch("CREATE TEMP TRIGGER fail_upload_outbox BEFORE UPDATE OF failed ON outbox WHEN NEW.failed = 1 BEGIN SELECT RAISE(ABORT, 'injected refusal transaction failure'); END;").unwrap();
    assert!(engine.upload_refused(blob).is_err());
    assert_eq!(engine.pending_uploads(8), uploads);
    assert!(engine
        .store
        .outbox()
        .unwrap()
        .iter()
        .all(|row| !row.failed && row.last_error.is_none() && row.attempts == 0));
    assert_ne!(
        engine
            .timeline(&space)
            .unwrap()
            .iter()
            .find(|entry| entry.id == sent.entries[0].id)
            .unwrap()
            .delivery,
        crate::Delivery::Failed
    );
    engine
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_upload_outbox")
        .unwrap();
    assert_eq!(engine.upload_refused(blob).unwrap(), vec![space.clone()]);
    assert_eq!(
        engine.pending_uploads(8).len(),
        1,
        "the second blob is deliberately not acknowledged"
    );
    assert!(
        engine
            .outbox_envelopes()
            .iter()
            .any(|env| env.client_id() == later.id),
        "an aborted album's orphan File must not hold a later ordinary head"
    );
    assert!(engine.store.upload_pending(blob).unwrap());
    assert_eq!(
        engine
            .timeline(&space)
            .unwrap()
            .iter()
            .find(|entry| entry.id == sent.entries[0].id)
            .unwrap()
            .delivery,
        crate::Delivery::Failed
    );
}

fn http_request(socket: &mut std::net::TcpStream) -> (String, Vec<u8>) {
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut headers = Vec::new();
    let mut byte = [0];
    while !headers.ends_with(b"\r\n\r\n") {
        socket.read_exact(&mut byte).unwrap();
        headers.push(byte[0]);
        assert!(headers.len() < 16 * 1024);
    }
    let headers = String::from_utf8(headers).unwrap();
    let length: usize = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|value| value.trim().parse().unwrap())
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    socket.read_exact(&mut body).unwrap();
    (headers, body)
}

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

async fn terminal_upload(status: u16) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let directory = Directory(std::env::temp_dir().join(types::new_id("upload-refusal")));
    std::fs::create_dir(&directory.0).unwrap();
    let receiver_path = directory.0.join("receiver.sqlite");
    let mut receiver = Engine::open(receiver_path.to_str().unwrap()).unwrap();
    let (_, receiver_secret, _) = receiver.create_account("Reader", "reader", &url).unwrap();
    let sender_path = directory.0.join("sender.sqlite");
    let sender = Client::open(sender_path.to_str().unwrap().into(), "en".into()).unwrap();
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
    let content = serde_json::json!({"kind":"attachments", "options":{"presentation":"file", "grouping":"separate"},
        "items":[{"asset_id":"a", "caption":"Keep this caption", "mutation_id":"upload-a"}]}).to_string();
    let saved = sender.save_media_draft(scope.clone(), "0".into(), content, vec![MediaDraftAssetInput {
        id: "a".into(), name: "fixture.pdf".into(), mime: "application/pdf".into(), bytes: plain.clone(),
    }]).unwrap();
    let sent = sender.send_media_draft(scope, saved.draft.revision).unwrap();
    let message = sent.entries[0].id.clone();
    let queued = sender.lock().pending_uploads(8);
    assert_eq!(queued.len(), 1);
    let (blob, ciphertext) = queued[0].clone();
    assert_eq!(hex::encode(Sha256::digest(&ciphertext)), blob);
    let http = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", http.local_addr().unwrap());
    let expected_blob = blob.clone();
    let expected_ciphertext = ciphertext.clone();
    let http_server = std::thread::spawn(move || {
        let (mut socket, _) = http.accept().unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut headers = Vec::new();
        let mut byte = [0];
        while !headers.ends_with(b"\r\n\r\n") {
            socket.read_exact(&mut byte).unwrap();
            headers.push(byte[0]);
            assert!(headers.len() < 16 * 1024);
        }
        let headers = String::from_utf8(headers).unwrap();
        assert!(headers.starts_with(&format!("PUT /v1/blobs/{expected_blob} ")));
        let length: usize = headers.lines().find_map(|line| {
            line.to_ascii_lowercase().strip_prefix("content-length:").map(|value| value.trim().parse().unwrap())
        }).unwrap();
        let mut body = vec![0; length];
        socket.read_exact(&mut body).unwrap();
        assert_eq!(body, expected_ciphertext);
        write!(socket, "HTTP/1.1 {status} Refused\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let lease = EngineLease::new(sender.inner.clone());
    let (_, key, _, _, _, _) = sender.lock().net_credentials().unwrap();
    let worker = tokio::spawn(media_worker(lease.clone(), None, base, "upload-fixture".into(), key, Arc::new(tokio::sync::Notify::new())));
    tokio::time::timeout(Duration::from_secs(5), async {
        while !sender.lock().pending_uploads(8).is_empty() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }).await.expect("upload response was not processed");
    worker.abort();
    let _ = worker.await;
    lease.revoke();
    http_server.join().unwrap();

    // Drive actual session/Accepted/Event handling to quiescence. The ordered
    // request is a wire barrier; a sleep is not evidence of absent publication.
    let (commands_tx, mut commands_rx) = mpsc::unbounded_channel();
    let (status_tx, _) = watch::channel(NetStatus { state: ConnState::Connecting, synced: false, error: None, registered: true, authenticated_relay: None });
    sender.lock().set_registered(true).unwrap();
    let ctx = Ctx { engine: EngineLease::new(sender.inner.clone()), listener: None, status: status_tx, lang: crate::i18n::Lang::En, maintenance_work: None };
    let client = async {
        let mut backoff = Duration::from_millis(500);
        session(&ctx, &mut commands_rx, &mut backoff).await
    };
    let relay = async {
        let mut socket = catch_up_socket(&listener).await;
        reply(&mut socket, ServerFrame::SyncDone).await;
        let marker = async {
            while sender.lock().store.outbox().unwrap().iter().any(|row| !row.failed) {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            let (answer, answered) = oneshot::channel();
            commands_tx.send(Cmd::Req { op: Op::FetchLink { id: "upload-quiescent".into() }, reply: answer }).unwrap();
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
                        reply(&mut socket, ServerFrame::Accepted { space: space.clone(), client_id: cid, seq: ev.seq }).await;
                        reply(&mut socket, ServerFrame::Event { ev }).await;
                    }
                    ClientFrame::Req { id, op: Op::FetchLink { id: marker } } if marker == "upload-quiescent" => {
                        reply(&mut socket, ServerFrame::Res { id, result: Ok(Reply::Link(None)) }).await;
                        break;
                    }
                    ClientFrame::Req { id, .. } => reply(&mut socket, ServerFrame::Res { id, result: Ok(Reply::Done) }).await,
                    ClientFrame::Ping => reply(&mut socket, ServerFrame::Pong { server_time_ms: None }).await,
                    ClientFrame::Ephemeral { .. } => {},
                    other => panic!("unexpected relay request: {other:?}"),
                }
            }
        };
        tokio::join!(marker, wire);
    };
    let (exit, ()) = tokio::time::timeout(Duration::from_secs(10), async { tokio::join!(client, relay) }).await.expect("upload refusal did not reach durable quiescence");
    assert!(matches!(exit, Exit::Stop));
    ctx.engine.revoke();
    drop(ctx);
    drop(sender);
    drop(receiver);
    let mut receiver = Engine::open(receiver_path.to_str().unwrap()).unwrap();
    assert!(receiver.unlock(Some(receiver_secret.to_vec())).unwrap());
    let sender = Client::open(sender_path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(sender.unlock(vault).unwrap());
    let local = sender.timeline(space.clone()).unwrap().into_iter().find(|entry| entry.id == message).unwrap();
    let remote = receiver.timeline(&space).unwrap();
    eprintln!("HTTP {status}: cold_sender={:?} cold_receiver_messages={} retained_uploads={}", local.delivery, remote.iter().filter(|entry| entry.id == message).count(), sender.lock().uploads_pending());
    assert!(!remote.iter().any(|entry| entry.id == message), "refused bytes must not acquire confirmed remote message delivery");
    assert_eq!(local.delivery, crate::Delivery::Failed, "terminal upload must retain a readable durable failure");
    assert_eq!(local.kind, sent.entries[0].kind);
    assert_eq!(local.media, sent.entries[0].media);
    assert_eq!(sender.lock().media(&hex::encode(Sha256::digest(&plain))).unwrap(), Some(plain));
}

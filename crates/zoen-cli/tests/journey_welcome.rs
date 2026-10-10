//! A delayed Welcome must hold the sender's messages until the recipient can open them.

mod common;
use common::World;
use futures_util::{SinkExt, StreamExt};
use roda_log::content::SealedKind;
use roda_proto::ClientFrame;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test(flavor = "multi_thread")]
async fn messages_wait_for_welcome_confirmation() {
    let w = World::new("welcome_order").await;
    w.init("bruno", "Bruno");

    // This socket forwards the real protocol unchanged, except for one Welcome held
    // until the test releases it. The relay cannot confirm something it hasn't received.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let relay = format!("ws://127.0.0.1:{}/v1/sync", w.port);
    let (welcome_tx, welcome_rx) = oneshot::channel();
    let (release_tx, mut release_rx) = oneshot::channel();
    let (early_tx, mut early_rx) = mpsc::unbounded_channel();
    // Context uploads use HTTP concurrently with sync. Admit them while the
    // protocol proxy holds a Welcome; otherwise the upload would deadlock sync.
    let (clients_tx, mut clients_rx) = mpsc::unbounded_channel();
    let relay_port = w.port;
    let acceptor = tokio::spawn(async move {
        let mut http = tokio::task::JoinSet::new();
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = [0; 4096];
            let sync = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let n = socket.peek(&mut head).await.unwrap();
                    if n == 0 {
                        return false;
                    }
                    if let Some(end) = head[..n].windows(2).position(|s| s == b"\r\n") {
                        return head[..end].starts_with(b"GET /v1/sync ");
                    }
                    assert!(
                        n < head.len(),
                        "proxy request line exceeds its fixture bound"
                    );
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .unwrap();
            if sync {
                if clients_tx.send(socket).is_err() {
                    break;
                }
            } else {
                http.spawn(async move {
                    let mut upstream = tokio::net::TcpStream::connect(("127.0.0.1", relay_port))
                        .await
                        .unwrap();
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut upstream).await;
                });
            }
            while http.try_join_next().is_some() {}
        }
    });
    let proxy = tokio::spawn(async move {
        let mut welcome_tx = Some(welcome_tx);
        let mut released = false;
        loop {
            let Some(socket) = clients_rx.recv().await else {
                break;
            };
            let client = tokio_tungstenite::accept_async(socket).await.unwrap();
            let (upstream, _) = tokio_tungstenite::connect_async(&relay).await.unwrap();
            let (mut to_client, mut from_client) = client.split();
            let (mut to_relay, mut from_relay) = upstream.split();
            let mut held = None;
            loop {
                tokio::select! {
                    frame = from_client.next() => {
                        let Some(Ok(frame)) = frame else { break };
                        if let Message::Binary(bytes) = &frame {
                            if let Ok(ClientFrame::Publish { env }) = ClientFrame::decode(bytes) {
                                let kind = env.sealed_data().map(|(k, _)| k);
                                if kind == Some(SealedKind::Welcome) && !released {
                                    held = Some(frame);
                                    if let Some(tx) = welcome_tx.take() { let _ = tx.send(()); }
                                    continue;
                                }
                                if kind == Some(SealedKind::Application) && held.is_some() {
                                    let _ = early_tx.send(env.client_id().to_string());
                                }
                            }
                        }
                        if to_relay.send(frame).await.is_err() { break }
                    }
                    frame = from_relay.next() => {
                        let Some(Ok(frame)) = frame else { break };
                        if to_client.send(frame).await.is_err() { break }
                    }
                    _ = &mut release_rx, if !released => {
                        released = true;
                        if let Some(frame) = held.take() {
                            if to_relay.send(frame).await.is_err() { break }
                        }
                    }
                }
            }
        }
    });

    // The account keeps its relay URL, so register it through the proxy too.
    w.zoen_at(port, "ana", &["init", "--name", "Ana", "--handle", "ana"]);
    let sender = w.spawn_zoen_at(port, "ana", &["dm", "@bruno", "primeira mensagem"]);
    tokio::time::timeout(Duration::from_secs(30), welcome_rx)
        .await
        .expect("the sender never published its Welcome")
        .unwrap();
    let early = tokio::time::timeout(Duration::from_secs(2), early_rx.recv()).await;
    release_tx.send(()).unwrap();
    let out = sender.wait_with_output().unwrap();
    proxy.abort();
    acceptor.abort();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        early.is_err(),
        "an application message overtook the unconfirmed Welcome: {early:?}"
    );
    let read = w.zoen("bruno", &["read", "@ana"]);
    assert!(read.contains("primeira mensagem"), "{read}");
    assert!(!w.zoen("bruno", &["verify"]).contains("BROKEN"));
}

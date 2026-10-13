//! A linked device receipts its box, and a later fetch finds nothing.

mod common;
use common::*;
use futures_util::{SinkExt, StreamExt};
use roda_log::{Author, Signer};
use roda_proto::{Op, Reply, ServerFrame};
use sqlx::Connection;
use std::{
    io::BufRead,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio_tungstenite::tungstenite::Message;

fn lines_of(child: &mut std::process::Child) -> std::sync::mpsc::Receiver<String> {
    let out = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

fn wait_linked(child: &mut std::process::Child, rx: &std::sync::mpsc::Receiver<String>) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    let mut codes = 1u32;
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left.min(std::time::Duration::from_millis(200))) {
            Ok(line) if line.starts_with("linked:") => {
                assert_eq!(codes, 1, "a lost fetch must not ask for a new code");
                return line["linked:".len()..].trim().to_string();
            }
            Ok(line) if line.starts_with("code:") => codes += 1,
            Ok(_) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if std::time::Instant::now() >= deadline {
                    panic!("no \"linked:\" line");
                }
                if let Ok(Some(status)) = child.try_wait() {
                    panic!(
                        "link-request exited {status} before linked:\n{}",
                        child_stderr(child)
                    );
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                let _ = child.wait();
                panic!("stdout closed before linked:\n{}", child_stderr(child));
            }
        }
    }
}

fn child_stderr(child: &mut std::process::Child) -> String {
    let Some(mut stderr) = child.stderr.take() else {
        return String::new();
    };
    let mut buf = String::new();
    let _ = std::io::Read::read_to_string(&mut stderr, &mut buf);
    buf
}

fn wait_line(rx: &std::sync::mpsc::Receiver<String>, prefix: &str) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok(line) if line.starts_with(prefix) => return line[prefix.len()..].trim().to_string(),
            Ok(_) => {}
            Err(_) => panic!("no {prefix:?} line"),
        }
    }
}

fn box_id(secret: &[u8; 32], label: &str) -> String {
    let mut bytes = b"zoen-link-box-v1\0".to_vec();
    bytes.extend(secret);
    bytes.extend(label.as_bytes());
    roda_log::sha256_hex(&bytes)
}

#[tokio::test]
async fn a_linked_device_receipts_its_box_and_a_later_fetch_finds_nothing() {
    let w = World::new("linkreceipt").await;
    w.init("ana", "Ana");
    let mut notebook = w.spawn_zoen("ana-notebook", &["link-request", "--for", "90"]);
    let rx = lines_of(&mut notebook);
    let code = wait_line(&rx, "code:");
    let hexed = code
        .strip_prefix("zoen-link:1:")
        .expect("link code")
        .to_string();
    assert_eq!(hexed.len(), 192, "{code}");
    let secret: [u8; 32] = hex::decode(&hexed[128..]).unwrap().try_into().unwrap();
    let identity_box = box_id(&secret, "identity");
    let history_box = box_id(&secret, "history");
    let linked = w.zoen("ana", &["link", &code]);
    assert!(linked.contains("history sent:"), "{linked}");
    let linked_line = wait_line(&rx, "linked:");
    assert!(linked_line.starts_with("@ana "), "{linked_line}");
    let out = notebook.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mut pg = sqlx::PgConnection::connect(&w.db_url).await.unwrap();
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM link_boxes")
        .fetch_one(&mut pg)
        .await
        .unwrap();
    assert_eq!(left, 0);

    let read_key = |name: &str| {
        let bytes: [u8; 32] = std::fs::read(w.dir.join("ana-notebook/vault").join(name))
            .unwrap()
            .try_into()
            .unwrap();
        Signer::from_secret(&bytes)
    };
    let author = Author::device(&read_key("zoen.identity.v1"), read_key("zoen.device.v1"));
    let mut client = RawClient::reconnect(&w.relay_url(), author).await;
    assert_eq!(
        client
            .request(Op::FetchLink {
                id: identity_box.clone(),
            })
            .await
            .unwrap(),
        Reply::Link(None)
    );
    assert_eq!(
        client
            .request(Op::FetchLink { id: history_box })
            .await
            .unwrap(),
        Reply::Link(None)
    );
}

#[tokio::test]
async fn a_lost_sealed_fetch_finishes_on_the_retry_without_a_new_code() {
    let w = World::new("linkloss").await;
    w.init("ana", "Ana");
    let (proxy, _proxy) = sealed_drop_proxy(w.port);
    let mut notebook = w.spawn_zoen_env(
        "ana-notebook",
        &["link-request", "--relay", &proxy, "--for", "90"],
        &[],
    );
    let rx = lines_of(&mut notebook);
    let code = wait_line(&rx, "code:");
    let linked = w.zoen("ana", &["link", &code]);
    assert!(linked.contains("history sent:"), "{linked}");
    let linked_line = wait_linked(&mut notebook, &rx);
    assert!(linked_line.starts_with("@ana "), "{linked_line}");
    let out = notebook.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut extra_codes = String::new();
    while let Ok(line) = rx.recv_timeout(Duration::from_secs(2)) {
        if line.starts_with("code:") {
            extra_codes.push_str(&line);
            extra_codes.push('\n');
        }
    }
    assert!(extra_codes.is_empty(), "{extra_codes}");
}

/// The journey blocks on child processes, so the proxy runs on its own runtime.
struct ProxyStop {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for ProxyStop {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn sealed_drop_proxy(relay_port: u16) -> (String, ProxyStop) {
    let (url_tx, url_rx) = std::sync::mpsc::sync_channel(1);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let thread = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("proxy runtime");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("proxy listen");
            let _ = url_tx.send(format!("http://{}", listener.local_addr().unwrap()));
            let dropped = Arc::new(AtomicBool::new(false));
            loop {
                if stop_thread.load(Ordering::SeqCst) {
                    break;
                }
                tokio::select! {
                    next = listener.accept() => {
                        let Ok((socket, _)) = next else { break };
                        let dropped = dropped.clone();
                        tokio::spawn(async move {
                            forward_except_first_sealed(socket, relay_port, dropped).await;
                        });
                    }
                    _ = tokio::time::sleep(Duration::from_millis(200)) => {}
                }
            }
        });
    });
    let url = url_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("proxy bind");
    (
        url,
        ProxyStop {
            stop,
            thread: Some(thread),
        },
    )
}

async fn forward_except_first_sealed(
    mut socket: tokio::net::TcpStream,
    relay_port: u16,
    dropped: Arc<AtomicBool>,
) {
    let websocket = tokio::time::timeout(Duration::from_secs(5), async {
        let mut header = [0u8; 32];
        loop {
            let n = socket.peek(&mut header).await.ok()?;
            if n == 0 {
                return None;
            }
            if n >= b"GET /v1/sync ".len() {
                return Some(header[..n].starts_with(b"GET /v1/sync "));
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .ok()
    .flatten();
    let Some(websocket) = websocket else {
        return;
    };
    if !websocket {
        if let Ok(mut upstream) = tokio::net::TcpStream::connect(("127.0.0.1", relay_port)).await {
            let _ = tokio::io::copy_bidirectional(&mut socket, &mut upstream).await;
        }
        return;
    }
    let Ok(front) = tokio_tungstenite::accept_async(socket).await else {
        return;
    };
    let Ok((back, _)) =
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{relay_port}/v1/sync")).await
    else {
        return;
    };
    let (mut front_tx, mut front_rx) = front.split();
    let (mut back_tx, mut back_rx) = back.split();
    loop {
        tokio::select! {
            msg = front_rx.next() => {
                let Some(Ok(msg)) = msg else { break };
                if back_tx.send(msg).await.is_err() { break }
            }
            msg = back_rx.next() => {
                let Some(Ok(msg)) = msg else { break };
                if let Message::Binary(bytes) = &msg {
                    if let Ok(ServerFrame::Res { result: Ok(Reply::Link(Some(_))), .. }) = ServerFrame::decode(bytes) {
                        if dropped
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                        {
                            let _ = front_tx.send(Message::Close(None)).await;
                            break;
                        }
                    }
                }
                if front_tx.send(msg).await.is_err() { break }
            }
        }
    }
}

//! Test-only transport faults around actual relay requests and committed replies.
use futures_util::{SinkExt, StreamExt};
use roda_proto::{ClientFrame, KeyPackageRecord, Op, Reply, ServerFrame};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, Notify},
    task::{JoinHandle, JoinSet},
};
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone, Copy)]
pub enum Boundary {
    Request,
    Reply,
    AbsentCapability,
    AbsentClock,
    MalformedClock,
}

#[derive(Debug)]
pub enum Observed {
    ClockRefreshed,
    Request {
        operation: Option<String>,
        targets: Vec<String>,
    },
    Reply {
        operation: Option<String>,
        records: Vec<KeyPackageRecord>,
        held: bool,
    },
}

pub struct ClaimProxy {
    pub url: String,
    pub events: mpsc::UnboundedReceiver<Observed>,
    pub release: Arc<Notify>,
    pub disconnect: Arc<Notify>,
    task: JoinHandle<()>,
}

impl ClaimProxy {
    pub async fn new(port: u16, boundary: Boundary) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (events, rx) = mpsc::unbounded_channel();
        let release = Arc::new(Notify::new());
        let disconnect = Arc::new(Notify::new());
        let once = Arc::new(AtomicBool::new(true));
        let release_task = release.clone();
        let disconnect_task = disconnect.clone();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    next = listener.accept() => {
                        let (socket, _) = next.unwrap();
                        connections.spawn(forward(
                            socket, port, boundary, once.clone(), events.clone(),
                            release_task.clone(), disconnect_task.clone(),
                        ));
                    }
                    Some(result) = connections.join_next(), if !connections.is_empty() => {
                        result.unwrap();
                    }
                }
            }
        });
        Self {
            url,
            events: rx,
            release,
            disconnect,
            task,
        }
    }

    pub async fn next(&mut self) -> Observed {
        tokio::time::timeout(Duration::from_secs(8), self.events.recv())
            .await
            .expect("claim boundary was not reached")
            .expect("proxy stopped")
    }
}

impl Drop for ClaimProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn forward(
    mut socket: TcpStream,
    port: u16,
    boundary: Boundary,
    once: Arc<AtomicBool>,
    events: mpsc::UnboundedSender<Observed>,
    release: Arc<Notify>,
    disconnect: Arc<Notify>,
) {
    // HTTP blobs use the same base URL as the socket. Forward their actual bytes too.
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
    let Some(websocket) = websocket else { return };
    if !websocket {
        if let Ok(mut upstream) = TcpStream::connect(("127.0.0.1", port)).await {
            let _ = tokio::io::copy_bidirectional(&mut socket, &mut upstream).await;
        }
        return;
    }
    let Ok(front) = tokio_tungstenite::accept_async(socket).await else {
        return;
    };
    let Ok((back, _)) =
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/v1/sync")).await
    else {
        return;
    };
    let (mut front_tx, mut front_rx) = front.split();
    let (mut back_tx, mut back_rx) = back.split();
    let mut claims = HashMap::new();
    loop {
        tokio::select! {
            msg = front_rx.next() => {
                let Some(Ok(msg)) = msg else { break };
                if let Message::Binary(bytes) = &msg {
                    if let Ok(ClientFrame::Req { id, op: Op::ClaimKeyPackages { ids, operation_id } }) = ClientFrame::decode(bytes) {
                        let held = matches!(boundary, Boundary::Request | Boundary::Reply) && once.swap(false, Ordering::SeqCst);
                        claims.insert(id, (operation_id.clone(), held));
                        let _ = events.send(Observed::Request { operation: operation_id, targets: ids });
                        if held && matches!(boundary, Boundary::Request) {
                            tokio::select! {
                                _ = release.notified() => {},
                                _ = disconnect.notified() => break,
                            }
                        }
                    }
                }
                if back_tx.send(msg).await.is_err() { break }
            }
            msg = back_rx.next() => {
                let Some(Ok(mut msg)) = msg else { break };
                if let Message::Binary(bytes) = &msg {
                    if let Ok(frame) = ServerFrame::decode(bytes) {
                        match frame {
                            ServerFrame::Challenge { nonce, relay, protocol, mut capabilities, mut server_time_ms } => {
                                match boundary {
                                    Boundary::AbsentCapability => capabilities.retain(|c| c != roda_proto::KEY_PACKAGE_CLAIM_CAPABILITY),
                                    Boundary::AbsentClock => server_time_ms = None,
                                    Boundary::MalformedClock => server_time_ms = Some(-1),
                                    Boundary::Request | Boundary::Reply => {},
                                }
                                msg = Message::Binary(ServerFrame::Challenge { nonce, relay, protocol, capabilities, server_time_ms }.encode().into());
                            }
                            ServerFrame::Pong { server_time_ms: Some(time) }
                                if matches!(boundary, Boundary::AbsentClock | Boundary::MalformedClock)
                                    && roda_proto::KeyPackageClaimClock::from_server_ms(time).is_some()
                                    && once.swap(false, Ordering::SeqCst) => {
                                let _ = events.send(Observed::ClockRefreshed);
                            }
                            ServerFrame::Res { id, result: Ok(Reply::KeyPackages(records)) } => {
                                if let Some((operation, selected)) = claims.remove(&id) {
                                    let held = selected && matches!(boundary, Boundary::Reply);
                                    let _ = events.send(Observed::Reply { operation, records, held });
                                    if held {
                                        tokio::select! {
                                            _ = release.notified() => {},
                                            _ = disconnect.notified() => break,
                                        }
                                    }
                                }
                            }
                            _ => {},
                        }
                    }
                }
                if front_tx.send(msg).await.is_err() { break }
            }
        }
    }
}

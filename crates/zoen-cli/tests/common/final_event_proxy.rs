//! Test-owned, one-frame wire loss AFTER relay B's actual NATS subscriber/WS writer.
//! Does not alter Hello/Auth, Sync, Ping/Pong, timing, relay storage or broker state.
use futures_util::{SinkExt, StreamExt};
use protocol::{ClientFrame, Cursor, EphemeralKind, ServerFrame};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    net::{TcpListener, TcpStream},
    task::{JoinHandle, JoinSet},
};
use tokio_tungstenite::tungstenite::Message;
use types::EventBody;

#[derive(Clone, Debug)]
pub enum Kind {
    Open,
    Close,
    TransportError(String),
    Sync {
        cursors: Vec<Cursor>,
        all: bool,
    },
    SyncDone,
    Ping,
    Pong,
    Joined(String),
    Publish,
    Delivered {
        space: String,
        seq: u64,
    },
    Event {
        space: String,
        id: String,
        seq: u64,
        hash: String,
        dropped: bool,
    },
    RelayError(String),
}

#[derive(Clone, Debug)]
pub struct Record {
    pub connection: u64,
    pub at: Instant,
    pub elapsed_ms: u128,
    pub kind: Kind,
}

pub(super) struct Selector {
    pub(super) space: String,
    pub(super) author: String,
    pub(super) text: String,
    pub(super) consumed: bool,
}

pub(super) struct State {
    born: Instant,
    connections: u64,
    pub(super) selector: Option<Selector>,
    pub(super) records: Vec<Record>,
}

impl State {
    fn record(&mut self, connection: u64, kind: Kind) {
        let at = Instant::now();
        self.records.push(Record {
            connection,
            at,
            elapsed_ms: at.duration_since(self.born).as_millis(),
            kind,
        });
    }
}

pub struct FinalEventProxy {
    pub url: String,
    pub(super) state: Arc<Mutex<State>>,
    evidence: PathBuf,
    task: JoinHandle<()>,
}

impl FinalEventProxy {
    pub async fn new(port: u16, evidence: PathBuf) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State {
            born: Instant::now(),
            connections: 0,
            selector: None,
            records: Vec::new(),
        }));
        let task_state = state.clone();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (socket, _) = accepted.unwrap();
                        connections.spawn(forward(socket, port, task_state.clone()));
                    }
                    Some(result) = connections.join_next(), if !connections.is_empty() => {
                        result.unwrap();
                    }
                }
            }
        });
        Self {
            url,
            state,
            evidence,
            task,
        }
    }

    pub fn snapshot(&self) -> Vec<Record> {
        self.state.lock().unwrap().records.clone()
    }

    pub async fn until(
        &self,
        timeout: Duration,
        label: &str,
        predicate: impl Fn(&[Record]) -> bool,
    ) -> Vec<Record> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let records = self.snapshot();
            if predicate(&records) {
                return records;
            }
            if tokio::time::Instant::now() >= deadline {
                // Preserve the missing-repair negative oracle BEFORE Device::drop's
                // intentional Stop closes the socket during panic cleanup.
                std::fs::write(
                    self.evidence.join("pre-cleanup-timeout-trace.txt"),
                    format!("{label}\n{records:#?}\n"),
                )
                .unwrap();
                panic!("{label}; trace:\n{records:#?}");
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    pub fn save(&self) {
        std::fs::create_dir_all(&self.evidence).unwrap();
        let trace = format!("{:#?}\n", self.snapshot());
        // This call is the explicit pre-Stop online witness. Drop may subsequently
        // record the test's intentional transport cleanup in wire-trace.txt.
        std::fs::write(self.evidence.join("online-witness-trace.txt"), &trace).unwrap();
        std::fs::write(self.evidence.join("wire-trace.txt"), trace).unwrap();
    }
}

impl Drop for FinalEventProxy {
    fn drop(&mut self) {
        // Evidence contains only fresh synthetic account IDs, cursors and frame metadata.
        // Keep the trace even when a qualification assertion panics.
        let _ = std::fs::create_dir_all(&self.evidence);
        let _ = std::fs::write(
            self.evidence.join("wire-trace.txt"),
            format!("{:#?}\n", self.snapshot()),
        );
        self.task.abort(); // Dropping its JoinSet also aborts only this proxy's children.
    }
}

async fn forward(mut socket: TcpStream, port: u16, state: Arc<Mutex<State>>) {
    // Preserve real HTTP profile/blob operations on the same client base URL.
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
    let connection = {
        let mut state = state.lock().unwrap();
        state.connections += 1;
        let id = state.connections;
        state.record(id, Kind::Open);
        id
    };
    let result = forward_ws(socket, port, connection, &state).await;
    let mut state = state.lock().unwrap();
    if let Err(error) = result {
        state.record(connection, Kind::TransportError(error));
    }
    state.record(connection, Kind::Close);
}

async fn forward_ws(
    socket: TcpStream,
    port: u16,
    connection: u64,
    state: &Arc<Mutex<State>>,
) -> Result<(), String> {
    let front = tokio_tungstenite::accept_async(socket)
        .await
        .map_err(|e| e.to_string())?;
    let (back, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/v1/sync"))
        .await
        .map_err(|e| e.to_string())?;
    let (mut front_tx, mut front_rx) = front.split();
    let (mut back_tx, mut back_rx) = back.split();
    loop {
        tokio::select! {
            msg = front_rx.next() => {
                let Some(msg) = msg else { return Ok(()) };
                let msg = msg.map_err(|e| e.to_string())?;
                if let Message::Binary(bytes) = &msg {
                    let frame = ClientFrame::decode(bytes).map_err(|e| e.to_string())?;
                    let kind = match frame {
                        ClientFrame::Sync { cursors, all } => Some(Kind::Sync { cursors, all }),
                        ClientFrame::Ping => Some(Kind::Ping),
                        ClientFrame::Publish { .. } => Some(Kind::Publish),
                        ClientFrame::Ephemeral { space, kind: EphemeralKind::Delivered { seq } } => Some(Kind::Delivered { space, seq }),
                        _ => None,
                    };
                    if let Some(kind) = kind { state.lock().unwrap().record(connection, kind); }
                }
                back_tx.send(msg).await.map_err(|e| e.to_string())?;
            }
            msg = back_rx.next() => {
                let Some(msg) = msg else { return Ok(()) };
                let msg = msg.map_err(|e| e.to_string())?;
                let mut dropped = false;
                if let Message::Binary(bytes) = &msg {
                    let frame = ServerFrame::decode(bytes).map_err(|e| e.to_string())?;
                    let mut state = state.lock().unwrap();
                    let kind = match frame {
                        ServerFrame::Event { ev } => {
                            if let Some(selector) = &mut state.selector {
                                let selected = !selector.consumed
                                    && ev.env.space() == selector.space
                                    && ev.env.author() == selector.author
                                    && ev.plain_event().is_some_and(|event| matches!(event.body,
                                        EventBody::MessagePosted { text, .. } if text == selector.text));
                                if selected { selector.consumed = true; dropped = true; }
                            }
                            Some(Kind::Event {
                                space: ev.env.space().into(), id: ev.env.client_id().into(),
                                seq: ev.seq, hash: ev.hash, dropped,
                            })
                        }
                        ServerFrame::Joined { space } => Some(Kind::Joined(space)),
                        ServerFrame::SyncDone => Some(Kind::SyncDone),
                        ServerFrame::Pong { .. } => Some(Kind::Pong),
                        ServerFrame::Error { code, message } => Some(Kind::RelayError(format!("{code:?}: {message}"))),
                        _ => None,
                    };
                    if let Some(kind) = kind { state.record(connection, kind); }
                }
                if !dropped { front_tx.send(msg).await.map_err(|e| e.to_string())?; }
            }
        }
    }
}

//! Actual public Client -> transparent HTTP / binary WebSocket
//! proxy -> OWNED local relay. No Engine/private mutation or fabricated receipt.
//! Use only an explicitly owned native realm and fresh private output directory.
//! Each run creates synthetic accounts through the actual public Client API;
//! random account frames are not an exact cross-runtime transcript differential.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use client_core::{Client, ConnectionDto, CoreListener, Delivery, EntryKind, SecretVault};
use futures_util::{SinkExt, StreamExt};
use protocol::{ClientFrame, EphemeralKind, ServerFrame};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
};
use tokio_tungstenite::{accept_async, connect_async, tungstenite::Message};
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
struct Capture {
    file: Mutex<std::fs::File>,
    frames: Mutex<Vec<Value>>,
    bytes: AtomicU64,
}
impl Capture {
    fn transport(&self, row: Value) {
        let mut file = self.file.lock().unwrap();
        serde_json::to_writer(&mut *file, &row).unwrap();
        writeln!(file).unwrap();
        file.flush().unwrap();
        self.frames.lock().unwrap().push(row);
    }
    fn record(&self, label: &str, generation: u64, outgoing: bool, bytes: &[u8]) {
        let total =
            self.bytes.fetch_add(bytes.len() as u64, Ordering::Relaxed) + bytes.len() as u64;
        assert!(
            total <= 32 * 1024 * 1024,
            "Owned proof transcript capacity exceeded"
        );
        let row = json!({"client":label,"generation":generation,"direction":if outgoing{"client"}else{"server"},"bytes_base64":STANDARD.encode(bytes)});
        let mut file = self.file.lock().unwrap();
        serde_json::to_writer(&mut *file, &row).unwrap();
        writeln!(file).unwrap();
        file.flush().unwrap();
        self.frames.lock().unwrap().push(row);
    }
    fn outgoing(&self, label: &str, space: &str, seq: u64, read: bool) -> bool {
        self.frames.lock().unwrap().iter().any(|row|{
   if row["client"]!=label||row["direction"]!="client"{return false}
   let bytes=STANDARD.decode(row["bytes_base64"].as_str().unwrap()).unwrap();
   matches!(ClientFrame::decode(&bytes),Ok(ClientFrame::Ephemeral{space:actual,kind}) if actual==space &&
    match kind{EphemeralKind::Read{seq:mark}=>read&&mark>=seq,EphemeralKind::Delivered{seq:mark}=>!read&&mark>=seq,_=>false})
  })
    }
    fn replay_before_sync_done(
        &self,
        label: &str,
        generation: u64,
        space: &str,
        from: &str,
        seq: u64,
    ) -> bool {
        let mut delivered = false;
        let mut read = false;
        for row in self.frames.lock().unwrap().iter().filter(|row| {
            row["client"] == label
                && row["direction"] == "server"
                && row["generation"] == generation
        }) {
            let raw = STANDARD
                .decode(row["bytes_base64"].as_str().unwrap())
                .unwrap();
            match ServerFrame::decode(&raw).unwrap() {
                ServerFrame::Ephemeral {
                    space: actual,
                    from: who,
                    kind,
                } if actual == space && who == from => match kind {
                    EphemeralKind::Delivered { seq: mark } => delivered = mark >= seq,
                    EphemeralKind::Read { seq: mark } => read = delivered && mark >= seq,
                    _ => {}
                },
                ServerFrame::SyncDone => return delivered && read,
                _ => {}
            }
        }
        false
    }
}
struct Listener(Arc<Capture>);
impl CoreListener for Listener {
    fn on_change(&self, _: Vec<String>) {}
    fn on_ephemeral(&self, _: String, _: String, _: String, _: String) {}
    fn on_presence(&self, _: String, _: bool) {}
    fn on_connection(&self, _: ConnectionDto) {}
    fn on_error(&self, message: String) {
        writeln!(
            self.0.file.lock().unwrap(),
            "{}",
            json!({"client_error":message})
        )
        .unwrap();
    }
    fn on_profile_changed(&self, _: String) {}
}
struct Proxy {
    url: String,
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
    generation: Arc<AtomicU64>,
    active: Arc<AtomicU64>,
}

// Peeking preserves every request byte, including any body already coalesced
// with the headers. Only the WebSocket handshake is terminated by this proxy;
// normal HTTP remains an opaque TCP tunnel to the same owned relay endpoint.
async fn request_kind(stream: &TcpStream) -> std::io::Result<(String, bool)> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut header = [0u8; 16 * 1024];
        loop {
            let count = stream.peek(&mut header).await?;
            if count == 0 {
                return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
            }
            if let Some(end) = header[..count].windows(4).position(|b| b == b"\r\n\r\n") {
                let text = std::str::from_utf8(&header[..end])
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
                let mut lines = text.split("\r\n");
                let request = lines.next().unwrap_or_default().to_string();
                let websocket = lines.any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("upgrade")
                            && value.trim().eq_ignore_ascii_case("websocket")
                    })
                });
                return Ok((request, websocket));
            }
            if count == header.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Owned proxy HTTP header capacity exceeded",
                ));
            }
            // A partial header remains readable, so avoid spinning on peek.
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))?
}

async fn tunnel_http(
    mut client: TcpStream,
    upstream: std::net::SocketAddr,
) -> std::io::Result<(u64, u64)> {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut server = TcpStream::connect(upstream).await?;
        copy_bidirectional(&mut client, &mut server).await
    })
    .await
    .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))?
}

struct ActiveConnection(Arc<AtomicU64>);
impl ActiveConnection {
    fn new(count: Arc<AtomicU64>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self(count)
    }
}
impl Drop for ActiveConnection {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

async fn proxy(label: &str, target: &str, capture: Arc<Capture>) -> Proxy {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let label = label.to_string();
    let endpoint = reqwest::Url::parse(target).unwrap();
    let upstream = std::net::SocketAddr::from(([127, 0, 0, 1], endpoint.port().unwrap()));
    let target = target.to_string();
    let generation = Arc::new(AtomicU64::new(0));
    let active = Arc::new(AtomicU64::new(0));
    let (stop, mut ended) = watch::channel(false);
    let generations = generation.clone();
    let count = active.clone();
    let task = tokio::spawn(async move {
        let mut jobs = JoinSet::new();
        loop {
            tokio::select! {
             _=ended.changed()=>break,
             Some(result)=jobs.join_next(),if !jobs.is_empty()=>{result.unwrap();},
             accepted=listener.accept()=>{
              assert!(jobs.len()<8,"Owned proxy connection capacity exceeded");
              let(stream,_)=accepted.unwrap();let target=target.clone();let label=label.clone();let capture=capture.clone();
              let generations=generations.clone();let count=count.clone();
              jobs.spawn(async move{
               let (request, websocket)=match request_kind(&stream).await {
                Ok(kind)=>kind,
                Err(error)=>{capture.transport(json!({"client":label,"transport_error":error.to_string()}));return;}
               };
               capture.transport(json!({"client":label,"http_request":request,"websocket":websocket}));
               if !websocket {
                let result=tunnel_http(stream,upstream).await;
                capture.transport(match result {
                 Ok((sent,received))=>json!({"client":label,"http_tunnel_complete":true,"sent_bytes":sent,"received_bytes":received}),
                 Err(error)=>json!({"client":label,"http_tunnel_error":error.to_string()}),
                });
                return;
               }
               // Never conceal an incorrectly composed client URL by forwarding
               // its handshake to the canonical upstream path anyway.
               if request != "GET /v1/sync HTTP/1.1" {
                capture.transport(json!({"client":label,"websocket_path_refused":request}));
                return;
               }
               let generation=generations.fetch_add(1,Ordering::SeqCst)+1;
               let _active=ActiveConnection::new(count);
               let mut client=match accept_async(stream).await {
                Ok(client)=>client,
                Err(error)=>{capture.transport(json!({"client":label,"websocket_accept_error":error.to_string()}));return;}
               };
               let mut server=match connect_async(target.as_str()).await {
                Ok((server,_))=>server,
                Err(error)=>{capture.transport(json!({"client":label,"websocket_connect_error":error.to_string()}));return;}
               };
               loop{tokio::select!{
                incoming=client.next()=>match incoming{Some(Ok(message))=>{
                 if let Message::Binary(bytes)=&message{capture.record(&label,generation,true,bytes);}
                 let close=message.is_close();if server.send(message).await.is_err()||close{break}
                },_=>break},
                incoming=server.next()=>match incoming{Some(Ok(message))=>{
                 if let Message::Binary(bytes)=&message{capture.record(&label,generation,false,bytes);}
                 let close=message.is_close();if client.send(message).await.is_err()||close{break}
                },_=>break},
               }}
              });
             }
            }
        }
        jobs.abort_all();
        while let Some(result) = jobs.join_next().await {
            assert!(result.is_ok() || result.unwrap_err().is_cancelled());
        }
    });
    Proxy {
        // Public Client accepts the relay BASE and appends /v1/sync or /v1/blobs.
        // The CLI target remains the exact upstream WebSocket endpoint.
        url: format!("ws://{address}"),
        stop,
        task,
        generation,
        active,
    }
}
async fn until(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Actual receipt journey deadline");
}
fn open(path: &Path, vault: Arc<Vault>) -> Arc<Client> {
    let client = Client::open(path.to_str().unwrap().into(), "en-US".into()).unwrap();
    assert!(client.unlock(vault).unwrap());
    client
}
fn delivery(client: &Client, space: &str, id: &str) -> Option<Delivery> {
    client
        .timeline(space.into())
        .unwrap()
        .iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.delivery)
}
#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let endpoint = args.next().expect("owned loopback relay WS URL required");
    let output = PathBuf::from(args.next().expect("fresh owned output directory required"));
    assert!(args.next().is_none());
    let parsed = reqwest::Url::parse(&endpoint).unwrap();
    assert_eq!(parsed.scheme(), "ws");
    assert_eq!(parsed.host_str(), Some("127.0.0.1"));
    assert_eq!(parsed.path(), "/v1/sync");
    assert!(parsed.username().is_empty() && parsed.password().is_none());
    assert!(parsed.query().is_none() && parsed.fragment().is_none());
    assert!(![8790, 5547].contains(&parsed.port().unwrap()));
    assert!(output.is_absolute() && !output.exists());
    std::fs::create_dir(&output).unwrap();
    let capture = Arc::new(Capture {
        file: Mutex::new(
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output.join("wire.jsonl"))
                .unwrap(),
        ),
        frames: Mutex::new(vec![]),
        bytes: AtomicU64::new(0),
    });
    let a_proxy = proxy("a", &endpoint, capture.clone()).await;
    let b_proxy = proxy("b", &endpoint, capture.clone()).await;
    let av = Arc::new(Vault::default());
    let bv = Arc::new(Vault::default());
    let ap = output.join("a.sqlite");
    let bp = output.join("b.sqlite");
    let a = Client::open(ap.to_str().unwrap().into(), "en-US".into()).unwrap();
    let b = Client::open(bp.to_str().unwrap().into(), "en-US".into()).unwrap();
    let suffix = types::new_id("receipt").replace('_', "").to_lowercase();
    let aa = a
        .create_account(
            "Receipt A".into(),
            format!("ra{}", &suffix[suffix.len() - 12..]),
            a_proxy.url.clone(),
            av.clone(),
        )
        .unwrap();
    let ba = b
        .create_account(
            "Receipt B".into(),
            format!("rb{}", &suffix[suffix.len() - 12..]),
            b_proxy.url.clone(),
            bv.clone(),
        )
        .unwrap();
    let listener = Arc::new(Listener(capture.clone()));
    a.start_sync(Some(listener.clone())).unwrap();
    b.start_sync(Some(listener.clone())).unwrap();
    a.wait_until_settled(30_000).await.unwrap();
    b.wait_until_settled(30_000).await.unwrap();
    let people = a.find_people(ba.handle.clone()).await.unwrap();
    assert!(people.iter().any(|p| p.id == ba.identity_id));
    let space = a.start_direct(ba.identity_id.clone()).unwrap();
    a.wait_until_settled(30_000).await.unwrap();
    let message = a
        .send_message(space.clone(), "actual encrypted receipt journey".into())
        .unwrap();
    until(||b.timeline(space.clone()).is_ok_and(|rows|rows.iter().any(|entry|entry.id==message.id&&entry.author.id==aa.identity_id&&matches!(&entry.kind,EntryKind::Message{text,..}if text=="actual encrypted receipt journey")))).await;
    let received = b
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == message.id)
        .unwrap();
    assert!(received.seq > 0);
    until(|| capture.outgoing("b", &space, received.seq, false)).await;
    until(|| {
        matches!(
            delivery(&a, &space, &message.id),
            Some(Delivery::Delivered | Delivery::Read)
        )
    })
    .await;
    b.mark_read(space.clone()).unwrap();
    until(|| capture.outgoing("b", &space, received.seq, true)).await;
    until(|| delivery(&a, &space, &message.id) == Some(Delivery::Read)).await;
    // Exercise actual public attachment encryption, signed object upload,
    // received card metadata and explicit bounded download on the second Client.
    // This is synthetic QA content, not a fabricated media event or server frame.
    let payload: Vec<u8> = (0..(1024 * 1024 + 777u32))
        .map(|index| {
            ((index.wrapping_mul(1_664_525).wrapping_add(1_013_904_223) >> 16) & 255) as u8
        })
        .collect();
    let attachment = a
        .send_file_message(
            space.clone(),
            "owned-proof.bin".into(),
            "application/octet-stream".into(),
            payload.clone(),
            None,
            Some(message.id.clone()),
            false,
            types::new_id("media-proof"),
        )
        .unwrap();
    let EntryKind::Message {
        card: Some(selected),
        ..
    } = &attachment.kind
    else {
        panic!("Actual file send must return its real file card");
    };
    let selected = selected.clone();
    assert_eq!(selected.kind_id, "file");
    a.wait_until_settled(30_000).await.unwrap();
    until(|| {
        b.timeline(space.clone()).is_ok_and(|rows| {
            rows.iter().any(|entry| {
                entry.id == attachment.id
                    && entry.author.id == aa.identity_id
                    && matches!(&entry.kind, EntryKind::Message { card: Some(card), .. }
            if card.item_id == selected.item_id && card.version == selected.version)
            })
        })
    })
    .await;
    let restored = b
        .ensure_file_version(
            selected.item_id.clone(),
            selected.version,
            payload.len() as u64,
            30_000,
        )
        .await
        .unwrap();
    assert!(restored.ready && restored.bytes == payload.len() as u64);
    assert_eq!(
        b.file_bytes_bounded(
            selected.item_id.clone(),
            Some(selected.version),
            payload.len() as u64
        )
        .unwrap(),
        Some(payload.clone())
    );

    // Observe a genuine signed MLS recovery reference, not a fabricated hash.
    let recoveries: Vec<String> = capture
        .frames
        .lock()
        .unwrap()
        .iter()
        .filter_map(|row| {
            if row["client"] != "a" || row["direction"] != "client" {
                return None;
            }
            let raw = STANDARD.decode(row["bytes_base64"].as_str()?).ok()?;
            let ClientFrame::Publish { env } = ClientFrame::decode(&raw).ok()? else {
                return None;
            };
            event_log::recovery::RecoveryRef::parse(env.recovery()?).map(|reference| reference.blob)
        })
        .collect();
    assert!(
        !recoveries.is_empty(),
        "Actual MLS Commit must retain its recovery reference"
    );
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    let http_base = a_proxy.url.replacen("ws://", "http://", 1);
    for blob in &recoveries {
        assert!(
            capture
                .frames
                .lock()
                .unwrap()
                .iter()
                .any(|row| row["client"] == "a"
                    && row["http_request"] == format!("PUT /v1/blobs/{blob} HTTP/1.1")),
            "Actual recovery reference must have an observed real signed PUT"
        );
        let mut response = http
            .get(format!("{http_base}/v1/blobs/{blob}"))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        let mut size = 0usize;
        while let Some(chunk) = response.chunk().await.unwrap() {
            size = size.checked_add(chunk.len()).unwrap();
            assert!(size <= event_log::recovery::MAX_RECOVERY_BLOB_BYTES);
            digest.update(&chunk);
        }
        assert!(size > 0);
        assert_eq!(hex::encode(digest.finalize()), *blob);
    }
    // Responses are fully consumed/dropped per iteration. Release this helper's
    // idle HTTP pool too; active/generation below deliberately count only WS.
    drop(http);

    // Actual local client restart BEFORE network attach proves persisted peer marks.
    a.stop_sync();
    until(|| a_proxy.active.load(Ordering::SeqCst) == 0).await;
    drop(a);
    let a = open(&ap, av);
    assert_eq!(delivery(&a, &space, &message.id), Some(Delivery::Read));
    a.start_sync(Some(listener)).unwrap();
    a.wait_until_settled(30_000).await.unwrap();
    let generation = a_proxy.generation.load(Ordering::SeqCst);
    assert!(
        capture.replay_before_sync_done("a", generation, &space, &ba.identity_id, received.seq),
        "Actual persisted Space D/R pair must precede reconnect SyncDone"
    );
    a.stop_sync();
    b.stop_sync();
    until(|| {
        a_proxy.active.load(Ordering::SeqCst) == 0 && b_proxy.active.load(Ordering::SeqCst) == 0
    })
    .await;
    a_proxy.stop.send(true).unwrap();
    b_proxy.stop.send(true).unwrap();
    a_proxy.task.await.unwrap();
    b_proxy.task.await.unwrap();
    std::fs::write(output.join("result.json"),serde_json::to_vec_pretty(&json!({"scope":"actual public native Client only; no cross-peer normalization or server-restart/scale claim","space":space,"message_id":message.id,"seq":received.seq.to_string(),"automatic_delivered":true,"public_mark_read":true,"client_restart_persistence":true,"reconnect_receipt_pair_before_sync_done":true,"public_encrypted_attachment_roundtrip":true,"media_bytes":payload.len(),"actual_recovery_blob_uploaded_and_content_address_verified":true,"recovery_blob_count":recoveries.len(),"actual_pass":true})).unwrap()).unwrap();
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // These use real loopback TCP, but prove only the transparent QA transport,
    // not relay authentication, actual recovery upload, or receipt delivery.
    #[tokio::test]
    async fn fragmented_http_classification_preserves_header_and_body() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let request =
            b"PUT /v1/blobs/opaque HTTP/1.1\r\nHost: owned\r\nContent-Length: 4\r\n\r\n\x00\xffab";
        let expected = request.to_vec();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let kind = request_kind(&stream).await.unwrap();
            assert_eq!(kind, ("PUT /v1/blobs/opaque HTTP/1.1".into(), false));
            let mut actual = vec![0; expected.len()];
            stream.read_exact(&mut actual).await.unwrap();
            assert_eq!(actual, expected);
        });
        let mut client = TcpStream::connect(address).await.unwrap();
        client.write_all(&request[..22]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(3)).await;
        client.write_all(&request[22..]).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn genuine_websocket_header_is_detected_case_insensitively() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            request_kind(&stream).await.unwrap()
        });
        let mut client = TcpStream::connect(address).await.unwrap();
        client
            .write_all(
                b"GET /v1/sync HTTP/1.1\r\nuPgRaDe: WebSocket\r\nConnection: Upgrade\r\n\r\n",
            )
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap(),
            ("GET /v1/sync HTTP/1.1".into(), true)
        );
    }

    #[tokio::test]
    async fn http_request_and_refusal_response_are_byte_exact() {
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_address = upstream.local_addr().unwrap();
        let request = b"PUT /v1/blobs/opaque HTTP/1.1\r\nHost: untouched\r\nContent-Length: 4\r\n\r\n\x00\xffab";
        let response =
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 3\r\nConnection: close\r\n\r\nno!";
        let server = tokio::spawn(async move {
            let (mut stream, _) = upstream.accept().await.unwrap();
            let mut actual = vec![0; request.len()];
            stream.read_exact(&mut actual).await.unwrap();
            assert_eq!(actual, request);
            stream.write_all(response).await.unwrap();
        });
        let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_address = proxy.local_addr().unwrap();
        let forwarded = tokio::spawn(async move {
            let (stream, _) = proxy.accept().await.unwrap();
            assert!(!request_kind(&stream).await.unwrap().1);
            tunnel_http(stream, upstream_address).await.unwrap()
        });
        let mut client = TcpStream::connect(proxy_address).await.unwrap();
        client.write_all(request).await.unwrap();
        client.shutdown().await.unwrap();
        let mut actual = vec![];
        tokio::time::timeout(Duration::from_secs(2), client.read_to_end(&mut actual))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(actual, response);
        let (sent, received) = forwarded.await.unwrap();
        assert_eq!(
            (sent, received),
            (request.len() as u64, response.len() as u64)
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn public_client_composes_one_sync_path_from_proxy_base() {
        let directory = std::env::temp_dir().join(types::new_id("receipt-proxy-test"));
        std::fs::create_dir(&directory).unwrap();
        let capture = Arc::new(Capture {
            file: Mutex::new(
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(directory.join("transport.jsonl"))
                    .unwrap(),
            ),
            frames: Mutex::new(vec![]),
            bytes: AtomicU64::new(0),
        });
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("ws://{}/v1/sync", upstream.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = upstream.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let Message::Binary(bytes) = socket.next().await.unwrap().unwrap() else {
                panic!("Actual public Client must send its real Hello");
            };
            assert!(matches!(
                ClientFrame::decode(&bytes),
                Ok(ClientFrame::Hello { .. })
            ));
            // No Challenge/Auth/receipt is fabricated: this unit stops at Hello.
        });
        let proxy = proxy("route-test", &endpoint, capture).await;
        let client = Client::open(
            directory.join("client.sqlite").to_str().unwrap().into(),
            "en-US".into(),
        )
        .unwrap();
        client
            .create_account(
                "Route test".into(),
                "receiptproxytest".into(),
                proxy.url.clone(),
                Arc::new(Vault::default()),
            )
            .unwrap();
        client.start_sync(None).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(5), server).await;
        client.stop_sync();
        proxy.stop.send(true).unwrap();
        proxy.task.await.unwrap();
        drop(client);
        result.unwrap().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
}

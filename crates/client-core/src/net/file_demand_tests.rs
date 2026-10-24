//! Real worker + HTTP ciphertext download, using signed histories and public file APIs.
use super::*;
use crate::{Client, CoreError, PrivacyDto, SecretVault};
use protocol::{Envelope, Sequenced};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

// Actual HTTP journeys must not race a sub-second deadline under the full
// parallel suite. Deadline/cancellation behavior has separate tight tests.
const HTTP_JOURNEY_TIMEOUT_MS: u64 = 5_000;

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

struct Fixture {
    sender: Arc<Client>,
    reader: Arc<Client>,
    space: String,
    item: String,
    old: Vec<u8>,
    old_blob: String,
    unrelated_old_blob: String,
    relay: event_log::SpaceLog,
    sequenced: HashSet<String>,
}

fn account(name: &str) -> Arc<Client> {
    let engine = Client::open(":memory:".into(), "en".into()).unwrap();
    engine
        .create_account(
            name.into(),
            name.into(),
            "http://127.0.0.1:1".into(),
            Arc::new(Vault::default()),
        )
        .unwrap();
    engine
}

impl Fixture {
    fn new() -> Self {
        let sender = account("history_author");
        let reader = account("history_reader");
        let space = sender
            .create_group_with(
                "History download QA".into(),
                vec![reader.account().unwrap().identity_id],
                PrivacyDto::Closed,
            )
            .unwrap();
        let old = b"%PDF-1.4 selected historical bytes 53 161".to_vec();
        let file = sender
            .file_add(
                space.clone(),
                "qa.pdf".into(),
                "qa.pdf".into(),
                "application/pdf".into(),
                old.clone(),
                None,
            )
            .unwrap();
        let old_blob = {
            let engine = sender.lock();
            let types::ItemContent::File(doc) = &engine.state.items[&file.id].versions[0].content
            else {
                panic!("file")
            };
            doc.chunks[0].blob.clone()
        };
        sender
            .file_new_version(
                file.id.clone(),
                b"%PDF-1.4 current bytes".to_vec(),
                None,
                "Newer canonical version".into(),
            )
            .unwrap();
        let other = sender
            .file_add(
                space.clone(),
                "other.pdf".into(),
                "other.pdf".into(),
                "application/pdf".into(),
                b"unrelated old bytes".to_vec(),
                None,
            )
            .unwrap();
        let unrelated_old_blob = {
            let engine = sender.lock();
            let types::ItemContent::File(doc) = &engine.state.items[&other.id].versions[0].content
            else {
                panic!("file")
            };
            doc.chunks[0].blob.clone()
        };
        sender
            .file_new_version(
                other.id,
                b"other current bytes".to_vec(),
                None,
                "Other newer version".into(),
            )
            .unwrap();
        let mut fixture = Self {
            sender,
            reader,
            space: space.clone(),
            item: file.id,
            old,
            old_blob,
            unrelated_old_blob,
            relay: event_log::SpaceLog::new(space),
            sequenced: HashSet::new(),
        };
        fixture.catch_up();
        fixture
    }

    fn catch_up(&mut self) {
        let pending = self.sender.lock().store.outbox().unwrap();
        for envelope in pending.into_iter().filter(|p| p.event.space == self.space) {
            if self.sequenced.insert(envelope.event.client_id.clone()) {
                let event = self.relay.sequence(envelope.event);
                assert!(matches!(
                    self.reader.lock().ingest(Sequenced {
                        seq: event.seq,
                        prev: event.prev.clone(),
                        hash: event.hash.clone(),
                        env: Envelope::plain(event)
                    }),
                    Ingest::Applied
                ));
            }
        }
    }

    fn uploads(&self) -> HashMap<String, Vec<u8>> {
        self.sender
            .lock()
            .pending_uploads(100)
            .into_iter()
            .collect()
    }
}

struct BlobServer {
    base: String,
    requests: Arc<Mutex<Vec<String>>>,
    served: Arc<Mutex<Vec<String>>>,
    blobs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    allow_old: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl BlobServer {
    fn start(blobs: HashMap<String, Vec<u8>>, held: String, corrupt: bool) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let served = Arc::new(Mutex::new(Vec::new()));
        let blobs = Arc::new(Mutex::new(blobs));
        let allow_old = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let (seen, allowed, stop) = (requests.clone(), allow_old.clone(), stopped.clone());
        let (bodies, completed) = (blobs.clone(), served.clone());
        let thread = std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                let (mut socket, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("fixture HTTP accept: {error}"),
                };
                // Accepted sockets can inherit the listener's nonblocking mode.
                // A read deadline must wait for delayed headers, not treat WouldBlock as EOF.
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut headers = Vec::new();
                let mut byte = [0];
                while !headers.ends_with(b"\r\n\r\n") && headers.len() < 8192 {
                    if socket.read(&mut byte).unwrap_or(0) == 0 {
                        break;
                    }
                    headers.push(byte[0]);
                }
                // A canceled request may close its accepted socket before it
                // writes a complete HTTP header. It is not a blob request.
                if !headers.ends_with(b"\r\n\r\n") {
                    continue;
                }
                let header = String::from_utf8(headers).unwrap();
                let path = header
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap();
                let blob = path
                    .strip_prefix("/v1/blobs/")
                    .expect("ordinary blob GET")
                    .to_owned();
                seen.lock().unwrap().push(blob.clone());
                while blob == held
                    && !allowed.load(Ordering::SeqCst)
                    && !stop.load(Ordering::SeqCst)
                {
                    std::thread::sleep(Duration::from_millis(2));
                }
                let mut body = bodies
                    .lock()
                    .unwrap()
                    .get(&blob)
                    .expect("only signed fixture ciphertext")
                    .clone();
                if blob == held && corrupt {
                    body[0] ^= 1;
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                if socket.write_all(response.as_bytes()).is_ok() && socket.write_all(&body).is_ok()
                {
                    completed.lock().unwrap().push(blob);
                }
            }
        });
        Self {
            base,
            requests,
            served,
            blobs,
            allow_old,
            stopped,
            thread: Some(thread),
        }
    }

    fn stop(&mut self) -> std::thread::Result<()> {
        self.stopped.store(true, Ordering::SeqCst);
        self.allow_old.store(true, Ordering::SeqCst);
        self.thread.take().map_or(Ok(()), |thread| thread.join())
    }
}
impl Drop for BlobServer {
    fn drop(&mut self) {
        // Normal journeys explicitly check stop(). During another assertion's
        // unwind, cleanup must preserve that failure instead of aborting twice.
        let _ = self.stop();
    }
}

#[test]
fn historical_file_fixture_keeps_accepted_socket_until_delayed_headers() {
    let body = b"sealed delayed-header fixture bytes".to_vec();
    let mut server = BlobServer::start(
        HashMap::from([("selected".into(), body.clone())]),
        String::new(),
        false,
    );
    let mut socket =
        std::net::TcpStream::connect(server.base.strip_prefix("http://").unwrap()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let mut peek = [0];
    assert!(
        matches!(socket.peek(&mut peek), Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)),
        "accepted HTTP connection must wait for headers rather than close"
    );
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket
        .write_all(b"GET /v1/blobs/selected HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(response.ends_with(&body));
    assert_eq!(server.requests.lock().unwrap().as_slice(), ["selected"]);
    server.stop().expect("delayed header fixture finishes");
}

#[test]
fn historical_file_fixture_survives_canceled_tcp_before_a_valid_blob_request() {
    let body = b"sealed fixture bytes".to_vec();
    let mut server = BlobServer::start(
        HashMap::from([("selected".into(), body.clone())]),
        "selected".into(),
        false,
    );
    server.allow_old.store(true, Ordering::SeqCst);
    let address = server.base.strip_prefix("http://").unwrap();
    for prefix in [b"".as_slice(), b"GET /v1/blobs/selected HTTP/1.1\r\n"] {
        let mut canceled = std::net::TcpStream::connect(address).unwrap();
        canceled
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        canceled.write_all(prefix).unwrap();
        canceled.shutdown(std::net::Shutdown::Write).unwrap();
        let mut response = Vec::new();
        canceled.read_to_end(&mut response).unwrap();
        assert!(response.is_empty(), "incomplete requests receive no blob");
    }
    assert!(server.requests.lock().unwrap().is_empty());
    let mut socket = std::net::TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket
        .write_all(b"GET /v1/blobs/selected HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
    let body_start = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap()
        + 4;
    assert_eq!(&response[body_start..], body);
    assert_eq!(*server.requests.lock().unwrap(), ["selected"]);
    server
        .stop()
        .expect("fixture HTTP worker must finish cleanly");
}

async fn until(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("fixture condition before deadline");
}

fn worker(
    reader: &Arc<Client>,
    server: &BlobServer,
    kick: Arc<tokio::sync::Notify>,
) -> tokio::task::JoinHandle<()> {
    let (_, key, _, _, _, _) = reader.lock().net_credentials().unwrap();
    tokio::spawn(media_worker(
        EngineLease::new(reader.inner.clone()),
        None,
        server.base.clone(),
        "history-fixture".into(),
        key,
        kick,
    ))
}

fn outbox(engine: &Client) -> Vec<(String, String)> {
    let mut entries = engine
        .lock()
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .map(|pending| {
            (
                pending.event.client_id.clone(),
                event_log::content_hash_of(&pending.event),
            )
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn ensure(
    reader: &Arc<Client>,
    item: &str,
    timeout: u64,
) -> tokio::task::JoinHandle<Result<crate::FileDto, CoreError>> {
    let (reader, item) = (reader.clone(), item.to_owned());
    tokio::spawn(async move { reader.ensure_file_version(item, 1, 1024, timeout).await })
}

#[path = "file_demand_tests/lifecycle.rs"]
mod lifecycle;
#[path = "file_demand_tests/selection.rs"]
mod selection;
#[path = "file_demand_tests/revocation.rs"]
mod revocation;

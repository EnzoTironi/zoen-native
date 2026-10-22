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

#[tokio::test]
async fn historical_file_demand_downloads_only_selected_version_and_coalesces_waiters() {
    let f = Fixture::new();
    let mut server = BlobServer::start(f.uploads(), f.old_blob.clone(), false);
    let kick = Arc::new(tokio::sync::Notify::new());
    let task = worker(&f.reader, &server, kick.clone());
    until(|| {
        f.reader
            .file_info_at(f.item.clone(), Some(2))
            .unwrap()
            .ready
    })
    .await;
    assert_eq!(
        f.reader
            .file_bytes_bounded(f.item.clone(), Some(1), 1024)
            .unwrap(),
        None
    );
    assert_eq!(f.reader.item(f.item.clone()).unwrap().version, 2);
    let before = (
        f.reader.item(f.item.clone()).unwrap(),
        f.reader.timeline(f.space.clone()).unwrap(),
        outbox(&f.reader),
    );
    let first = ensure(&f.reader, &f.item, HTTP_JOURNEY_TIMEOUT_MS);
    let second = ensure(&f.reader, &f.item, HTTP_JOURNEY_TIMEOUT_MS);
    until(|| f.reader.lock().net.file_demands.waiter_count() == 2).await;
    assert_eq!(f.reader.lock().net.file_demands.active().len(), 1);
    kick.notify_one();
    until(|| server.requests.lock().unwrap().contains(&f.old_blob)).await;
    first.abort();
    let _ = first.await;
    assert_eq!(
        f.reader.lock().net.file_demands.active().len(),
        1,
        "one canceled waiter cannot cancel another"
    );
    server.allow_old.store(true, Ordering::SeqCst);
    let ready = second.await.unwrap().unwrap();
    assert!(ready.ready);
    assert_eq!(ready.sha256, hex::encode(Sha256::digest(&f.old)));
    assert_eq!(
        f.reader
            .file_bytes_bounded(f.item.clone(), Some(1), 1024)
            .unwrap(),
        Some(f.old)
    );
    assert_eq!(
        (
            f.reader.item(f.item.clone()).unwrap(),
            f.reader.timeline(f.space).unwrap(),
            outbox(&f.reader)
        ),
        before
    );
    assert!(f.reader.lock().net.file_demands.active().is_empty());
    {
        let requests = server.requests.lock().unwrap();
        assert_eq!(
            requests.iter().filter(|blob| *blob == &f.old_blob).count(),
            1
        );
        assert!(
            !requests.contains(&f.unrelated_old_blob),
            "never eagerly fetch unrelated historical bytes"
        );
    }
    task.abort();
    let _ = task.await;
    server
        .stop()
        .expect("fixture HTTP worker must finish cleanly");
}

#[tokio::test]
async fn historical_file_demand_timeout_cancel_and_invalid_requests_leave_no_demand() {
    let f = Fixture::new();
    assert!(f
        .reader
        .ensure_file_version(f.item.clone(), 1, 1, 100)
        .await
        .is_err());
    assert!(f
        .reader
        .ensure_file_version(f.item.clone(), 99, 1024, 100)
        .await
        .is_err());
    assert!(f
        .reader
        .ensure_file_version(f.item.clone(), 1, u64::MAX, 100)
        .await
        .is_err());
    assert!(f
        .reader
        .ensure_file_version(f.item.clone(), 1, 1024, 0)
        .await
        .is_err());
    assert!(f
        .reader
        .ensure_file_version(f.item.clone(), 1, 1024, 60_001)
        .await
        .is_err());
    assert!(f.reader.lock().net.file_demands.active().is_empty());
    assert!(f
        .reader
        .ensure_file_version(f.item.clone(), 1, 1024, 60)
        .await
        .is_err());
    assert!(
        f.reader.lock().net.file_demands.active().is_empty(),
        "expired demand removed"
    );
    let waiting = ensure(&f.reader, &f.item, 2000);
    until(|| !f.reader.lock().net.file_demands.active().is_empty()).await;
    waiting.abort();
    let _ = waiting.await;
    assert!(
        f.reader.lock().net.file_demands.active().is_empty(),
        "canceled future releases lease"
    );
}

/// Original C17 adaptation: immutable item/version IDs replace Signal's positional
/// backfill pointers. A bad historical item must not poison its successful neighbor.
#[tokio::test]
async fn mixed_historical_downloads_preserve_item_routing_and_retry_after_corruption() {
    for corrupt_first in [true, false] {
        let f = Fixture::new();
        let other = f
            .reader
            .items()
            .into_iter()
            .find(|item| item.id != f.item)
            .expect("second signed file");
        let ids = [f.item.clone(), other.id];
        let bodies = [f.old.clone(), b"unrelated old bytes".to_vec()];
        let bad = usize::from(!corrupt_first);
        let good = 1 - bad;
        let bad_blob = if bad == 0 {
            f.old_blob.clone()
        } else {
            f.unrelated_old_blob.clone()
        };
        let mut corrupt = f.uploads();
        corrupt.get_mut(&bad_blob).unwrap()[0] ^= 1;
        let mut server = BlobServer::start(corrupt, String::new(), false);
        let kick = Arc::new(tokio::sync::Notify::new());
        let task = worker(&f.reader, &server, kick.clone());
        until(|| {
            ids.iter()
                .all(|id| f.reader.file_info_at(id.clone(), Some(2)).unwrap().ready)
        })
        .await;
        for id in &ids {
            assert!(!f.reader.file_info_at(id.clone(), Some(1)).unwrap().ready);
        }
        let before = (
            ids.iter()
                .map(|id| f.reader.item(id.clone()).unwrap())
                .collect::<Vec<_>>(),
            f.reader.timeline(f.space.clone()).unwrap(),
            outbox(&f.reader),
        );
        let first = ensure(&f.reader, &ids[0], HTTP_JOURNEY_TIMEOUT_MS);
        let second = ensure(&f.reader, &ids[1], HTTP_JOURNEY_TIMEOUT_MS);
        until(|| f.reader.lock().net.file_demands.waiter_count() == 2).await;
        let admitted_blobs: Vec<_> = f
            .reader
            .lock()
            .wanted_media()
            .into_iter()
            .map(|w| w.blob)
            .collect();
        assert!(admitted_blobs.contains(&f.old_blob), "old blob absent before wake; corrupt_first={corrupt_first}; admitted={admitted_blobs:?}");
        assert!(admitted_blobs.contains(&f.unrelated_old_blob), "neighbor blob absent before wake; corrupt_first={corrupt_first}; admitted={admitted_blobs:?}");
        kick.notify_one();
        let results = [first.await.unwrap(), second.await.unwrap()];
        let ready = results[good].as_ref().expect("unrelated version succeeds");
        assert!(ready.ready);
        assert_eq!(ready.sha256, hex::encode(Sha256::digest(&bodies[good])));
        assert!(matches!(results[bad], Err(CoreError::Invalid { .. })));
        assert_eq!(
            f.reader
                .file_bytes_bounded(ids[good].clone(), Some(1), 1024)
                .unwrap(),
            Some(bodies[good].clone())
        );
        assert_eq!(
            f.reader
                .file_bytes_bounded(ids[bad].clone(), Some(1), 1024)
                .unwrap(),
            None
        );
        assert!(f.reader.lock().net.file_demands.active().is_empty());
        let requested = server.requests.lock().unwrap().clone();
        assert!(requested.contains(&f.old_blob), "old blob not requested; corrupt_first={corrupt_first}; admitted={admitted_blobs:?}; requests={requested:?}; served={:?}", server.served.lock().unwrap());
        assert!(requested.contains(&f.unrelated_old_blob), "neighbor blob not requested; corrupt_first={corrupt_first}; admitted={admitted_blobs:?}; requests={requested:?}; served={:?}", server.served.lock().unwrap());
        task.abort();
        let _ = task.await;
        server.stop().expect("corrupt HTTP worker finishes");

        // Restore only the transport bytes, retaining the same account, signed
        // history, Item IDs and successful cache. A failure must stay retryable.
        let mut healthy = BlobServer::start(f.uploads(), String::new(), false);
        let retry = ensure(&f.reader, &ids[bad], HTTP_JOURNEY_TIMEOUT_MS);
        until(|| !f.reader.lock().net.file_demands.active().is_empty()).await;
        let task = worker(&f.reader, &healthy, Arc::new(tokio::sync::Notify::new()));
        assert!(retry.await.unwrap().unwrap().ready);
        assert_eq!(healthy.requests.lock().unwrap().as_slice(), [bad_blob]);
        for (id, body) in ids.iter().zip(&bodies) {
            assert_eq!(
                f.reader
                    .file_bytes_bounded(id.clone(), Some(1), 1024)
                    .unwrap(),
                Some(body.clone())
            );
        }
        assert_eq!(
            (
                ids.iter()
                    .map(|id| f.reader.item(id.clone()).unwrap())
                    .collect::<Vec<_>>(),
                f.reader.timeline(f.space.clone()).unwrap(),
                outbox(&f.reader),
            ),
            before,
            "downloads cannot change canonical items, timeline or outgoing intent"
        );
        assert!(f.reader.verify_log(f.space).valid);
        assert!(f.reader.lock().net.file_demands.active().is_empty());
        task.abort();
        let _ = task.await;
        healthy.stop().expect("healthy HTTP worker finishes");
    }
}

#[tokio::test]
async fn new_foreground_demand_retries_same_worker_and_coalesces_before_backoff() {
    let f = Fixture::new();
    let mut corrupt = f.uploads();
    corrupt.get_mut(&f.old_blob).unwrap()[0] ^= 1;
    let mut server = BlobServer::start(corrupt, String::new(), false);
    let kick = Arc::new(tokio::sync::Notify::new());
    let task = worker(&f.reader, &server, kick.clone());
    until(|| {
        f.reader.items().iter().all(|item| {
            f.reader
                .file_info_at(item.id.clone(), Some(2))
                .unwrap()
                .ready
        })
    })
    .await;
    let before = (
        f.reader.item(f.item.clone()).unwrap(),
        f.reader.timeline(f.space.clone()).unwrap(),
        outbox(&f.reader),
    );
    server.requests.lock().unwrap().clear();
    server.served.lock().unwrap().clear();

    let first = ensure(&f.reader, &f.item, HTTP_JOURNEY_TIMEOUT_MS);
    until(|| f.reader.lock().net.file_demands.waiter_count() == 1).await;
    kick.notify_one();
    until(|| server.served.lock().unwrap().contains(&f.old_blob)).await;
    // End only the first foreground demand. The exact same HTTP server, worker,
    // account and signed version survive its real corrupt-ciphertext response.
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    assert!(f.reader.lock().net.file_demands.active().is_empty());
    assert!(
        !f.reader
            .file_info_at(f.item.clone(), Some(1))
            .unwrap()
            .ready
    );
    server
        .blobs
        .lock()
        .unwrap()
        .insert(f.old_blob.clone(), f.uploads()[&f.old_blob].clone());

    // This is a deadline/backoff contract, rather than a generic HTTP journey:
    // the new episode must make one fresh attempt before the old 2s backoff.
    let retry = ensure(&f.reader, &f.item, 1_000);
    let joined = ensure(&f.reader, &f.item, 1_000);
    until(|| f.reader.lock().net.file_demands.waiter_count() == 2).await;
    kick.notify_one();
    for result in [retry.await.unwrap(), joined.await.unwrap()] {
        assert!(
            result
                .expect("new foreground episode bypasses old backoff once")
                .ready
        );
    }
    assert_eq!(
        server.requests.lock().unwrap().as_slice(),
        [f.old_blob.clone(), f.old_blob.clone()]
    );
    assert_eq!(
        f.reader
            .file_bytes_bounded(f.item.clone(), Some(1), 1024)
            .unwrap(),
        Some(f.old.clone())
    );
    assert_eq!(
        (
            f.reader.item(f.item.clone()).unwrap(),
            f.reader.timeline(f.space.clone()).unwrap(),
            outbox(&f.reader),
        ),
        before,
        "foreground retry cannot change canonical content or outgoing intent"
    );
    assert!(f.reader.verify_log(f.space).valid);
    assert!(f.reader.lock().net.file_demands.active().is_empty());
    task.abort();
    let _ = task.await;
    server.stop().expect("same HTTP worker finishes");
}

#[test]
fn foreground_attempt_does_not_reset_background_deadline_or_failure_count() {
    let now = tokio::time::Instant::now();
    let mut retries = MediaDownloadRetries::default();
    retries.failed("selected", now);
    retries.failed("neighbor", now);
    let first_retry = now + Duration::from_secs(1);
    assert!(!retries.ready("selected", false, first_retry));
    assert!(retries.ready("selected", true, first_retry));
    assert!(!retries.ready("selected", false, first_retry));
    assert!(!retries.ready("neighbor", false, first_retry));
    retries.failed("selected", first_retry);
    assert!(!retries.ready("selected", false, first_retry + Duration::from_secs(3)));
    assert!(retries.ready("selected", false, first_retry + Duration::from_secs(4)));
    retries.completed("selected");
    assert!(retries.ready("selected", false, first_retry));
    assert!(!retries.ready("neighbor", false, first_retry));
}

#[test]
fn foreground_attempt_revalidates_scope_and_retains_unsynced_demand() {
    let mut f = Fixture::new();
    {
        let e = f.reader.lock();
        assert!(!e.wanted_media().is_empty());
        for _ in 0..3 {
            assert!(
                e.take_foreground_file_attempts().is_empty(),
                "background snapshots cannot create an attempt"
            );
        }
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let key = f.reader.lock().file_demand_key(&f.item, 1, 1024).unwrap();
    let lease = f
        .reader
        .lock()
        .net
        .file_demands
        .acquire(key.clone(), 1024, deadline)
        .unwrap();
    {
        let mut e = f.reader.lock();
        e.net.synced.remove(&f.space);
        assert!(e.take_foreground_file_attempts().is_empty());
        assert_eq!(
            e.net.file_demands.foreground_attempts(),
            [(key.clone(), 1024)]
        );
        e.net.synced.insert(f.space.clone());
        let attempts = e.take_foreground_file_attempts();
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts.iter().next().unwrap().blob, f.old_blob);
        assert!(e.take_foreground_file_attempts().is_empty());
    }
    drop(lease);
    // These are stale local demand keys, not altered canonical events.
    for field in ["identity", "device", "version", "sha256"] {
        let mut stale = key.clone();
        match field {
            "identity" => stale.identity = "foreign".into(),
            "device" => stale.device = "foreign".into(),
            "version" => stale.version = 2,
            "sha256" => stale.sha256 = "00".repeat(32),
            _ => unreachable!(),
        }
        let e = f.reader.lock();
        let stale_lease = e.net.file_demands.acquire(stale, 1024, deadline).unwrap();
        assert!(
            e.take_foreground_file_attempts().is_empty(),
            "stale {field}"
        );
        assert!(!stale_lease.active());
    }
    let lease = f
        .reader
        .lock()
        .net
        .file_demands
        .acquire(key, 1024, deadline)
        .unwrap();
    f.sender
        .remove_member(f.space.clone(), f.reader.account().unwrap().identity_id)
        .unwrap();
    f.catch_up();
    assert!(f.reader.lock().take_foreground_file_attempts().is_empty());
    assert!(!lease.active());
    assert!(f.reader.verify_log(f.space).valid);
}

#[tokio::test]
async fn historical_file_demand_rechecks_membership_before_accepting_inflight_bytes() {
    let mut f = Fixture::new();
    let mut server = BlobServer::start(f.uploads(), f.old_blob.clone(), false);
    let waiting = ensure(&f.reader, &f.item, HTTP_JOURNEY_TIMEOUT_MS);
    until(|| !f.reader.lock().net.file_demands.active().is_empty()).await;
    let task = worker(&f.reader, &server, Arc::new(tokio::sync::Notify::new()));
    until(|| server.requests.lock().unwrap().contains(&f.old_blob)).await;
    f.sender
        .remove_member(f.space.clone(), f.reader.account().unwrap().identity_id)
        .unwrap();
    f.catch_up();
    server.allow_old.store(true, Ordering::SeqCst);
    assert!(matches!(
        waiting.await.unwrap(),
        Err(CoreError::Forbidden { .. })
    ));
    assert!(f.reader.lock().net.file_demands.active().is_empty());
    assert_eq!(
        f.reader
            .file_bytes_bounded(f.item.clone(), Some(1), 1024)
            .unwrap(),
        None
    );
    assert!(f
        .reader
        .ensure_file_version(f.item, 1, 1024, 100)
        .await
        .is_err());
    task.abort();
    let _ = task.await;
    server
        .stop()
        .expect("fixture HTTP worker must finish cleanly");
}

#[tokio::test]
async fn historical_file_demand_rejects_corrupt_ciphertext_and_clears_on_deadline() {
    let f = Fixture::new();
    let mut server = BlobServer::start(f.uploads(), f.old_blob.clone(), true);
    server.allow_old.store(true, Ordering::SeqCst);
    let waiting = ensure(&f.reader, &f.item, HTTP_JOURNEY_TIMEOUT_MS);
    until(|| !f.reader.lock().net.file_demands.active().is_empty()).await;
    let task = worker(&f.reader, &server, Arc::new(tokio::sync::Notify::new()));
    until(|| server.requests.lock().unwrap().contains(&f.old_blob)).await;
    assert!(waiting.await.unwrap().is_err());
    assert!(server.requests.lock().unwrap().contains(&f.old_blob));
    assert_eq!(
        f.reader.file_bytes_bounded(f.item, Some(1), 1024).unwrap(),
        None
    );
    assert!(f.reader.lock().net.file_demands.active().is_empty());
    task.abort();
    let _ = task.await;
    server
        .stop()
        .expect("fixture HTTP worker must finish cleanly");
}

#[tokio::test]
async fn historical_file_demand_sync_stop_cancels_waiters_and_account_change_is_rejected() {
    let f = Fixture::new();
    f.reader.start_sync(None).unwrap();
    let waiting = ensure(&f.reader, &f.item, 2000);
    until(|| !f.reader.lock().net.file_demands.active().is_empty()).await;
    f.reader.stop_sync();
    assert!(waiting.await.unwrap().is_err());
    assert!(f.reader.lock().net.file_demands.active().is_empty());
    let waiting = ensure(&f.reader, &f.item, 2000);
    until(|| !f.reader.lock().net.file_demands.active().is_empty()).await;
    f.reader.lock().net.author = None; // A real locked-account state, not download completion.
    assert!(matches!(
        waiting.await.unwrap(),
        Err(CoreError::Forbidden { .. })
    ));
    assert!(f.reader.lock().net.file_demands.active().is_empty());
}

#[tokio::test]
async fn historical_file_demand_stop_without_a_connection_cancels_pending_open() {
    let f = Fixture::new();
    assert!(f.reader.net.lock().unwrap().is_none());
    let waiting = ensure(&f.reader, &f.item, 2000);
    until(|| !f.reader.lock().net.file_demands.active().is_empty()).await;
    f.reader.stop_sync();
    assert!(tokio::time::timeout(Duration::from_millis(250), waiting)
        .await
        .expect("stop must cancel promptly, not merely reach the original two-second deadline")
        .unwrap()
        .is_err());
    assert!(f.reader.lock().net.file_demands.active().is_empty());
    assert_eq!(
        f.reader.file_bytes_bounded(f.item, Some(1), 1024).unwrap(),
        None
    );
}

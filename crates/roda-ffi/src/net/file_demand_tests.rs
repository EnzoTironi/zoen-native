//! Real worker + HTTP ciphertext download, using signed histories and public file APIs.
use super::*;
use crate::{CoreError, PrivacyDto, RodaEngine, SecretVault};
use roda_proto::{Envelope, Sequenced};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

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
    sender: Arc<RodaEngine>,
    reader: Arc<RodaEngine>,
    space: String,
    item: String,
    old: Vec<u8>,
    old_blob: String,
    unrelated_old_blob: String,
    relay: roda_log::SpaceLog,
    sequenced: HashSet<String>,
}

fn account(name: &str) -> Arc<RodaEngine> {
    let engine = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
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
            let roda_types::ItemContent::File(doc) =
                &engine.state.items[&file.id].versions[0].content
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
            let roda_types::ItemContent::File(doc) =
                &engine.state.items[&other.id].versions[0].content
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
            relay: roda_log::SpaceLog::new(space),
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
        let allow_old = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let (seen, allowed, stop) = (requests.clone(), allow_old.clone(), stopped.clone());
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
                let mut body = blobs
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
                let _ = socket.write_all(response.as_bytes());
                let _ = socket.write_all(&body);
            }
        });
        Self {
            base,
            requests,
            allow_old,
            stopped,
            thread: Some(thread),
        }
    }
}
impl Drop for BlobServer {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.allow_old.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}

async fn until(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("fixture condition before deadline");
}

fn worker(
    reader: &Arc<RodaEngine>,
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

fn outbox(engine: &RodaEngine) -> Vec<(String, String)> {
    let mut entries = engine
        .lock()
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .map(|pending| {
            (
                pending.event.client_id.clone(),
                roda_log::content_hash_of(&pending.event),
            )
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn ensure(
    reader: &Arc<RodaEngine>,
    item: &str,
    timeout: u64,
) -> tokio::task::JoinHandle<Result<crate::FileDto, CoreError>> {
    let (reader, item) = (reader.clone(), item.to_owned());
    tokio::spawn(async move { reader.ensure_file_version(item, 1, 1024, timeout).await })
}

#[tokio::test]
async fn historical_file_demand_downloads_only_selected_version_and_coalesces_waiters() {
    let f = Fixture::new();
    let server = BlobServer::start(f.uploads(), f.old_blob.clone(), false);
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
    let first = ensure(&f.reader, &f.item, 2000);
    let second = ensure(&f.reader, &f.item, 2000);
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

#[tokio::test]
async fn historical_file_demand_rechecks_membership_before_accepting_inflight_bytes() {
    let mut f = Fixture::new();
    let server = BlobServer::start(f.uploads(), f.old_blob.clone(), false);
    let waiting = ensure(&f.reader, &f.item, 2000);
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
}

#[tokio::test]
async fn historical_file_demand_rejects_corrupt_ciphertext_and_clears_on_deadline() {
    let f = Fixture::new();
    let server = BlobServer::start(f.uploads(), f.old_blob.clone(), true);
    server.allow_old.store(true, Ordering::SeqCst);
    let waiting = ensure(&f.reader, &f.item, 180);
    until(|| !f.reader.lock().net.file_demands.active().is_empty()).await;
    let task = worker(&f.reader, &server, Arc::new(tokio::sync::Notify::new()));
    assert!(waiting.await.unwrap().is_err());
    assert!(server.requests.lock().unwrap().contains(&f.old_blob));
    assert_eq!(
        f.reader.file_bytes_bounded(f.item, Some(1), 1024).unwrap(),
        None
    );
    assert!(f.reader.lock().net.file_demands.active().is_empty());
    task.abort();
    let _ = task.await;
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

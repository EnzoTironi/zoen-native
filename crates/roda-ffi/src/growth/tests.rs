use super::*;
use crate::SecretVault;
use roda_log::verify_sig;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};

const CANONICAL_NAME: &str = "authenticated-growth-relay";

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

struct Hold {
    path: &'static str,
    arrived: tokio::sync::oneshot::Sender<()>,
    release: mpsc::Receiver<()>,
}

struct CapturedReport {
    device: String,
    signature_valid: bool,
    report: ClientReport,
}

struct RelayState {
    refuse_next_report: AtomicBool,
    reports: Mutex<Vec<CapturedReport>>,
    hold: Mutex<Option<Hold>>,
}

/// Real HTTP and Ed25519 verification; the CLI growth journey supplies the real WebSocket login.
struct HttpRelay {
    url: String,
    state: Arc<RelayState>,
    stopped: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl HttpRelay {
    fn new(refuse_next_report: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(RelayState {
            refuse_next_report: AtomicBool::new(refuse_next_report),
            reports: Mutex::new(Vec::new()),
            hold: Mutex::new(None),
        });
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_state = state.clone();
        let worker_stopped = stopped.clone();
        let worker = thread::spawn(move || {
            for socket in listener.incoming() {
                let mut socket = socket.unwrap();
                if worker_stopped.load(Ordering::Acquire) {
                    break;
                }
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 4096];
                let end = loop {
                    let n = socket.read(&mut buffer).unwrap();
                    assert!(n > 0, "request headers truncated");
                    bytes.extend_from_slice(&buffer[..n]);
                    assert!(bytes.len() <= 1_000_000, "oversized fixture request");
                    if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let head = std::str::from_utf8(&bytes[..end]).unwrap();
                let path = head
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .to_string();
                let headers: HashMap<String, String> = head
                    .lines()
                    .skip(1)
                    .filter_map(|line| line.split_once(':'))
                    .map(|(key, value)| (key.to_lowercase(), value.trim().to_string()))
                    .collect();
                let length: usize = headers
                    .get("content-length")
                    .map_or(0, |n| n.parse().unwrap());
                assert!(length <= 1_000_000);
                while bytes.len() < end + length {
                    let n = socket.read(&mut buffer).unwrap();
                    assert!(n > 0, "request body truncated");
                    bytes.extend_from_slice(&buffer[..n]);
                }
                let (status, body) = if path == "/v1/config" {
                    let mut config = RemoteConfig::builtin();
                    config.version = 777;
                    (200, serde_json::to_vec(&config).unwrap())
                } else {
                    assert_eq!(path, "/v1/report");
                    let body = &bytes[end..end + length];
                    let sha = hex::encode(Sha256::digest(body));
                    let ts = headers["x-zoen-ts"].parse().unwrap();
                    let valid = verify_sig(
                        &headers["x-zoen-device"],
                        &report_message(&sha, ts, CANONICAL_NAME),
                        &headers["x-zoen-sig"],
                    );
                    worker_state.reports.lock().unwrap().push(CapturedReport {
                        device: headers["x-zoen-device"].clone(),
                        signature_valid: valid,
                        report: serde_json::from_slice(body).unwrap(),
                    });
                    let refused = worker_state
                        .refuse_next_report
                        .swap(false, Ordering::AcqRel);
                    (if valid && !refused { 200 } else { 401 }, b"{}".to_vec())
                };
                let hold = {
                    let mut pending = worker_state.hold.lock().unwrap();
                    if pending.as_ref().is_some_and(|hold| hold.path == path) {
                        pending.take()
                    } else {
                        None
                    }
                };
                if let Some(hold) = hold {
                    hold.arrived.send(()).unwrap();
                    hold.release
                        .recv_timeout(Duration::from_secs(5))
                        .expect("held response not released");
                }
                write!(socket, "HTTP/1.1 {status} {}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nETag: fixture-777\r\nConnection: close\r\n\r\n",
                    if status == 200 { "OK" } else { "Unauthorized" }, body.len()).unwrap();
                socket.write_all(&body).unwrap();
            }
        });
        Self {
            url,
            state,
            stopped,
            worker: Some(worker),
        }
    }

    fn hold(&self, path: &'static str) -> (tokio::sync::oneshot::Receiver<()>, mpsc::Sender<()>) {
        let (arrived, waiting) = tokio::sync::oneshot::channel();
        let (release, released) = mpsc::channel();
        *self.state.hold.lock().unwrap() = Some(Hold {
            path,
            arrived,
            release: released,
        });
        (waiting, release)
    }

    fn reports(&self) -> usize {
        self.state.reports.lock().unwrap().len()
    }
}

impl Drop for HttpRelay {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.url.trim_start_matches("http://"));
        if let Some(worker) = self.worker.take() {
            if let Err(error) = worker.join() {
                if !thread::panicking() {
                    std::panic::resume_unwind(error);
                }
            }
        }
    }
}

fn account(url: &str) -> (Arc<RodaEngine>, Arc<Vault>) {
    let engine = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    engine
        .create_account("Ana".into(), "ana".into(), url.into(), vault.clone())
        .unwrap();
    engine.lock().set_registered(true).unwrap();
    (engine, vault)
}

fn authenticate(engine: &RodaEngine) {
    let mut state = engine.lock();
    let account = state.net.account.clone().unwrap();
    state.remember_authenticated_relay(
        &account.identity,
        &account.device,
        &account.relay_url,
        CANONICAL_NAME,
    );
}

#[tokio::test]
async fn reports_defer_without_login_and_retain_pending_data_after_refusal() {
    let relay = HttpRelay::new(true);
    let (engine, _) = account(&relay.url);
    engine.growth_capture_link("https://tryzoen.com/?utm_campaign=fixture".into());
    engine.mark_exposed("fixture", "treatment");
    assert!(
        !engine
            .growth_sync(None, false, 0, 0)
            .await
            .unwrap()
            .reported
    );
    assert_eq!(
        relay.reports(),
        0,
        "a URL must never be guessed as the signing name"
    );
    authenticate(&engine);
    let error = engine.growth_sync(None, false, 0, 0).await.unwrap_err();
    assert!(error.to_string().contains("report refused (401"));
    assert!(engine.meta(META_SOURCE_SENT).is_none());
    assert_eq!(
        serde_json::from_str::<Vec<Exposure>>(&engine.meta(META_EXPOSURES).unwrap())
            .unwrap()
            .len(),
        1
    );
    assert!(
        engine
            .growth_sync(None, false, 0, 0)
            .await
            .unwrap()
            .reported
    );
    assert_eq!(engine.meta(META_SOURCE_SENT).as_deref(), Some("1"));
    assert_eq!(engine.meta(META_EXPOSURES).as_deref(), Some("[]"));
    assert!(
        !engine
            .growth_sync(None, false, 0, 0)
            .await
            .unwrap()
            .reported
    );
    assert_eq!(relay.reports(), 2);
    for captured in relay.state.reports.lock().unwrap().iter() {
        assert!(
            captured.signature_valid,
            "the canonical name must verify despite a different URL"
        );
        assert_eq!(
            captured
                .report
                .attribution
                .as_ref()
                .unwrap()
                .campaign
                .as_deref(),
            Some("fixture")
        );
        assert!(captured
            .report
            .attribution
            .as_ref()
            .unwrap()
            .target
            .is_none());
    }
}

#[tokio::test]
async fn report_binding_survives_profile_registration_but_not_relay_key_or_account_changes() {
    let relay = HttpRelay::new(false);
    let other = HttpRelay::new(false);
    let (engine, vault) = account(&relay.url);
    authenticate(&engine);
    engine
        .lock()
        .update_profile("Ana updated", "ana", "Updated profile")
        .unwrap();
    assert!(engine.lock().net.authenticated_relay.is_some());
    engine.lock().set_registered(true).unwrap();
    assert!(
        !engine
            .growth_sync(Some(other.url.clone()), false, 0, 0)
            .await
            .unwrap()
            .reported
    );
    assert_eq!(other.reports(), 0);
    assert!(engine.meta(META_SOURCE_SENT).is_none());
    assert!(
        engine
            .growth_sync(Some(format!("{}/", relay.url)), false, 0, 0)
            .await
            .unwrap()
            .reported
    );
    {
        let mut state = engine.lock();
        let mut changed = state.net.account.clone().unwrap();
        changed.relay_url = other.url.clone();
        state.save_linked_account(changed).unwrap();
        assert!(state.net.authenticated_relay.is_none());
    }
    authenticate(&engine);
    assert!(engine.unlock(vault.clone()).unwrap());
    assert!(engine.lock().net.authenticated_relay.is_none());
    authenticate(&engine);
    let old = engine.account().unwrap();
    engine.erase_device(vault.clone()).unwrap();
    engine
        .create_account("Bia".into(), "bia".into(), relay.url.clone(), vault)
        .unwrap();
    engine.lock().set_registered(true).unwrap();
    engine.lock().remember_authenticated_relay(
        &old.identity_id,
        &old.device_id,
        &relay.url,
        CANONICAL_NAME,
    );
    assert!(engine.lock().net.authenticated_relay.is_none());
    assert!(
        !engine
            .growth_sync(None, false, 0, 0)
            .await
            .unwrap()
            .reported
    );
    assert!(engine.meta(META_SOURCE_SENT).is_none());
    assert_eq!(relay.reports(), 1);
}

#[tokio::test]
async fn a_stale_config_response_cannot_overwrite_the_next_accounts_cache_or_report() {
    let relay = HttpRelay::new(false);
    let (engine, vault) = account(&relay.url);
    authenticate(&engine);
    let (arrived, release) = relay.hold("/v1/config");
    let request = tokio::spawn({
        let engine = engine.clone();
        async move { engine.growth_sync(None, false, 0, 0).await.unwrap() }
    });
    arrived.await.unwrap();
    engine.erase_device(vault.clone()).unwrap();
    engine
        .create_account("Bia".into(), "bia".into(), relay.url.clone(), vault)
        .unwrap();
    engine.set_meta(META_CONFIG, "{\"version\":888}").unwrap();
    engine.set_meta(META_ETAG, "new-account").unwrap();
    release.send(()).unwrap();
    let result = request.await.unwrap();
    assert!(!result.reported);
    assert_eq!(result.config_version, 888);
    assert_eq!(engine.meta(META_ETAG).as_deref(), Some("new-account"));
    assert_eq!(relay.reports(), 0);
}

#[tokio::test]
async fn an_accepted_report_keeps_source_and_exposures_that_arrived_in_flight() {
    let relay = HttpRelay::new(false);
    let (engine, _) = account(&relay.url);
    authenticate(&engine);
    engine.mark_exposed("before", "control");
    let (arrived, release) = relay.hold("/v1/report");
    let request = tokio::spawn({
        let engine = engine.clone();
        async move { engine.growth_sync(None, false, 0, 0).await.unwrap() }
    });
    arrived.await.unwrap();
    engine.growth_capture_link("https://tryzoen.com/?utm_campaign=late_campaign".into());
    engine.mark_exposed("during", "treatment");
    release.send(()).unwrap();
    assert!(request.await.unwrap().reported);
    assert!(
        engine.meta(META_SOURCE_SENT).is_none(),
        "late attribution must still be reportable"
    );
    let pending: Vec<Exposure> =
        serde_json::from_str(&engine.meta(META_EXPOSURES).unwrap()).unwrap();
    assert_eq!(
        pending,
        vec![Exposure {
            flag: "during".into(),
            variant: "treatment".into()
        }]
    );
    assert!(
        engine
            .growth_sync(None, false, 0, 0)
            .await
            .unwrap()
            .reported
    );
    let reports = relay.state.reports.lock().unwrap();
    assert_eq!(
        reports[0].report.attribution.as_ref().unwrap().kind,
        SourceKind::Organic
    );
    assert_eq!(
        reports[1]
            .report
            .attribution
            .as_ref()
            .unwrap()
            .campaign
            .as_deref(),
        Some("late_campaign")
    );
}

#[tokio::test]
async fn an_old_accounts_accepted_report_cannot_consume_the_new_accounts_metadata() {
    let relay = HttpRelay::new(false);
    let (engine, vault) = account(&relay.url);
    authenticate(&engine);
    let old_device = engine.account().unwrap().device_id;
    engine.growth_capture_link("https://tryzoen.com/?utm_campaign=old_campaign".into());
    let (arrived, release) = relay.hold("/v1/report");
    let request = tokio::spawn({
        let engine = engine.clone();
        async move { engine.growth_sync(None, false, 0, 0).await.unwrap() }
    });
    arrived.await.unwrap();
    engine.erase_device(vault.clone()).unwrap();
    engine
        .create_account("Bia".into(), "bia".into(), relay.url.clone(), vault)
        .unwrap();
    engine.lock().set_registered(true).unwrap();
    authenticate(&engine);
    engine.growth_capture_link("https://tryzoen.com/?utm_campaign=new_campaign".into());
    engine.mark_exposed("new-account", "control");
    release.send(()).unwrap();
    assert!(request.await.unwrap().reported);
    assert!(engine.meta(META_SOURCE_SENT).is_none());
    assert!(engine.meta(META_EXPOSURES).unwrap().contains("new-account"));
    assert!(
        engine
            .growth_sync(None, false, 0, 0)
            .await
            .unwrap()
            .reported
    );
    let reports = relay.state.reports.lock().unwrap();
    assert!(reports.iter().all(|r| r.signature_valid));
    assert_eq!(reports[0].device, old_device);
    assert_eq!(
        reports[0]
            .report
            .attribution
            .as_ref()
            .unwrap()
            .campaign
            .as_deref(),
        Some("old_campaign")
    );
    assert_eq!(reports[1].device, engine.account().unwrap().device_id);
    assert_eq!(
        reports[1]
            .report
            .attribution
            .as_ref()
            .unwrap()
            .campaign
            .as_deref(),
        Some("new_campaign")
    );
}

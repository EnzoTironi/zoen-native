//! Local peers only. These exercise the real synchronous HttpTransport/reqwest
//! path, never a provider, account, Gate policy replacement or shared service.
use super::*;

/// Explicit operator-only probe. Credentials are inherited in memory from the
/// supported Wrangler CLI; no local Zoen profile or human content is read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicit ZOEN_LIVE_CLEF_QA=1 and Workers AI credentials; one synthetic request"]
async fn live_clef_configured_gate_synthetic_once() {
    assert_eq!(std::env::var("ZOEN_LIVE_CLEF_QA").as_deref(), Ok("1"));
    assert_ne!(std::env::var("ZOEN_AGENT_GATE").as_deref(), Ok("offline"));
    let _ = rustls::crypto::ring::default_provider().install_default();
    let gate = gate().expect("explicit Workers AI Gate configuration");
    let decision = gate.check(&gate::Check::new(
        gate::Point::Inbound,
        "Synthetic QA weather report: the sky is clear and the temperature is pleasant.",
    ));
    assert_eq!(decision.provider, "cloudflare:clef-flash");
    assert!(
        !decision.rule.starts_with("fail:"),
        "the real provider contract must be admitted"
    );
    assert_eq!(decision.answers.len(), 2);
    assert!(decision
        .answers
        .values()
        .all(|p| p.is_finite() && (0.0..=1.0).contains(p)));
    println!(
        "{}",
        json!({"provider":decision.provider,"outcome":decision.outcome.id(),"rule":decision.rule,"answers":decision.answers,"latency_ms":decision.latency_ms,"synthetic_only":true})
    );
}
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{mpsc, Condvar},
    thread::{self, JoinHandle},
};

const PEER_FALLBACK: Duration = Duration::from_secs(3);
#[derive(Debug, Default)]
struct Observed {
    request: Vec<u8>,
    tls_bytes: usize,
    accepted_after_ms: Option<u128>,
    tls_read_started_after_ms: Option<u128>,
    tls_read_finished_after_ms: Option<u128>,
    tls_read_outcome: Option<Result<usize, String>>,
    tls_client_hello_record: bool,
    body_bytes_sent: usize,
    fallback_closed: bool,
}
struct Peer {
    url: String,
    observed: Arc<Mutex<Observed>>,
    stop: Arc<(Mutex<bool>, Condvar)>,
    socket: Arc<Mutex<Option<TcpStream>>>,
    thread: Option<JoinHandle<()>>,
    tls_published: mpsc::Receiver<()>,
}
enum Reply {
    HoldHeaders,
    HoldTls,
    Http {
        status: u16,
        body: Vec<u8>,
        length: Option<usize>,
        chunked: bool,
        drip: Duration,
        finish: bool,
    },
}
fn cancelled(stop: &(Mutex<bool>, Condvar), delay: Duration) -> bool {
    let (flag, signal) = stop;
    let (stopped, _) = signal
        .wait_timeout_while(flag.lock().unwrap(), delay, |stopped| !*stopped)
        .unwrap();
    *stopped
}
fn hold(peer: &(Mutex<bool>, Condvar), observed: &Mutex<Observed>, stream: &TcpStream) {
    if !cancelled(peer, PEER_FALLBACK) {
        observed.lock().unwrap().fallback_closed = true;
    }
    // A bounded fallback makes a deliberately unbounded negative terminate too;
    // the assertions require the transport to finish before this peer close.
    let _ = stream.shutdown(Shutdown::Both);
}
fn request(stream: &mut TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let mut buf = [0; 1024];
        let Ok(n) = stream.read(&mut buf) else {
            return Vec::new();
        };
        if n == 0 {
            return Vec::new();
        }
        bytes.extend_from_slice(&buf[..n]);
        assert!(
            bytes.len() <= 16 * 1024,
            "synthetic request must stay small"
        );
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let header_end = end + 4;
            let headers = std::str::from_utf8(&bytes[..header_end]).unwrap();
            let length = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= header_end + length {
                return bytes;
            }
        }
    }
}
impl Peer {
    fn start(reply: Reply) -> Self {
        // These tests bypass CLI/Net startup, which installs the same provider.
        // Install it explicitly rather than depending on another test's order.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let scheme = if matches!(&reply, Reply::HoldTls) {
            "https"
        } else {
            "http"
        };
        let url = format!("{scheme}://{}/gate-fixture", listener.local_addr().unwrap());
        let observed = Arc::new(Mutex::new(Observed::default()));
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let socket = Arc::new(Mutex::new(None));
        let (state, stopping, control) = (observed.clone(), stop.clone(), socket.clone());
        let (ready, started) = mpsc::sync_channel(1);
        let (publish_tls, tls_published) = mpsc::sync_channel(1);
        let epoch = Instant::now();
        let thread = thread::spawn(move || {
            ready.send(()).unwrap();
            let until = Instant::now() + PEER_FALLBACK;
            let mut stream = loop {
                if *stopping.0.lock().unwrap() {
                    return;
                }
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < until, "fixture was never contacted");
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("owned local accept: {error}"),
                }
            };
            state.lock().unwrap().accepted_after_ms = Some(epoch.elapsed().as_millis());
            stream.set_read_timeout(Some(PEER_FALLBACK)).unwrap();
            stream.set_write_timeout(Some(PEER_FALLBACK)).unwrap();
            *control.lock().unwrap() = Some(stream.try_clone().unwrap());
            if matches!(&reply, Reply::HoldTls) {
                let mut hello = [0; 4096];
                state.lock().unwrap().tls_read_started_after_ms = Some(epoch.elapsed().as_millis());
                let result = stream.read(&mut hello);
                {
                    let mut observed = state.lock().unwrap();
                    observed.tls_read_finished_after_ms = Some(epoch.elapsed().as_millis());
                    observed.tls_bytes = result.as_ref().copied().unwrap_or(0);
                    observed.tls_client_hello_record =
                        observed.tls_bytes >= 6 && hello[0] == 22 && hello[1] == 3 && hello[5] == 1;
                    observed.tls_read_outcome =
                        Some(result.map_err(|error| format!("{:?}", error.kind())));
                }
                // Publish only after the observation is complete. The caller
                // waits after measuring transport elapsed, outside its budget.
                let _ = publish_tls.send(());
                hold(&stopping, &state, &stream);
                return;
            }
            let bytes = request(&mut stream);
            state.lock().unwrap().request = bytes;
            match reply {
                Reply::HoldHeaders => hold(&stopping, &state, &stream),
                Reply::HoldTls => unreachable!(),
                Reply::Http {
                    status,
                    body,
                    length,
                    chunked,
                    drip,
                    finish,
                } => {
                    let encoding = if chunked {
                        "transfer-encoding: chunked\r\n".to_owned()
                    } else {
                        format!("content-length: {}\r\n", length.unwrap_or(body.len()))
                    };
                    let header = format!("HTTP/1.1 {status} fixture\r\ncontent-type: application/json\r\nconnection: close\r\n{encoding}\r\n");
                    if stream.write_all(header.as_bytes()).is_err() {
                        return;
                    }
                    let size = if drip.is_zero() { 17 } else { 1 };
                    for part in body.chunks(size) {
                        if !drip.is_zero() && cancelled(&stopping, drip) {
                            return;
                        }
                        if chunked && write!(stream, "{:x}\r\n", part.len()).is_err() {
                            return;
                        }
                        if stream.write_all(part).is_err() {
                            return;
                        }
                        if chunked && stream.write_all(b"\r\n").is_err() {
                            return;
                        }
                        state.lock().unwrap().body_bytes_sent += part.len();
                    }
                    if finish {
                        if chunked {
                            let _ = stream.write_all(b"0\r\n\r\n");
                        }
                    } else {
                        hold(&stopping, &state, &stream);
                    }
                }
            }
        });
        started.recv_timeout(PEER_FALLBACK).unwrap();
        Self {
            url,
            observed,
            stop,
            socket,
            thread: Some(thread),
            tls_published,
        }
    }
    fn did_receive_http(&self) {
        let state = self.observed.lock().unwrap();
        assert!(
            !state.request.is_empty(),
            "must exercise the actual HTTP request"
        );
        assert!(
            !state.fallback_closed,
            "transport must return before the peer closes it"
        );
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        *self.stop.0.lock().unwrap() = true;
        self.stop.1.notify_all();
        if let Some(socket) = self.socket.lock().unwrap().as_ref() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}
fn call(transport: &ReqwestTransport, peer: &Peer) -> Result<String, String> {
    transport.post_json(&peer.url, Some("fixture-token"), "{\"question\":true}")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gate_transport_accepts_exact_cap_and_preserves_post_and_http_error_body_semantics() {
    for status in [200, 500] {
        for chunked in [false, true] {
            let text = format!("{{\"fixture\":true}}{}", " ".repeat(48));
            assert_eq!(text.len(), 64);
            let peer = Peer::start(Reply::Http {
                status,
                body: text.as_bytes().to_vec(),
                length: None,
                chunked,
                drip: Duration::ZERO,
                finish: true,
            });
            let transport = ReqwestTransport::with_limits(
                reqwest::Client::builder().no_proxy(),
                Duration::from_secs(2),
                Duration::from_secs(1),
                64,
            )
            .unwrap();
            assert_eq!(call(&transport, &peer), Ok(text));
            peer.did_receive_http();
            let state = peer.observed.lock().unwrap();
            let request = String::from_utf8_lossy(&state.request);
            assert!(request.starts_with("POST /gate-fixture HTTP/1.1\r\n"));
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-token\r\n"));
            assert!(request
                .to_ascii_lowercase()
                .contains("content-type: application/json\r\n"));
            assert!(request.ends_with("{\"question\":true}"));
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gate_transport_bounds_missing_response_headers_before_peer_eof() {
    let peer = Peer::start(Reply::HoldHeaders);
    let transport = ReqwestTransport::with_limits(
        reqwest::Client::builder().no_proxy(),
        Duration::from_millis(400),
        Duration::from_millis(100),
        128,
    )
    .unwrap();
    let start = Instant::now();
    assert_eq!(call(&transport, &peer), Err("transport".into()));
    assert!(start.elapsed() < Duration::from_secs(2));
    peer.did_receive_http();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gate_transport_connect_deadline_covers_stalled_tls_before_later_total_deadline() {
    let peer = Peer::start(Reply::HoldTls);
    let transport = ReqwestTransport::with_limits(
        reqwest::Client::builder().no_proxy(),
        Duration::from_secs(3),
        Duration::from_millis(200),
        128,
    )
    .unwrap();
    let start = Instant::now();
    let result = call(&transport, &peer);
    let transport_elapsed = start.elapsed();
    let published = peer.tls_published.recv_timeout(PEER_FALLBACK);
    let state = peer.observed.lock().unwrap();
    println!(
        "local_tls_diagnostic transport={result:?} transport_elapsed_ms={} published={published:?} peer={state:?}",
        transport_elapsed.as_millis()
    );
    assert_eq!(result, Err("transport".into()));
    assert!(
        transport_elapsed < Duration::from_millis(1500),
        "connect must expire before the3s request/peer deadline"
    );
    assert!(published.is_ok(), "TLS peer must publish its read outcome");
    assert!(
        state.tls_bytes > 0,
        "actual TCP connection and TLS client hello required"
    );
    assert!(
        state.tls_client_hello_record,
        "TLS ClientHello record required"
    );
    assert!(!state.fallback_closed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gate_transport_total_deadline_is_not_extended_by_body_progress() {
    let peer = Peer::start(Reply::Http {
        status: 200,
        body: vec![b'x'; 32],
        length: None,
        chunked: false,
        drip: Duration::from_millis(75),
        finish: true,
    });
    let transport = ReqwestTransport::with_limits(
        reqwest::Client::builder().no_proxy(),
        Duration::from_millis(400),
        Duration::from_millis(100),
        128,
    )
    .unwrap();
    let start = Instant::now();
    assert_eq!(call(&transport, &peer), Err("body".into()));
    assert!(start.elapsed() < Duration::from_secs(2));
    peer.did_receive_http();
    let state = peer.observed.lock().unwrap();
    assert!(
        state.body_bytes_sent > 0 && state.body_bytes_sent < 32,
        "ongoing reads must not renew the total request budget"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gate_transport_rejects_large_declared_bodies_without_waiting_for_bytes_or_eof() {
    for status in [200, 500] {
        let peer = Peer::start(Reply::Http {
            status,
            body: Vec::new(),
            length: Some(129),
            chunked: false,
            drip: Duration::ZERO,
            finish: false,
        });
        let transport = ReqwestTransport::with_limits(
            reqwest::Client::builder().no_proxy(),
            Duration::from_secs(2),
            Duration::from_secs(1),
            128,
        )
        .unwrap();
        let start = Instant::now();
        assert_eq!(call(&transport, &peer), Err("response too large".into()));
        assert!(start.elapsed() < Duration::from_millis(1500));
        peer.did_receive_http();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gate_transport_rejects_oversized_chunked_bodies_before_eof_on_success_and_error() {
    for status in [200, 500] {
        let peer = Peer::start(Reply::Http {
            status,
            body: vec![b'x'; 129],
            length: None,
            chunked: true,
            drip: Duration::ZERO,
            finish: false,
        });
        let transport = ReqwestTransport::with_limits(
            reqwest::Client::builder().no_proxy(),
            Duration::from_secs(2),
            Duration::from_secs(1),
            128,
        )
        .unwrap();
        let start = Instant::now();
        assert_eq!(call(&transport, &peer), Err("response too large".into()));
        assert!(start.elapsed() < Duration::from_millis(1500));
        peer.did_receive_http();
    }
}

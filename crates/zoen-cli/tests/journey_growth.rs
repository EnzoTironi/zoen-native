//! Growth journey (ADR 0044): acquisition source → server-driven onboarding → experiment
//! arm → exposure → funnel by source and by arm, on a real relay with real `zoen` installs.
//! The remote config changes mid-test through the admin endpoint (no new build), which is
//! also how the test pins each install's arm.
//!
//!   ZOEN_TEST_PG=... cargo test -p zoen-cli --test journey_growth

mod common;
#[path = "journey_growth/onboarding.rs"]
mod onboarding;
use client_core::{Client, SecretVault};
use common::*;
use std::io::{Read, Write};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

const TOKEN: &str = "journey-admin-token-0123456789abcdef";

struct Vault(PathBuf);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(key)).ok()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        std::fs::create_dir_all(&self.0).is_ok() && std::fs::write(self.0.join(key), value).is_ok()
    }
    fn delete(&self, key: String) {
        let _ = std::fs::remove_file(self.0.join(key));
    }
}

#[tokio::test]
async fn reports_use_the_current_ready_relay_domain_and_keep_refused_intents() {
    use protocol::experiments::report_message;
    use sha2::{Digest, Sha256};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let mut w = World::new("growth_domain").await;
    let host = format!("127.0.0.1:{}", w.port);
    assert_ne!(w.relay_name(), host);
    w.init("ana", "Ana");
    let home = w.dir.join("ana");
    let key = event_log::Signer::from_secret(
        &std::fs::read(home.join("vault/zoen.device.v1"))
            .unwrap()
            .try_into()
            .unwrap(),
    );
    let body = r#"{"unit":"ffffffffffffffffffffffffffffffff","attribution":null,"exposures":[],"health":{"sessions":1,"crashes":0}}"#;
    let ts = now_ms();
    let sig = key.sign(&report_message(
        &hex::encode(Sha256::digest(body)),
        ts,
        &host,
    ));
    assert_eq!(
        http(
            w.port,
            "POST",
            "/v1/report",
            &[
                ("Content-Type", "application/json"),
                ("x-zoen-device", &key.id()),
                ("x-zoen-ts", &ts.to_string()),
                ("x-zoen-sig", &sig),
            ],
            body,
        )
        .0,
        401,
        "a URL-host signature must not authenticate as the named relay"
    );

    let core = Client::open(
        home.join("zoen.sqlite").to_string_lossy().into(),
        "en-US".into(),
    )
    .unwrap();
    assert!(core.unlock(Arc::new(Vault(home.join("vault")))).unwrap());
    core.growth_capture_link("zoen://friend/ana?c=domain_regression".into());
    let error = core.growth_sync(None, false, 0, 0).await.unwrap_err();
    assert!(
        error.to_string().contains("Connect to this relay"),
        "{error}"
    );
    assert_eq!(
        core.growth_acquisition().campaign.as_deref(),
        Some("domain_regression")
    );
    core.start_sync(None).unwrap();
    core.wait_until_settled(8000).await.unwrap();

    // A public config override must not receive account report credentials.
    let endpoint = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let override_url = format!("http://{}", endpoint.local_addr().unwrap());
    let (requests, mut received) = tokio::sync::mpsc::unbounded_channel();
    let config = tokio::spawn(async move {
        loop {
            let (socket, _) = endpoint.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            let mut header = String::new();
            loop {
                let mut line = String::new();
                assert_ne!(socket.read_line(&mut line).await.unwrap(), 0);
                header.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            requests.send(header).unwrap();
            let body = protocol::experiments::DEFAULT_CONFIG;
            socket
                .get_mut()
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    let error = core
        .growth_sync(Some(override_url), false, 0, 0)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("Connect to this relay"),
        "{error}"
    );
    let request = received.try_recv().unwrap();
    assert!(request.starts_with("GET /v1/config "), "{request}");
    assert!(!request.to_lowercase().contains("x-zoen-"), "{request}");
    assert!(received.try_recv().is_err(), "override received a report");
    config.abort();

    // No health or exposures can mask a consumed source intent. Both refusals
    // must leave that sole report queued, and success must consume it once.
    assert!(core.growth_sync(None, false, 0, 0).await.unwrap().reported);
    assert!(!core.growth_sync(None, false, 0, 0).await.unwrap().reported);

    let previous_name = w.relay_name().to_string();
    core.stop_sync();
    w.stop_relay();
    w.start_relay();
    assert_ne!(w.relay_name(), previous_name);
    let error = core.growth_sync(None, true, 1, 0).await.unwrap_err();
    assert!(
        error.to_string().contains("Connect to this relay"),
        "{error}"
    );
    core.start_sync(None).unwrap();
    core.wait_until_settled(8000).await.unwrap();
    assert!(core.growth_sync(None, true, 1, 0).await.unwrap().reported);
}

struct Request {
    header: String,
    body: Vec<u8>,
    bytes: Vec<u8>,
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> Request {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    let header_end = loop {
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
        let mut chunk = [0; 4096];
        let n = socket.read(&mut chunk).await.unwrap();
        assert_ne!(n, 0, "socket closed before HTTP headers");
        bytes.extend_from_slice(&chunk[..n]);
        assert!(bytes.len() <= 1024 * 1024, "oversized test request");
    };
    let header = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
    let length = header
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    assert!(header_end + length <= 1024 * 1024);
    while bytes.len() < header_end + length {
        let mut chunk = [0; 4096];
        let n = socket.read(&mut chunk).await.unwrap();
        assert_ne!(n, 0, "socket closed before HTTP body");
        bytes.extend_from_slice(&chunk[..n]);
    }
    Request {
        header,
        body: bytes[header_end..header_end + length].to_vec(),
        bytes,
    }
}

#[tokio::test]
async fn signed_source_report_cannot_follow_a_redirect_and_stays_queued() {
    use tokio::io::AsyncWriteExt;
    let w = World::new("growth_redirect").await;
    let foreign = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let destination = format!("http://{}/v1/report", foreign.local_addr().unwrap());
    let (requests, mut received) = tokio::sync::mpsc::unbounded_channel();
    let foreign_task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = foreign.accept().await.unwrap();
            let request = read_request(&mut socket).await;
            requests.send(request).unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
        }
    });
    let endpoint = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_url = format!("http://{}", endpoint.local_addr().unwrap());
    let redirect = Arc::new(AtomicBool::new(true));
    let is_redirecting = redirect.clone();
    let port = w.port;
    let proxy = tokio::spawn(async move {
        loop {
            let (mut socket, _) = endpoint.accept().await.unwrap();
            let destination = destination.clone();
            let redirect = is_redirecting.clone();
            tokio::spawn(async move {
                let request = read_request(&mut socket).await;
                if request.header.starts_with("POST /v1/report ") && redirect.load(Ordering::SeqCst)
                {
                    socket
                        .write_all(
                            format!(
                                "HTTP/1.1 307 Temporary Redirect\r\nLocation: {destination}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            )
                            .as_bytes(),
                        )
                        .await
                        .unwrap();
                } else {
                    let mut relay = tokio::net::TcpStream::connect(("127.0.0.1", port))
                        .await
                        .unwrap();
                    if request.header.to_lowercase().contains("upgrade: websocket") {
                        relay.write_all(&request.bytes).await.unwrap();
                    } else {
                        // Parse every HTTP report separately, even when the
                        // client would reuse its public-config connection.
                        let header: Vec<_> = request
                            .header
                            .lines()
                            .filter(|line| {
                                !line.is_empty() && !line.to_lowercase().starts_with("connection:")
                            })
                            .collect();
                        relay
                            .write_all(
                                format!("{}\r\nConnection: close\r\n\r\n", header.join("\r\n"))
                                    .as_bytes(),
                            )
                            .await
                            .unwrap();
                        relay.write_all(&request.body).await.unwrap();
                    }
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut relay).await;
                }
            });
        }
    });
    let home = w.dir.join("ana");
    std::fs::create_dir_all(&home).unwrap();
    let core = Client::open(
        home.join("zoen.sqlite").to_string_lossy().into(),
        "en-US".into(),
    )
    .unwrap();
    core.create_account(
        "Ana".into(),
        "ana".into(),
        relay_url,
        Arc::new(Vault(home.join("vault"))),
    )
    .unwrap();
    core.growth_capture_link("zoen://friend/ana?c=redirect_regression".into());
    core.start_sync(None).unwrap();
    core.wait_until_settled(8000).await.unwrap();
    // The WebSocket handshake and Ready above came from the real relay. Only
    // the authenticated endpoint's HTTP report reply is replaced by a redirect.
    let refused = core.growth_sync(None, false, 0, 0).await;
    let escaped = received.try_recv();
    eprintln!(
        "[growth-redirect] result={refused:?} external_request={:?}",
        escaped.as_ref().map(|request| (
            request.header.lines().next(),
            request.header.to_lowercase().contains("x-zoen-sig:"),
            request.body.len(),
        ))
    );
    assert!(
        escaped.is_err(),
        "foreign endpoint received a signed report"
    );
    let error = refused.unwrap_err();
    assert!(error.to_string().contains("307"), "{error}");
    redirect.store(false, Ordering::SeqCst);
    assert!(core.growth_sync(None, false, 0, 0).await.unwrap().reported);
    assert!(!core.growth_sync(None, false, 0, 0).await.unwrap().reported);
    assert!(
        received.try_recv().is_err(),
        "foreign endpoint received a retry"
    );
    core.stop_sync();
    proxy.abort();
    foreign_task.abort();
}

fn http(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (u16, String, String) {
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let mut extra = String::new();
    for (k, v) in headers {
        extra.push_str(&format!("{k}: {v}\r\n"));
    }
    write!(
        s,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let status = out[9..12].parse().unwrap();
    let (head, body) = out.split_once("\r\n\r\n").unwrap_or((&out, ""));
    let body = if head.to_lowercase().contains("transfer-encoding: chunked") {
        let mut rest = body;
        let mut joined = String::new();
        while let Some((len, tail)) = rest.split_once("\r\n") {
            let n = usize::from_str_radix(len.trim(), 16).unwrap_or(0);
            if n == 0 {
                break;
            }
            joined.push_str(&tail[..n]);
            rest = &tail[n + 2..];
        }
        joined
    } else {
        body.to_string()
    };
    (status, head.to_string(), body)
}

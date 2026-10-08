//! S7 journey: two relay nodes export traces and logs over OTLP to a collector stand-in while
//! Ana (node A) and Bruno (node B) chat, a group grows through an invite, and a third client
//! connects from a distinctive address. Then it reads every byte the relays exported and
//! every line they logged at debug level:
//! - the work is traced: handshake, request, publish → fdb.append, sync, and a publish on
//!   node A continues on node B as `bus.deliver`, in the same trace;
//! - nothing identifying leaves: no identity, device or Space id, handle, invite code, chat
//!   name, message text or client address, in any exported span, log record or stdout line.
//!
//! Needs ZOEN_TEST_PG, FoundationDB and NATS:
//!   eval "$(scripts/fdb.sh env)"; eval "$(scripts/nats.sh env)"
//!   cargo test -p zoen-cli --test journey_telemetry

mod common;
use common::*;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread::sleep,
    time::Duration,
};

use opentelemetry_proto::tonic::{
    collector::{logs::v1::ExportLogsServiceRequest, trace::v1::ExportTraceServiceRequest},
    common::v1::{any_value::Value, KeyValue},
    trace::v1::Span,
};
use prost::Message;

/// Request bodies by path, in arrival order.
type Received = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// An OTLP/HTTP receiver: keeps every request body by path and answers 200.
struct Collector {
    port: u16,
    got: Received,
}

impl Collector {
    fn start() -> Collector {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let got: Received = Arc::default();
        let sink = got.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                let sink = sink.clone();
                std::thread::spawn(move || {
                    let mut out = conn.try_clone().unwrap();
                    let mut r = BufReader::new(conn);
                    loop {
                        let mut line = String::new();
                        if r.read_line(&mut line).unwrap_or(0) == 0 {
                            return;
                        }
                        let path = line.split(' ').nth(1).unwrap_or("").to_string();
                        let mut len = 0usize;
                        loop {
                            let mut h = String::new();
                            r.read_line(&mut h).unwrap();
                            if h.trim().is_empty() {
                                break;
                            }
                            if let Some((k, v)) = h.split_once(':') {
                                if k.eq_ignore_ascii_case("content-length") {
                                    len = v.trim().parse().unwrap();
                                }
                            }
                        }
                        let mut body = vec![0; len];
                        r.read_exact(&mut body).unwrap();
                        sink.lock().unwrap().push((path, body));
                        out.write_all(
                            b"HTTP/1.1 200 OK\r\ncontent-type: application/x-protobuf\r\ncontent-length: 0\r\n\r\n",
                        )
                        .unwrap();
                    }
                });
            }
        });
        Collector { port, got }
    }

    fn bodies(&self, path: &str) -> Vec<Vec<u8>> {
        self.got
            .lock()
            .unwrap()
            .iter()
            .filter(|(p, _)| p == path)
            .map(|(_, b)| b.clone())
            .collect()
    }

    fn spans(&self) -> Vec<Span> {
        self.bodies("/v1/traces")
            .iter()
            .flat_map(|b| {
                ExportTraceServiceRequest::decode(b.as_slice())
                    .unwrap()
                    .resource_spans
            })
            .flat_map(|r| r.scope_spans)
            .flat_map(|s| s.spans)
            .collect()
    }

    fn log_bodies(&self) -> Vec<(String, Vec<u8>)> {
        self.bodies("/v1/logs")
            .iter()
            .flat_map(|b| {
                ExportLogsServiceRequest::decode(b.as_slice())
                    .unwrap()
                    .resource_logs
            })
            .flat_map(|r| r.scope_logs)
            .flat_map(|s| s.log_records)
            .map(|l| {
                let body = match l.body.and_then(|b| b.value) {
                    Some(Value::StringValue(s)) => s,
                    _ => String::new(),
                };
                (body, l.trace_id)
            })
            .collect()
    }
}

fn attr<'a>(s: &'a Span, key: &str) -> Option<&'a Value> {
    s.attributes
        .iter()
        .find(|KeyValue { key: k, .. }| k == key)
        .and_then(|kv| kv.value.as_ref())
        .and_then(|v| v.value.as_ref())
}

fn attr_str(s: &Span, key: &str) -> String {
    match attr(s, key) {
        Some(Value::StringValue(v)) => v.clone(),
        Some(Value::IntValue(v)) => v.to_string(),
        Some(Value::BoolValue(v)) => v.to_string(),
        other => format!("{other:?}"),
    }
}

const EXPECTED: [&str; 7] = [
    "handshake",
    "request",
    "publish",
    "fdb.append",
    "sync",
    "ephemeral",
    "bus.deliver",
];

fn named<'a>(spans: &'a [Span], name: &'a str) -> impl Iterator<Item = &'a Span> {
    spans.iter().filter(move |s| s.name == name)
}

const CLIENT_IP: &str = "203.0.113.77";
const IP_HEADER: &str = "x-zoen-test-client-ip";

#[tokio::test]
async fn relays_trace_the_work_and_export_nothing_identifying() {
    let collector = Collector::start();
    let endpoint = format!("http://127.0.0.1:{}", collector.port);
    let nats = std::env::var("ZOEN_NATS_URL").expect("eval \"$(scripts/nats.sh env)\"");
    let mut w = World::new_with(
        "telemetry",
        Some(nats),
        &[
            ("OTEL_EXPORTER_OTLP_ENDPOINT", &endpoint),
            ("OTEL_BSP_SCHEDULE_DELAY", "200"),
            ("OTEL_BLRP_SCHEDULE_DELAY", "200"),
            ("RUST_LOG", "debug"),
            ("LOG_FORMAT", "json"),
            ("ZOEN_CLIENT_IP_HEADER", IP_HEADER),
        ],
    )
    .await;
    let b = w.start_node();

    let marker = format!("segredo-{}", &w.cell[w.cell.len() - 8..]);
    let group = format!("Trilha {}", &w.cell[w.cell.len() - 8..]);
    w.init("anapii", "Ana Pii");
    let out = w.zoen_at(
        b,
        "brunopii",
        &["init", "--name", "Bruno Pii", "--handle", "brunopii"],
    );
    assert!(out.contains("registered"), "{out}");
    w.zoen("anapii", &["dm", "@brunopii", &format!("oi {marker}")]);

    let watcher = w.spawn_zoen_at(b, "brunopii", &["watch", "--for", "6"]);
    sleep(Duration::from_millis(2500));
    w.zoen("anapii", &["typing", "@brunopii", "--for", "1"]);
    w.zoen(
        "anapii",
        &["send", "@brunopii", &format!("ao vivo {marker}")],
    );
    let watched = String::from_utf8_lossy(&watcher.wait_with_output().unwrap().stdout).to_string();
    assert!(watched.contains(&format!("ao vivo {marker}")), "{watched}");

    w.zoen("anapii", &["group", &group, "@brunopii"]);
    let code = w.zoen("anapii", &["invite", &group]);
    let code = code.split('\t').next().unwrap().trim().to_string();
    assert_eq!(code.len(), 10);
    let (_carla, registered) =
        RawClient::connect_registering_with(&w.relay_url(), "carlapii", &[(IP_HEADER, CLIENT_IP)])
            .await;
    registered.expect("carla registers");

    // Both relays' batch exporters flush every 200 ms.
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while std::time::Instant::now() < deadline
        && !EXPECTED
            .iter()
            .all(|n| named(&collector.spans(), n).count() > 0)
    {
        sleep(Duration::from_millis(250));
    }
    sleep(Duration::from_millis(600));
    let spans = collector.spans();

    // ── the work is traced ──
    let named = |n: &'static str| named(&spans, n);
    for n in EXPECTED {
        assert!(
            named(n).count() > 0,
            "no {n} span among {:?}",
            spans.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
    }
    assert!(named("GET /v1/sync").any(|s| attr_str(s, "http.route") == "/v1/sync"));
    assert!(named("handshake").any(|s| attr_str(s, "outcome") == "ok"));
    assert!(named("request").any(|s| attr_str(s, "op") == "create_invite"));
    let by_id: HashMap<&[u8], &Span> = spans.iter().map(|s| (s.span_id.as_slice(), s)).collect();
    let sequenced: Vec<&Span> = named("publish")
        .filter(|s| attr_str(s, "outcome") == "sequenced")
        .collect();
    assert!(!sequenced.is_empty());
    assert!(
        named("fdb.append").any(|a| by_id
            .get(a.parent_span_id.as_slice())
            .is_some_and(|p| p.name == "publish" && p.trace_id == a.trace_id)),
        "fdb.append runs inside publish"
    );
    let crossed: Vec<&Span> = named("bus.deliver")
        .filter(|d| {
            by_id.get(d.parent_span_id.as_slice()).is_some_and(|p| {
                (p.name == "publish" || p.name == "ephemeral") && p.trace_id == d.trace_id
            })
        })
        .collect();
    assert!(
        crossed.iter().any(|d| attr_str(d, "delivered_here") != "0"),
        "a publish on node A continues on node B in the same trace"
    );
    for s in &sequenced {
        assert!(attr_str(s, "identity").starts_with("p:"), "{s:?}");
        assert!(attr_str(s, "space").starts_with("p:"), "{s:?}");
    }
    let logs = collector.log_bodies();
    assert!(logs.iter().any(|(b, _)| b == "session ready"), "{logs:?}");
    assert!(
        logs.iter()
            .any(|(b, t)| b == "space created" && t.len() == 16),
        "a log inside a span carries its trace: {logs:?}"
    );

    // ── nothing identifying leaves ──
    let mut secrets: Vec<(String, String)> = vec![
        ("handle".into(), "anapii".into()),
        ("handle".into(), "brunopii".into()),
        ("handle".into(), "carlapii".into()),
        ("name".into(), "Bruno Pii".into()),
        ("message text".into(), marker.clone()),
        ("chat name".into(), group.clone()),
        ("invite code".into(), code.clone()),
        ("client address".into(), CLIENT_IP.into()),
    ];
    for id in sqlx_strings(&w, "SELECT id FROM identities").await {
        for s in w.spaces_of_id(&id).await {
            secrets.push(("space id".into(), s));
        }
        secrets.push(("identity id".into(), id));
    }
    for d in sqlx_strings(&w, "SELECT device FROM devices").await {
        secrets.push(("device id".into(), d));
    }
    assert!(secrets.iter().filter(|(k, _)| k == "identity id").count() == 3);
    assert!(secrets.iter().any(|(k, _)| k == "space id"));

    let exported: Vec<u8> = collector
        .got
        .lock()
        .unwrap()
        .iter()
        .flat_map(|(_, b)| b.clone())
        .collect();
    let stdout = w.relay_log_text();
    assert!(
        stdout.contains("\"level\":\"DEBUG\""),
        "the relays logged at debug level"
    );
    for (what, value) in &secrets {
        assert!(
            !contains(&exported, value.as_bytes()),
            "a {what} ({value}) reached the OTLP export"
        );
        assert!(
            !stdout.contains(value.as_str()),
            "a {what} ({value}) reached the relay's log"
        );
    }
    eprintln!(
        "telemetry: {} spans, {} log records, {} bytes exported, {} stdout bytes, {} secrets checked",
        spans.len(),
        logs.len(),
        exported.len(),
        stdout.len(),
        secrets.len()
    );
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

async fn sqlx_strings(w: &World, sql: &str) -> Vec<String> {
    use sqlx::Connection;
    let mut c = sqlx::PgConnection::connect(&w.db_url).await.unwrap();
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_all(&mut c)
        .await
        .unwrap()
}

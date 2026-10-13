//! A linked device receipts its box, and a later fetch finds nothing.

mod common;
use common::*;
use roda_log::{Author, Signer};
use roda_proto::{Op, Reply};
use sqlx::Connection;
use std::io::BufRead;

fn lines_of(child: &mut std::process::Child) -> std::sync::mpsc::Receiver<String> {
    let out = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

fn wait_line(rx: &std::sync::mpsc::Receiver<String>, prefix: &str) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok(line) if line.starts_with(prefix) => return line[prefix.len()..].trim().to_string(),
            Ok(_) => {}
            Err(_) => panic!("no {prefix:?} line"),
        }
    }
}

fn box_id(secret: &[u8; 32], label: &str) -> String {
    let mut bytes = b"zoen-link-box-v1\0".to_vec();
    bytes.extend(secret);
    bytes.extend(label.as_bytes());
    roda_log::sha256_hex(&bytes)
}

#[tokio::test]
async fn a_linked_device_receipts_its_box_and_a_later_fetch_finds_nothing() {
    let w = World::new("linkreceipt").await;
    w.init("ana", "Ana");
    let mut notebook = w.spawn_zoen("ana-notebook", &["link-request", "--for", "90"]);
    let rx = lines_of(&mut notebook);
    let code = wait_line(&rx, "code:");
    let hexed = code
        .strip_prefix("zoen-link:1:")
        .expect("link code")
        .to_string();
    assert_eq!(hexed.len(), 192, "{code}");
    let secret: [u8; 32] = hex::decode(&hexed[128..]).unwrap().try_into().unwrap();
    let identity_box = box_id(&secret, "identity");
    let history_box = box_id(&secret, "history");
    let linked = w.zoen("ana", &["link", &code]);
    assert!(linked.contains("history sent:"), "{linked}");
    let linked_line = wait_line(&rx, "linked:");
    assert!(linked_line.starts_with("@ana "), "{linked_line}");
    let out = notebook.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mut pg = sqlx::PgConnection::connect(&w.db_url).await.unwrap();
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM link_boxes")
        .fetch_one(&mut pg)
        .await
        .unwrap();
    assert_eq!(left, 0);

    let read_key = |name: &str| {
        let bytes: [u8; 32] = std::fs::read(w.dir.join("ana-notebook/vault").join(name))
            .unwrap()
            .try_into()
            .unwrap();
        Signer::from_secret(&bytes)
    };
    let author = Author::device(&read_key("zoen.identity.v1"), read_key("zoen.device.v1"));
    let mut client = RawClient::reconnect(&w.relay_url(), author).await;
    assert_eq!(
        client
            .request(Op::FetchLink {
                id: identity_box.clone(),
            })
            .await
            .unwrap(),
        Reply::Link(None)
    );
    assert_eq!(
        client
            .request(Op::FetchLink { id: history_box })
            .await
            .unwrap(),
        Reply::Link(None)
    );
}

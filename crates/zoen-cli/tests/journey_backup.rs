//! Encrypted server backup (ADR 0046): Ana turns on a password backup, loses her phone and
//! gets her account and history back on a new one; the relay never holds anything readable;
//! ten wrong passwords lock a backup for good; a recovery key works without the vault.
//!
//!   ZOEN_TEST_PG=postgres://zoen@127.0.0.1:15433/postgres cargo test -p zoen-cli --test journey_backup

#[path = "common/backup_store.rs"]
mod backup_store;
mod common;
use common::*;
use types::EventBody;

const VAULT_KEY: &str = "5ec0e7d5ec0e7d5ec0e7d5ec0e7d5ec0e7d5ec0e7d5ec0e7d5ec0e7d5ec0e7d0";
const PASSWORD: &str = "trilha no sábado 42";

async fn world(name: &str) -> World {
    World::with_env(
        name,
        &[
            ("ZOEN_BACKUP_VAULT_KEY", VAULT_KEY),
            ("ZOEN_DEV_ALLOW_UNAUTHENTICATED_PASSWORD_BACKUP", "1"),
            ("ZOEN_LIMITS", "backup_restore_ip=600/h:600"),
        ],
    )
    .await
}

fn backup_files(w: &World) -> Vec<Vec<u8>> {
    let dir = w.dir.join("blobs").join("backups");
    fn visit(dir: &std::path::Path, files: &mut Vec<Vec<u8>>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit(&path, files);
                } else {
                    files.push(std::fs::read(path).unwrap());
                }
            }
        }
    }
    let mut files = Vec::new();
    visit(&dir, &mut files);
    files
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

fn device_key(w: &World, who: &str) -> event_log::Signer {
    let bytes = std::fs::read(w.dir.join(who).join("vault/zoen.device.v1")).unwrap();
    event_log::Signer::from_secret(&bytes.try_into().unwrap())
}

fn signed_backup_request(
    w: &World,
    key: &event_log::Signer,
    identity: &str,
    method: &str,
    op: &str,
    path: &str,
    body: &[u8],
) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let ts = now_ms();
    let relay = format!("127.0.0.1:{}", w.port);
    let sig = key.sign(&protocol::backup_message(
        w.relay_name(),
        identity,
        op,
        &hex::encode(Sha256::digest(body)),
        ts,
    ));
    let headers = format!(
        "{method} {path} HTTP/1.1\r\nHost: {relay}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\nx-zoen-device: {}\r\nx-zoen-ts: {ts}\r\nx-zoen-sig: {sig}\r\n\r\n",
        body.len(), key.id(),
    );
    [headers.as_bytes(), body].concat()
}

async fn backup_http(w: &World, request: &[u8]) -> (u16, Vec<u8>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let response = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", w.port))
            .await
            .unwrap();
        socket.write_all(request).await.unwrap();
        let mut response = Vec::new();
        socket.read_to_end(&mut response).await.unwrap();
        response
    })
    .await
    .unwrap();
    let split = response
        .windows(4)
        .position(|b| b == b"\r\n\r\n")
        .expect("HTTP response headers");
    let status = std::str::from_utf8(&response[..split])
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (status, response[split + 4..].to_vec())
}

async fn signed_backup_write(
    w: &World,
    key: &event_log::Signer,
    identity: &str,
    method: &str,
    op: &str,
    path: &str,
    body: &[u8],
) -> u16 {
    let request = signed_backup_request(w, key, identity, method, op, path, body);
    backup_http(w, &request).await.0
}

async fn restore_post(w: &World, path: &str, body: serde_json::Value) -> (u16, Vec<u8>) {
    let body = body.to_string();
    let headers = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        w.port, body.len(),
    );
    backup_http(w, &[headers.as_bytes(), body.as_bytes()].concat()).await
}

include!("journey_backup/enrollment.rs");
include!("journey_backup/storage.rs");
include!("journey_backup/password_recovery.rs");
include!("journey_backup/mls_recovery.rs");

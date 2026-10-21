//! Authenticated public restore regression corpus. All identities and secrets are synthetic.
use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Default)]
struct Vault {
    values: Mutex<std::collections::HashMap<String, Vec<u8>>>,
    writes: AtomicUsize,
}
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.values.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.values.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.values.lock().unwrap().remove(&key);
    }
}

fn snapshot() -> (Header, Vec<u8>) {
    let source = Client::open(":memory:".into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    source
        .create_account(
            "Backup source".into(),
            "backup_source".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    let space = source
        .create_group_with("Portable history".into(), vec![], crate::PrivacyDto::Closed)
        .unwrap();
    source
        .send_message(space, "Confirmed portable content".into())
        .unwrap();
    // Only the actual confirmed log is portable. Sequence the synthetic source's
    // public writes through the same production ingest path before exporting.
    let pending = source.lock().store.outbox().unwrap();
    for row in pending {
        let mut engine = source.lock();
        let mut log = engine
            .logs
            .get(&row.event.space)
            .cloned()
            .unwrap_or_else(|| event_log::SpaceLog::new(&row.event.space));
        let event = log.sequence(row.event);
        assert_eq!(
            engine.ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(event)
            }),
            crate::sync::Ingest::Confirmed
        );
    }
    let root = vault.load(VAULT_IDENTITY.into()).unwrap();
    let agreement = vault.load(VAULT_AGREEMENT.into()).unwrap();
    let header = Header {
        version: 1,
        identity: source.account().unwrap().identity_id,
        relay_url: "http://127.0.0.1:1".into(),
        identity_secret: hex::encode(root),
        agreement_secret: hex::encode(agreement),
        created_ms: 1_700_000_000_000,
    };
    let database = source.lock().backup_snapshot().unwrap();
    (header, database)
}

fn change_database(database: &[u8], sql: &str) -> Vec<u8> {
    let path = std::env::temp_dir().join(types::new_id("backup-structural-fixture"));
    std::fs::write(&path, database).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("PRAGMA journal_mode=DELETE;")
        .unwrap();
    connection.execute_batch(sql).unwrap();
    connection.execute_batch("VACUUM;").unwrap();
    drop(connection);
    let result = std::fs::read(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    result
}

fn seeded_target() -> Arc<Client> {
    let target = Client::open(":memory:".into(), "en".into()).unwrap();
    let mut engine = target.lock();
    engine
        .create_identity(
            types::Identity {
                id: String::new(),
                kind: types::IdentityKind::Person,
                name: "Existing local content".into(),
                handle: "local_content".into(),
                tint_hex: "#123456".into(),
                glyph: None,
                owner: None,
                bio: String::new(),
                owner_proof: None,
            },
            true,
        )
        .unwrap();
    engine
        .store
        .set_meta("structural-target-marker", "keep this exact target")
        .unwrap();
    assert!(!engine.is_empty());
    assert!(engine.account().is_none());
    drop(engine);
    target
}

async fn request(socket: &mut tokio::net::TcpStream) -> (String, String, Vec<u8>) {
    let mut bytes = Vec::new();
    let end = loop {
        let mut buffer = [0; 4096];
        let n = socket.read(&mut buffer).await.unwrap();
        assert_ne!(n, 0);
        bytes.extend_from_slice(&buffer[..n]);
        assert!(bytes.len() < 64 * 1024);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
    let length = headers
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .map(|(_, value)| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while bytes.len() < end + length {
        let mut buffer = [0; 4096];
        let n = socket.read(&mut buffer).await.unwrap();
        assert_ne!(n, 0);
        bytes.extend_from_slice(&buffer[..n]);
    }
    let path = headers
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();
    (path, headers, bytes[end..end + length].to_vec())
}

async fn public_restore_case(case: &str) {
    let (mut header, healthy) = snapshot();
    let database = match case {
        "empty" => change_database(&[], "CREATE TABLE removed (x); DROP TABLE removed;"),
        "partial" => change_database(&healthy, "DROP TABLE events;"),
        "malformed" => b"authenticated bytes that are not SQLite".to_vec(),
        "agreement-short" => {
            header.agreement_secret = hex::encode([92; 31]);
            healthy
        }
        _ => unreachable!(),
    };
    let key = [93; 32];
    let recovery = "7".repeat(64);
    let keys = backup::recovery_keys(&recovery).unwrap();
    let auth = hex::encode(keys.auth);
    let generation = "ab".repeat(32);
    let wrapped = hex::encode(backup::wrap_key(&keys, &header.identity, &key));
    let sealed = backup::seal_payload(&key, &header, &database).unwrap();
    // Prove that the deliberately bad database is a valid authenticated payload,
    // not a wrong-key, format-header or HTTP fixture failure.
    let (opened, bytes) = backup::open_payload(&key, &header.identity, &sealed).unwrap();
    assert_eq!(opened.identity, header.identity);
    assert_eq!(bytes, database);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let enrolls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::new(Mutex::new(Vec::new()));
    let counted = enrolls.clone();
    let calls = observed.clone();
    let identity = header.identity.clone();
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let (path, headers, body) = request(&mut socket).await;
            calls.lock().unwrap().push(path.clone());
            let reply = match path.as_str() {
                "/v1/backup/restore/start" => {
                    let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(request["recovery_only"], true);
                    serde_json::json!({"identity":identity, "mode":"recovery_key", "kdf":null, "evaluated":null, "generation":generation}).to_string().into_bytes()
                }
                "/v1/backup/restore/open" => {
                    let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(request["auth_key"], auth);
                    assert_eq!(request["generation"], generation);
                    serde_json::json!({"wrapped_key":wrapped, "generation":generation})
                        .to_string()
                        .into_bytes()
                }
                path if path.starts_with("/v1/backup/restore/blob?") => {
                    assert!(headers.lines().any(|line| line
                        .split_once(':')
                        .is_some_and(|(name, value)| name
                            .eq_ignore_ascii_case("x-zoen-backup-auth")
                            && value.trim() == auth)));
                    sealed.clone()
                }
                "/v1/backup/restore/enroll" => {
                    let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(request["auth_key"], auth);
                    assert_eq!(request["generation"], generation);
                    counted.fetch_add(1, Ordering::SeqCst);
                    b"{}".to_vec()
                }
                _ => panic!("unexpected restore request {path}"),
            };
            let prefix = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", reply.len());
            socket.write_all(prefix.as_bytes()).await.unwrap();
            socket.write_all(&reply).await.unwrap();
        }
    });
    let target = seeded_target();
    let before = target.lock().store.memory_image().unwrap();
    let vault = Arc::new(Vault::default());
    vault
        .values
        .lock()
        .unwrap()
        .insert("keep-vault-marker".into(), vec![17, 18, 19]);
    let vault_before = vault.values.lock().unwrap().clone();
    let result = target
        .restore_backup_with_recovery_key(
            format!("http://{address}"),
            "backup_source".into(),
            recovery,
            vault.clone(),
        )
        .await;
    server.abort();
    let enrolls = enrolls.load(Ordering::SeqCst);
    let writes = vault.writes.load(Ordering::SeqCst);
    let unchanged = target.lock().store.memory_image().unwrap() == before;
    eprintln!("authenticated {case} SQLite: accepted={}, enrolls={enrolls}, vault_writes={writes}, target_unchanged={unchanged}", result.is_ok());
    assert!(
        observed
            .lock()
            .unwrap()
            .iter()
            .any(|path| path.starts_with("/v1/backup/restore/blob?")),
        "fixture must reach authenticated download"
    );
    assert!(result.is_err() && enrolls == 0 && writes == 0 && unchanged, "{case}: invalid authenticated SQLite must be refused before enrollment, vault writes or target mutation");
    assert_eq!(*vault.values.lock().unwrap(), vault_before);
    assert!(target.account().is_none());
}

#[tokio::test]
async fn structural_restore_authenticated_empty_sqlite_is_refused_without_side_effects() {
    public_restore_case("empty").await;
}
#[tokio::test]
async fn structural_restore_authenticated_partial_sqlite_is_refused_without_side_effects() {
    public_restore_case("partial").await;
}
#[tokio::test]
async fn structural_restore_authenticated_malformed_sqlite_is_refused_without_side_effects() {
    public_restore_case("malformed").await;
}

#[tokio::test]
async fn structural_restore_authenticated_short_agreement_secret_is_refused_without_side_effects() {
    public_restore_case("agreement-short").await;
}

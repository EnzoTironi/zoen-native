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

fn snapshot() -> std::result::Result<(Header, Vec<u8>), Box<dyn std::error::Error>> {
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
        let event = log.sequence(row.event)?;
        assert_eq!(
            engine.ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(event)?
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
    Ok((header, database))
}

fn change_database(
    database: &[u8],
    sql: &str,
) -> std::result::Result<Vec<u8>, Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(types::new_id("backup-structural-fixture")?);
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
    Ok(result)
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

fn corrupt_table_page(database: &[u8]) -> std::result::Result<Vec<u8>, Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(types::new_id("backup-corrupt-page")?);
    std::fs::write(&path, database).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let page: usize = connection
        .query_row(
            "SELECT rootpage FROM sqlite_schema WHERE type='table' AND name='events'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let size: usize = connection
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .unwrap();
    drop(connection);
    std::fs::remove_file(path).unwrap();
    let mut corrupt = database.to_vec();
    let start = (page - 1) * size;
    assert!(start >= size && start < corrupt.len());
    assert_ne!(
        corrupt[start], 0,
        "fixture must corrupt an actual B-tree page"
    );
    corrupt[start] = 0;
    Ok(corrupt)
}

async fn public_restore_case(case: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut header, healthy) = snapshot()?;
    let database = match case {
        "empty" => change_database(&[], "CREATE TABLE removed (x); DROP TABLE removed;")?,
        "partial" => change_database(&healthy, "DROP TABLE events;")?,
        "malformed" => b"authenticated bytes that are not SQLite".to_vec(),
        "corrupt-page" => corrupt_table_page(&healthy)?,
        "missing-column" => change_database(&healthy, "ALTER TABLE events DROP COLUMN json;")?,
        "view" => change_database(&healthy, "DROP TABLE events; CREATE VIEW events AS SELECT '' AS space, 0 AS seq, '' AS hash, 0 AS at_ms, '' AS json WHERE 0;")?,
        "invalid-row" => change_database(&healthy, "DROP TABLE media_keys; CREATE TABLE media_keys (plain_sha TEXT, blob_sha TEXT, key TEXT); INSERT INTO media_keys VALUES ('fixture', NULL, 'fixture');")?,
        "duplicate-row" => change_database(&healthy, "DROP TABLE synced_spaces; CREATE TABLE synced_spaces (space TEXT); INSERT INTO synced_spaces VALUES ('duplicate'), ('duplicate');")?,
        "profile-row-type" => change_database(&healthy, "UPDATE profile_keys SET fields='text instead of blob';")?,
        "synced-row-type" => change_database(&healthy, "UPDATE synced_spaces SET space=x'010203';")?,
        "profile-key-invalid" => change_database(&healthy, "UPDATE profile_keys SET key='invalid profile key';")?,
        "invalid-event" => change_database(&healthy, "UPDATE events SET json='{bad';")?,
        "invalid-signature" => change_database(&healthy, "UPDATE events SET json=json_set(json, '$.sig', '00') WHERE seq=1;")?,
        "indexed-event-drift" => change_database(&healthy, "UPDATE events SET at_ms=at_ms+1;")?,
        "missing-format" => change_database(&healthy, "DELETE FROM meta WHERE key='event_format';")?,
        "missing-root" => change_database(&healthy, &format!("DELETE FROM identities WHERE id='{}';", header.identity))?,
        "agreement-short" => { header.agreement_secret = hex::encode([92; 31]); healthy.clone() },
        "agreement-invalid" => { header.agreement_secret = "not hex".into(); healthy.clone() },
        "root-short" => { header.identity_secret = hex::encode([91; 31]); healthy.clone() },
        "root-mismatch" => { header.identity_secret = hex::encode([91; 32]); healthy.clone() },
        "version" => { header.version += 1; healthy.clone() },
        "healthy" => healthy.clone(),
        "compatible-columns" => change_database(&healthy, "ALTER TABLE identities DROP COLUMN secret; ALTER TABLE profile_keys DROP COLUMN seen_version; ALTER TABLE profile_keys DROP COLUMN fields; ALTER TABLE events ADD COLUMN future_optional TEXT; PRAGMA user_version=3;")?,
        _ => unreachable!(),
    };
    let key = [93; 32];
    let recovery = "7".repeat(64);
    let keys = backup::recovery_keys(&recovery).unwrap();
    let auth = hex::encode(keys.auth);
    let generation = "ab".repeat(32);
    let wrapped = hex::encode(backup::wrap_key(&keys, &header.identity, &key)?);
    let sealed = backup::seal_payload(&key, &header, &database).unwrap();
    // Prove that the deliberately bad database is a valid authenticated payload,
    // not a wrong-key, format-header or HTTP fixture failure.
    if case == "version" {
        assert!(backup::open_payload(&key, &header.identity, &sealed).is_err());
    } else {
        let (opened, bytes) = backup::open_payload(&key, &header.identity, &sealed).unwrap();
        assert_eq!(opened.identity, header.identity);
        assert_eq!(bytes, database);
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let enrolls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::new(Mutex::new(Vec::new()));
    let counted = enrolls.clone();
    let calls = observed.clone();
    let identity = header.identity.clone();
    let served_payload = Arc::new(Mutex::new(sealed));
    let active_payload = served_payload.clone();
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
                    active_payload.lock().unwrap().clone()
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
            recovery.clone(),
            vault.clone(),
        )
        .await;
    let enrollment_count = enrolls.load(Ordering::SeqCst);
    let writes = vault.writes.load(Ordering::SeqCst);
    let unchanged = target.lock().store.memory_image().unwrap() == before;
    eprintln!("authenticated {case} SQLite: accepted={}, enrolls={enrollment_count}, vault_writes={writes}, target_unchanged={unchanged}", result.is_ok());
    assert!(
        observed
            .lock()
            .unwrap()
            .iter()
            .any(|path| path.starts_with("/v1/backup/restore/blob?")),
        "fixture must reach authenticated download"
    );
    if matches!(case, "healthy" | "compatible-columns") {
        assert!(
            result.is_ok(),
            "{case}: valid portable compatibility must remain accepted: {result:?}"
        );
        assert_eq!(enrollment_count, 1);
        assert_eq!(writes, 5);
        assert!(!unchanged);
        assert_eq!(target.account().unwrap().identity_id, header.identity);
        assert!(target.lock().logs.values().flat_map(|log| log.events()).any(|event| matches!(&event.body, types::EventBody::MessagePosted { text, .. } if text == "Confirmed portable content")));
    } else {
        assert!(result.is_err() && enrollment_count == 0 && writes == 0 && unchanged, "{case}: invalid authenticated SQLite must be refused before enrollment, vault writes or target mutation");
        assert_eq!(*vault.values.lock().unwrap(), vault_before);
        assert!(target.account().is_none());
    }
    if case == "partial" {
        // The same real target/vault and authenticated recovery ceremony must
        // remain usable after refusal, with a healthy payload on the same relay.
        *served_payload.lock().unwrap() = backup::seal_payload(&key, &header, &healthy).unwrap();
        let restored = target
            .restore_backup_with_recovery_key(
                format!("http://{address}"),
                "backup_source".into(),
                recovery,
                vault.clone(),
            )
            .await
            .unwrap();
        assert_eq!(restored.identity_id, header.identity);
        assert_eq!(enrolls.load(Ordering::SeqCst), 1);
        assert_eq!(vault.writes.load(Ordering::SeqCst), 5);
        assert!(target.lock().logs.values().flat_map(|log| log.events()).any(|event| matches!(&event.body, types::EventBody::MessagePosted { text, .. } if text == "Confirmed portable content")));
    }
    server.abort();

    Ok(())
}

#[tokio::test]
async fn structural_restore_authenticated_empty_sqlite_is_refused_without_side_effects() {
    public_restore_case("empty").await.unwrap();
}
#[tokio::test]
async fn structural_restore_authenticated_partial_sqlite_is_refused_without_side_effects() {
    public_restore_case("partial").await.unwrap();
}
#[tokio::test]
async fn structural_restore_authenticated_malformed_sqlite_is_refused_without_side_effects() {
    public_restore_case("malformed").await.unwrap();
}

#[tokio::test]
async fn structural_restore_authenticated_short_agreement_secret_is_refused_without_side_effects() {
    public_restore_case("agreement-short").await.unwrap();
}

#[tokio::test]
async fn structural_restore_required_columns_real_tables_and_target_constraints_are_validated() {
    for case in [
        "missing-column",
        "corrupt-page",
        "view",
        "invalid-row",
        "duplicate-row",
    ] {
        public_restore_case(case).await.unwrap();
    }
}
#[tokio::test]
async fn structural_restore_canonical_history_root_and_format_are_validated_before_effects() {
    for case in [
        "invalid-event",
        "invalid-signature",
        "indexed-event-drift",
        "missing-format",
        "missing-root",
    ] {
        public_restore_case(case).await.unwrap();
    }
}
#[tokio::test]
async fn structural_restore_header_boundaries_precede_effects() {
    for case in [
        "agreement-invalid",
        "root-short",
        "root-mismatch",
        "version",
    ] {
        public_restore_case(case).await.unwrap();
    }
}
#[tokio::test]
async fn structural_restore_healthy_and_compatible_optional_columns_remain_accepted() {
    for case in ["healthy", "compatible-columns"] {
        public_restore_case(case).await.unwrap();
    }
}
#[test]
fn structural_restore_direct_rejects_invalid_input_before_target_wipe(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (header, healthy) = snapshot()?;
    let databases = [
        change_database(&[], "CREATE TABLE removed (x); DROP TABLE removed;").unwrap(),
        change_database(&healthy, "DROP TABLE events;").unwrap(),
        b"not SQLite".to_vec(),
    ];
    for database in databases {
        let target = seeded_target();
        let before = target.lock().store.memory_image().unwrap();
        assert!(target
            .lock()
            .restore_snapshot(&header, &database, event_log::Signer::generate().unwrap())
            .is_err());
        assert_eq!(target.lock().store.memory_image().unwrap(), before);
        assert!(target.account().is_none());
    }
    Ok(())
}

#[test]
fn structural_restore_direct_requires_every_portable_table_before_wipe(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (header, healthy) = snapshot()?;
    for table in [
        "identities",
        "events",
        "meta",
        "profile_keys",
        "peer_agreement_keys",
        "blocked",
        "synced_spaces",
        "media_keys",
    ] {
        let database = change_database(&healthy, &format!("DROP TABLE {table};")).unwrap();
        let target = seeded_target();
        let before = target.lock().store.memory_image().unwrap();
        assert!(
            target
                .lock()
                .restore_snapshot(&header, &database, event_log::Signer::generate().unwrap())
                .is_err(),
            "missing portable table {table}"
        );
        assert_eq!(
            target.lock().store.memory_image().unwrap(),
            before,
            "missing {table} must not erase the target"
        );
        assert!(target.account().is_none());
    }
    Ok(())
}

#[tokio::test]
async fn structural_restore_portable_reader_profile_type_is_validated_before_effects() {
    public_restore_case("profile-row-type").await.unwrap();
}
#[tokio::test]
async fn structural_restore_portable_reader_synced_type_is_validated_before_effects() {
    public_restore_case("synced-row-type").await.unwrap();
}
#[tokio::test]
async fn structural_restore_portable_reader_profile_key_is_validated_before_effects() {
    public_restore_case("profile-key-invalid").await.unwrap();
}

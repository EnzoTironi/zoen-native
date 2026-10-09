//! Encrypted server backup (ADR 0046): Ana turns on a password backup, loses her phone and
//! gets her account and history back on a new one; the relay never holds anything readable;
//! ten wrong passwords lock a backup for good; a recovery key works without the vault.
//!
//!   ZOEN_TEST_PG=postgres://zoen@127.0.0.1:15433/postgres cargo test -p zoen-cli --test journey_backup

#[path = "common/backup_store.rs"]
mod backup_store;
mod common;
use common::*;
use roda_types::EventBody;

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

fn device_key(w: &World, who: &str) -> roda_log::Signer {
    let bytes = std::fs::read(w.dir.join(who).join("vault/zoen.device.v1")).unwrap();
    roda_log::Signer::from_secret(&bytes.try_into().unwrap())
}

fn signed_backup_request(
    w: &World,
    key: &roda_log::Signer,
    identity: &str,
    method: &str,
    op: &str,
    path: &str,
    body: &[u8],
) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let ts = now_ms();
    let relay = format!("127.0.0.1:{}", w.port);
    let sig = key.sign(&roda_proto::backup_message(
        &relay,
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
    key: &roda_log::Signer,
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

#[tokio::test]
async fn restore_enrollment_requires_current_recovery_authority_and_never_revives_revoked_keys() {
    use roda_log::{Author, Signer};
    use sha2::Sha256;

    let w = world("backup_enrollment").await;
    w.init("ana", "Ana");
    let root_secret = std::fs::read(w.dir.join("ana/vault/zoen.identity.v1")).unwrap();
    let root = Signer::from_secret(&root_secret.try_into().unwrap());
    let out = w.zoen("ana", &["backup", "on", "--recovery-key"]);
    let secret = out
        .lines()
        .find_map(|line| line.strip_prefix("recovery-key\t"))
        .expect("recovery key");
    let digits: String = secret.chars().filter(|c| c.is_ascii_digit()).collect();
    let mut auth = [0u8; 32];
    hkdf::Hkdf::<Sha256>::new(Some(b"zoen-backup-recovery-key-v1"), digits.as_bytes())
        .expand(b"zoen-backup-auth-v1", &mut auth)
        .unwrap();
    let auth = hex::encode(auth);
    let generation = w.scalar("SELECT generation FROM backup_vaults").await;
    let device = Author::device(&root, Signer::generate());
    let cert = device.cert.as_deref().unwrap();
    let request = serde_json::json!({
        "handle": "ana", "auth_key": auth, "generation": generation,
        "device": device.key.id(), "cert": cert,
        "sig": device.key.sign(&roda_proto::backup_enroll_message(
            &root.id(), &device.key.id(), cert, &generation,
        )),
    });
    let registered = format!(
        "SELECT count(*) FROM devices WHERE device = '{}'",
        device.key.id()
    );
    for (field, value, expected) in [
        ("auth_key", "00".repeat(32), 403),
        ("generation", "ee".repeat(32), 409),
        ("sig", "00".repeat(64), 403),
    ] {
        let mut wrong = request.clone();
        wrong[field] = value.into();
        let (status, body) = restore_post(&w, "/v1/backup/restore/enroll", wrong).await;
        assert_eq!(status, expected, "{field}: {body:?}");
        assert_eq!(w.count(&registered).await, 0);
    }
    let mut wrong_certificate = request.clone();
    let invalid_cert = "00".repeat(64);
    wrong_certificate["cert"] = invalid_cert.clone().into();
    wrong_certificate["sig"] = device
        .key
        .sign(&roda_proto::backup_enroll_message(
            &root.id(),
            &device.key.id(),
            &invalid_cert,
            &generation,
        ))
        .into();
    assert_eq!(
        restore_post(&w, "/v1/backup/restore/enroll", wrong_certificate)
            .await
            .0,
        403
    );
    assert_eq!(w.count(&registered).await, 0);
    for _ in 0..2 {
        assert_eq!(
            restore_post(&w, "/v1/backup/restore/enroll", request.clone())
                .await
                .0,
            204,
            "enrollment retries keep one active device",
        );
        assert_eq!(w.count(&registered).await, 1);
    }
    let socket =
        RawClient::reconnect(&format!("http://127.0.0.1:{}", w.port), device.clone()).await;
    drop(socket);
    w.zoen("ana", &["unlink", &device.key.id()]);
    assert_eq!(
        restore_post(&w, "/v1/backup/restore/enroll", request.clone())
            .await
            .0,
        403,
        "replaying recovery cannot reactivate the revoked key",
    );
    w.zoen("ana", &["backup", "on", "--recovery-key"]);
    let next = Author::device(&root, Signer::generate());
    let cert = next.cert.as_deref().unwrap();
    let stale = serde_json::json!({
        "handle": "ana", "auth_key": auth, "generation": generation,
        "device": next.key.id(), "cert": cert,
        "sig": next.key.sign(&roda_proto::backup_enroll_message(
            &root.id(), &next.key.id(), cert, &generation,
        )),
    });
    assert_eq!(
        restore_post(&w, "/v1/backup/restore/enroll", stale).await.0,
        403
    );
    assert_eq!(
        w.count(&format!(
            "SELECT count(*) FROM devices WHERE device = '{}'",
            next.key.id()
        ))
        .await,
        0,
        "rotating recovery authority invalidates its previous enrollment proof"
    );
}

fn link_notebook(w: &World) {
    use std::io::BufRead;
    let mut child = w.spawn_zoen("ana-notebook", &["link-request", "--for", "120"]);
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
        {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let code = loop {
        let line = rx
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .unwrap();
        if let Some(code) = line.strip_prefix("code:") {
            break code.trim().to_string();
        }
    };
    w.zoen("ana", &["link", &code]);
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    w.sync_until("ana-notebook", |s| s.contains("pending=0"));
}

#[tokio::test]
async fn replayed_uploads_use_distinct_objects_and_survive_delayed_cleanup() {
    let w = world("backup_replay").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    w.zoen("ana", &["group", "Casa", "@bruno", "--readable"]);
    let message = "história preservada depois de repetir o upload";
    w.zoen("ana", &["send", "Casa", message]);
    w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    let identity = w.id_of("ana").await;
    let key = device_key(&w, "ana");
    let generation = w.scalar("SELECT generation FROM backup_vaults").await;
    let sha = w.scalar("SELECT blob_sha FROM backup_vaults").await;
    let original_key = w.scalar("SELECT blob_key FROM backup_vaults").await;
    let sealed = std::fs::read(w.dir.join("blobs").join(original_key)).unwrap();
    let body = [
        roda_proto::BACKUP_UPLOAD_MAGIC.as_slice(),
        hex::decode(&generation).unwrap().as_slice(),
        sealed.as_slice(),
    ]
    .concat();
    let request =
        signed_backup_request(&w, &key, &identity, "PUT", "blob", "/v1/backup/blob", &body);
    assert_eq!(backup_http(&w, &request).await.0, 201);
    let first_key = w.scalar("SELECT blob_key FROM backup_vaults").await;
    assert_eq!(backup_http(&w, &request).await.0, 201);
    let second_key = w.scalar("SELECT blob_key FROM backup_vaults").await;
    assert_ne!(first_key, second_key, "a replay reused an object path");
    assert_eq!(w.scalar("SELECT blob_sha FROM backup_vaults").await, sha);
    assert_eq!(
        w.scalar("SELECT generation FROM backup_vaults").await,
        generation
    );
    // A cloud DELETE issued for the first object may complete after the replay.
    match std::fs::remove_file(w.dir.join("blobs").join(first_key)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => panic!("delayed cleanup failed: {e}"),
    }
    assert_eq!(
        std::fs::read(w.dir.join("blobs").join(second_key)).unwrap(),
        sealed
    );
    assert!(w
        .zoen("ana-new", &["recover", "@ana", "--password", PASSWORD])
        .contains("restored"));
    assert!(w.zoen("ana-new", &["read", "Casa"]).contains(message));
    assert!(!w.zoen("ana-new", &["verify"]).contains("BROKEN"));
}

#[tokio::test]
async fn password_restore_metadata_does_not_identify_decoys() {
    use sha2::{Digest, Sha256};
    // Standard compressed Ristretto255 basepoint: also a valid blinded input.
    const BASEPOINT: &str = "e2f2ae0a6abc4e71a884a961c500515f58e30b6aa582dd8db6a65945e08d2d76";
    let w = world("backup_decoy_metadata").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    let mut responses = Vec::new();
    for handle in ["ana", "bruno", "carla"] {
        let request = serde_json::json!({"handle": handle, "blinded": BASEPOINT});
        let (status, body) = restore_post(&w, "/v1/backup/restore/start", request.clone()).await;
        assert_eq!(status, 200, "{handle}: {}", String::from_utf8_lossy(&body));
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let (status, body) = restore_post(&w, "/v1/backup/restore/start", request).await;
        assert_eq!(status, 200, "{handle}: {}", String::from_utf8_lossy(&body));
        let repeated: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response, repeated, "metadata changed for {handle}");
        assert_eq!(response["mode"], "passphrase");
        let generation = response["generation"].as_str().expect("opaque generation");
        assert_eq!(hex::decode(generation).unwrap().len(), 32);
        let identity = response["identity"].as_str().unwrap();
        assert_eq!(hex::decode(identity).unwrap().len(), 32);
        let evaluated = hex::decode(response["evaluated"].as_str().unwrap()).unwrap();
        assert_eq!(evaluated.len(), 32);
        // OPRF evaluations are public. Hashing the evaluation at a chosen basepoint
        // would make a decoy's metadata recognizable without knowing any secret.
        let predictable_generation = hex::encode(
            Sha256::new()
                .chain_update(b"zoen-backup-decoy-generation")
                .chain_update(&evaluated)
                .finalize(),
        );
        assert_ne!(generation, predictable_generation, "{handle}");
        let predictable_identity = hex::encode(
            Sha256::new()
                .chain_update(b"zoen-backup-decoy-identity")
                .chain_update(&evaluated)
                .finalize(),
        );
        assert_ne!(identity, predictable_identity, "{handle}");
        responses.push(response);
    }
    assert_eq!(responses[0]["identity"], w.id_of("ana").await);
    assert_eq!(responses[1]["identity"], w.id_of("bruno").await);
    assert_eq!(responses[0]["kdf"], responses[1]["kdf"]);
    assert_eq!(responses[0]["kdf"], responses[2]["kdf"]);
    assert_ne!(responses[0]["generation"], responses[1]["generation"]);
    assert_ne!(responses[1]["generation"], responses[2]["generation"]);
}

#[tokio::test]
async fn wrong_restore_auth_and_generation_do_not_reveal_backup_existence() {
    let w = world("backup_decoy_authorization").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    let auth = "55".repeat(32);
    let generation = "66".repeat(32);
    let mut expected_open = None;
    let mut expected_blob = None;
    for handle in ["ana", "bruno", "carla"] {
        let response = restore_post(
            &w,
            "/v1/backup/restore/open",
            serde_json::json!({"handle": handle, "auth_key": auth, "generation": generation}),
        )
        .await;
        assert_eq!(response.0, 403, "open revealed {handle}: {response:?}");
        if let Some(expected) = &expected_open {
            assert_eq!(&response, expected, "open revealed {handle}");
        } else {
            expected_open = Some(response);
        }
        let request = format!(
            "GET /v1/backup/restore/blob?handle={handle}&generation={generation} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\nx-zoen-backup-auth: {auth}\r\n\r\n",
            w.port,
        );
        let response = backup_http(&w, request.as_bytes()).await;
        assert_eq!(response.0, 403, "blob revealed {handle}: {response:?}");
        if let Some(expected) = &expected_blob {
            assert_eq!(&response, expected, "blob revealed {handle}");
        } else {
            expected_blob = Some(response);
        }
    }
}

#[tokio::test]
async fn linked_devices_cannot_overwrite_a_new_backup_with_an_old_key() {
    let w = world("backup_generation").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    w.zoen("ana", &["group", "Casa", "@bruno", "--readable"]);
    w.zoen("ana", &["send", "Casa", "história antes do notebook"]);
    w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    let first = w.scalar("SELECT generation FROM backup_vaults").await;
    link_notebook(&w);
    assert!(!w.dir.join("ana-notebook/vault/zoen.backup.v1").exists());
    w.zoen("ana-notebook", &["send", "Casa", "cópia nova no notebook"]);
    w.zoen(
        "ana-notebook",
        &["backup", "on", "--password", "senha nova do notebook"],
    );
    let second = w.scalar("SELECT generation FROM backup_vaults").await;
    assert_ne!(first, second);
    let sha = w.scalar("SELECT blob_sha FROM backup_vaults").await;
    let refused = w.try_zoen("ana", &["backup", "now"]).unwrap_err();
    assert!(refused.contains("configuration changed"), "{refused}");
    assert_eq!(w.scalar("SELECT blob_sha FROM backup_vaults").await, sha);
    assert_eq!(w.count("SELECT count(*) FROM backup_setups").await, 0);
    w.zoen(
        "ana-new",
        &["recover", "@ana", "--password", "senha nova do notebook"],
    );
    let history = w.zoen("ana-new", &["read", "Casa"]);
    assert!(
        history.contains("história antes do notebook")
            && history.contains("cópia nova no notebook"),
        "{history}"
    );
    assert!(!w.zoen("ana-new", &["verify"]).contains("BROKEN"));
}

#[tokio::test]
async fn password_backups_require_explicit_dev_opt_in_and_preserve_existing_data() {
    let mut w = world("backup_password_gate").await;
    w.init("ana", "Ana");
    w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    let identity = w.id_of("ana").await;
    let signer = device_key(&w, "ana");
    let generation = w.scalar("SELECT generation FROM backup_vaults").await;
    let key = w.scalar("SELECT blob_key FROM backup_vaults").await;
    let oprf_key = w
        .scalar("SELECT encode(oprf_key, 'hex') FROM backup_vaults")
        .await;
    let status = w.zoen("ana", &["backup", "status"]);
    let sealed = std::fs::read(w.dir.join("blobs").join(&key)).unwrap();
    assert_eq!(
        w.count(
            "WITH changed AS (UPDATE backup_vaults SET guesses = 3 RETURNING identity)
             SELECT count(*) FROM changed"
        )
        .await,
        1
    );
    w.stop_relay();
    // A vault key alone must never activate the destructive public ceremony.
    w.configure_relay_env("ZOEN_DEV_ALLOW_UNAUTHENTICATED_PASSWORD_BACKUP", None);
    w.start_relay();
    for _ in 0..12 {
        assert_eq!(
            restore_post(
                &w,
                "/v1/backup/restore/start",
                serde_json::json!({"handle": "ana", "blinded": "01".repeat(32)}),
            )
            .await
            .0,
            503
        );
    }
    assert_eq!(
        restore_post(
            &w,
            "/v1/backup/restore/open",
            serde_json::json!({"handle": "ana", "auth_key": "22".repeat(32), "generation": generation}),
        )
        .await
        .0,
        503
    );
    let request = format!(
        "GET /v1/backup/restore/blob?handle=ana&generation={generation} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\nx-zoen-backup-auth: {}\r\n\r\n",
        w.port, "22".repeat(32),
    );
    assert_eq!(backup_http(&w, request.as_bytes()).await.0, 503);
    for (method, op, path, body) in [
        (
            "POST",
            "oprf",
            "/v1/backup/oprf",
            serde_json::json!({"blinded": "01".repeat(32), "generation": "11".repeat(32)})
                .to_string()
                .into_bytes(),
        ),
        (
            "PUT",
            "vault",
            "/v1/backup/vault",
            serde_json::json!({
                "generation": "11".repeat(32), "mode": "passphrase",
                "verifier": "22".repeat(32), "wrapped_key": "33".repeat(72),
                "kdf": {"alg": "argon2id", "m_kib": 65536, "t": 3, "p": 1, "v": 1},
            })
            .to_string()
            .into_bytes(),
        ),
        (
            "PUT",
            "blob",
            "/v1/backup/blob",
            [
                roda_proto::BACKUP_UPLOAD_MAGIC.as_slice(),
                hex::decode(&generation).unwrap().as_slice(),
                sealed.as_slice(),
            ]
            .concat(),
        ),
    ] {
        assert_eq!(
            signed_backup_write(&w, &signer, &identity, method, op, path, &body).await,
            503,
            "{op} bypassed the password activation gate"
        );
    }
    let refused = w
        .try_zoen(
            "ana",
            &["backup", "on", "--password", "another good password"],
        )
        .unwrap_err();
    assert!(refused.contains("Password backup"), "{refused}");
    assert_eq!(w.zoen("ana", &["backup", "status"]), status);
    assert_eq!(
        w.scalar("SELECT generation FROM backup_vaults").await,
        generation
    );
    assert_eq!(w.scalar("SELECT blob_key FROM backup_vaults").await, key);
    assert_eq!(
        w.scalar("SELECT encode(oprf_key, 'hex') FROM backup_vaults")
            .await,
        oprf_key
    );
    assert_eq!(
        w.count("SELECT guesses::bigint FROM backup_vaults").await,
        3
    );
    assert_eq!(w.count("SELECT count(*) FROM backup_pending").await, 0);
    assert_eq!(w.count("SELECT count(*) FROM backup_setups").await, 0);
    assert_eq!(
        std::fs::read(w.dir.join("blobs").join(key)).unwrap(),
        sealed
    );
    w.stop_relay();
    w.configure_relay_env("ZOEN_DEV_ALLOW_UNAUTHENTICATED_PASSWORD_BACKUP", Some("1"));
    w.start_relay();
    assert!(w
        .zoen("ana-new", &["recover", "@ana", "--password", PASSWORD])
        .contains("restored"));
}

#[tokio::test]
async fn backup_database_lock_waits_are_bounded() {
    use sqlx::Connection;
    let w = World::with_env(
        "backup_db_timeouts",
        &[
            ("ZOEN_BACKUP_VAULT_KEY", VAULT_KEY),
            ("ZOEN_DEV_ALLOW_UNAUTHENTICATED_PASSWORD_BACKUP", "1"),
            ("ZOEN_BACKUP_STORAGE_TIMEOUT_MS", "250"),
            ("ZOEN_BACKUP_REQUEST_TIMEOUT_MS", "1500"),
            ("ZOEN_BACKUP_LOCK_TIMEOUT_MS", "100"),
        ],
    )
    .await;
    w.init("ana", "Ana");
    w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    let key = w.scalar("SELECT blob_key FROM backup_vaults").await;
    let mut db = sqlx::postgres::PgConnection::connect(&w.db_url)
        .await
        .unwrap();
    let mut lock = db.begin().await.unwrap();
    sqlx::query("SELECT identity FROM backup_vaults FOR UPDATE")
        .execute(&mut *lock)
        .await
        .unwrap();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        restore_post(
            &w,
            "/v1/backup/restore/start",
            serde_json::json!({"handle": "ana", "blinded": "01".repeat(32)}),
        ),
    )
    .await
    .expect("backup request remained blocked by another transaction");
    assert_eq!(response.0, 503);
    lock.rollback().await.unwrap();
    let partial = format!(
        "POST /v1/backup/restore/start HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 1024\r\n\r\n",
        w.port,
    );
    let started = std::time::Instant::now();
    let response = backup_http(&w, partial.as_bytes()).await;
    assert_eq!(response.0, 503);
    assert!(String::from_utf8_lossy(&response.1).contains("backup operation timed out"));
    assert!(started.elapsed() < std::time::Duration::from_millis(2500));
    assert_eq!(w.scalar("SELECT blob_key FROM backup_vaults").await, key);
    assert_eq!(
        w.count("SELECT guesses::bigint FROM backup_vaults").await,
        0
    );
    assert!(w
        .zoen("ana-new", &["recover", "@ana", "--password", PASSWORD])
        .contains("restored"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stalled_backup_storage_is_bounded_and_does_not_block_unlink() {
    use std::time::{Duration, Instant};
    let store = backup_store::MockS3::new();
    let w = World::with_env(
        "backup_store_timeouts",
        &[
            ("ZOEN_S3_BUCKET", "backup-test"),
            ("AWS_ENDPOINT_URL_S3", &store.endpoint),
            ("AWS_ACCESS_KEY_ID", "mock-access-key"),
            ("AWS_SECRET_ACCESS_KEY", "mock-secret-key"),
            ("AWS_REGION", "us-east-1"),
            ("AWS_ALLOW_HTTP", "true"),
            ("AWS_DISABLE_BULK_DELETE", "true"),
            ("ZOEN_BACKUP_STORAGE_TIMEOUT_MS", "350"),
            ("ZOEN_BACKUP_REQUEST_TIMEOUT_MS", "3000"),
            ("ZOEN_BACKUP_LOCK_TIMEOUT_MS", "350"),
        ],
    )
    .await;
    w.init("ana", "Ana");
    let out = w.zoen("ana", &["backup", "on", "--recovery-key"]);
    let recovery_key = out
        .lines()
        .find_map(|line| line.strip_prefix("recovery-key\t"))
        .expect("recovery key");
    w.zoen(
        "ana-new",
        &["recover", "@ana", "--recovery-key", recovery_key],
    );
    let identity = w.id_of("ana").await;
    let signer = device_key(&w, "ana");
    let generation = w.scalar("SELECT generation FROM backup_vaults").await;
    let original_key = w.scalar("SELECT blob_key FROM backup_vaults").await;
    let sha = w.scalar("SELECT blob_sha FROM backup_vaults").await;
    let original = store.bytes(&original_key);
    let body = [
        roda_proto::BACKUP_UPLOAD_MAGIC.as_slice(),
        hex::decode(&generation).unwrap().as_slice(),
        original.as_slice(),
    ]
    .concat();
    let request = signed_backup_request(
        &w,
        &signer,
        &identity,
        "PUT",
        "blob",
        "/v1/backup/blob",
        &body,
    );
    store.stall(Some("PUT"));
    let after = store.call_count();
    let started = Instant::now();
    let (response, orphan_key) = tokio::join!(backup_http(&w, &request), async {
        let orphan_key = store.wait_for_call("PUT", after).await;
        // The signed upload now holds FOR SHARE on Ana's original device. Unlink must
        // complete even though the object store will not acknowledge the upload.
        let mut child = w.spawn_zoen("ana-new", &["unlink", &signer.id()]);
        let expires = Instant::now() + Duration::from_secs(2);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= expires {
                let _ = child.kill();
                let _ = child.wait();
                panic!("stalled backup prevented Unlink");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        orphan_key
    });
    assert_eq!(response.0, 503);
    assert!(started.elapsed() < Duration::from_millis(2500));
    assert_eq!(
        w.scalar("SELECT blob_key FROM backup_vaults").await,
        original_key
    );
    assert_eq!(w.scalar("SELECT blob_sha FROM backup_vaults").await, sha);
    assert_ne!(orphan_key, original_key);
    store.stall(None);
    store.wait_for_completion("PUT", &orphan_key).await;
    assert_eq!(store.bytes(&orphan_key), original);
    assert_eq!(
        w.scalar("SELECT blob_key FROM backup_vaults").await,
        original_key
    );
    assert!(w
        .zoen(
            "ana-third",
            &["recover", "@ana", "--recovery-key", recovery_key]
        )
        .contains("restored"));

    // Headers arrive, but the complete object body must share the same storage deadline.
    store.stall(Some("GET_BODY"));
    let after = store.call_count();
    let started = Instant::now();
    let refused = w
        .try_zoen(
            "ana-fourth",
            &["recover", "@ana", "--recovery-key", recovery_key],
        )
        .unwrap_err();
    assert!(refused.contains("storage unavailable"), "{refused}");
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(store.wait_for_call("GET", after).await, original_key);
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(1),
            w.count(
                "WITH changed AS (UPDATE backup_vaults SET guesses = guesses RETURNING identity)
                 SELECT count(*) FROM changed"
            ),
        )
        .await
        .expect("timed-out restore retained its row lock"),
        1
    );
    store.stall(None);

    // Cleanup failure must neither hold authorization locks nor delay an acknowledged
    // new upload indefinitely. Its late DELETE only targets the retired unique key.
    store.stall(Some("DELETE"));
    let after = store.call_count();
    let started = Instant::now();
    assert!(w
        .zoen("ana-new", &["backup", "now"])
        .starts_with("backup on"));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(store.wait_for_call("DELETE", after).await, original_key);
    let latest_key = w.scalar("SELECT blob_key FROM backup_vaults").await;
    assert_ne!(latest_key, original_key);
    let latest = store.bytes(&latest_key);
    store.stall(None);
    store.wait_for_completion("DELETE", &original_key).await;
    assert_eq!(store.bytes(&latest_key), latest);
    assert!(w
        .zoen(
            "ana-fourth",
            &["recover", "@ana", "--recovery-key", recovery_key]
        )
        .contains("restored"));

    store.stall(Some("DELETE"));
    let after = store.call_count();
    let started = Instant::now();
    w.zoen("ana-new", &["backup", "off"]);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(store.wait_for_call("DELETE", after).await, latest_key);
    assert_eq!(w.count("SELECT count(*) FROM backup_vaults").await, 0);
    store.stall(None);
    store.wait_for_completion("DELETE", &latest_key).await;
}

#[tokio::test]
async fn interrupted_backup_setup_keeps_the_previous_copy_restorable() {
    let w = world("backup_atomic").await;
    w.init("ana", "Ana");
    w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    let identity = w.id_of("ana").await;
    let key = device_key(&w, "ana");
    let generation = w.scalar("SELECT generation FROM backup_vaults").await;
    let sha = w.scalar("SELECT blob_sha FROM backup_vaults").await;
    let setup = serde_json::json!({
        "generation": "11".repeat(32), "mode": "recovery_key",
        "verifier": "22".repeat(32), "wrapped_key": "33".repeat(72),
        "kdf": {"alg": "hkdf-sha256", "v": 1},
    });
    assert_eq!(
        signed_backup_write(
            &w,
            &key,
            &identity,
            "PUT",
            "vault",
            "/v1/backup/vault",
            setup.to_string().as_bytes()
        )
        .await,
        204
    );
    assert_eq!(w.count("SELECT count(*) FROM backup_setups").await, 1);
    assert_eq!(
        w.scalar("SELECT generation FROM backup_vaults").await,
        generation
    );
    assert_eq!(w.scalar("SELECT blob_sha FROM backup_vaults").await, sha);
    assert!(w
        .zoen("ana-new", &["recover", "@ana", "--password", PASSWORD])
        .contains("restored"));
}

#[tokio::test]
async fn revoked_devices_are_refused_by_every_backup_write_endpoint() {
    let w = world("backup_revoked").await;
    w.init("ana", "Ana");
    w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    let identity = w.id_of("ana").await;
    let key = device_key(&w, "ana");
    let sha = w.scalar("SELECT blob_sha FROM backup_vaults").await;
    w.zoen("ana-new", &["recover", "@ana", "--password", PASSWORD]);
    w.zoen("ana-new", &["unlink", &key.id()]);
    for (method, op, path) in [
        ("POST", "oprf", "/v1/backup/oprf"),
        ("PUT", "vault", "/v1/backup/vault"),
        ("PUT", "blob", "/v1/backup/blob"),
        ("DELETE", "delete", "/v1/backup"),
    ] {
        assert_eq!(
            signed_backup_write(&w, &key, &identity, method, op, path, b"{}").await,
            403,
            "{op}"
        );
    }
    assert_eq!(w.scalar("SELECT blob_sha FROM backup_vaults").await, sha);
    assert_eq!(w.count("SELECT count(*) FROM backup_setups").await, 0);
    assert!(w
        .zoen("ana-third", &["recover", "@ana", "--password", PASSWORD])
        .contains("restored"));
}

#[tokio::test]
async fn a_pre_generation_backup_still_restores_after_upgrade() {
    use sqlx::Connection;
    let mut w = world("backup_legacy").await;
    w.init("ana", "Ana");
    w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    let identity = w.id_of("ana").await;
    let legacy_path = w.dir.join("blobs/backups").join(&identity);
    let stored_key = w.scalar("SELECT blob_key FROM backup_vaults").await;
    let generated_path = w.dir.join("blobs/backups/v2").join(&identity);
    let object = std::fs::read(w.dir.join("blobs").join(stored_key)).unwrap();
    // The previous version stored one object directly at backups/{identity}.
    std::fs::remove_dir_all(&generated_path).unwrap();
    std::fs::write(&legacy_path, object).unwrap();
    w.stop_relay();
    let mut db = sqlx::postgres::PgConnection::connect(&w.db_url)
        .await
        .unwrap();
    sqlx::raw_sql(
        "DELETE FROM _sqlx_migrations WHERE version IN (21, 23);
         DROP TABLE backup_setups;
         ALTER TABLE backup_vaults DROP COLUMN generation, DROP COLUMN blob_key;
         ALTER TABLE backup_pending DROP COLUMN generation, DROP COLUMN device;",
    )
    .execute(&mut db)
    .await
    .unwrap();
    drop(db);
    w.start_relay();
    assert_eq!(
        w.scalar("SELECT generation FROM backup_vaults").await.len(),
        64
    );
    assert!(w
        .zoen("ana-new", &["recover", "@ana", "--password", PASSWORD])
        .contains("restored"));
    // The verified wrapper binds the restored key to its migrated generation.
    assert!(w
        .zoen("ana-new", &["backup", "now"])
        .starts_with("backup on"));
    assert!(w
        .zoen("ana-new", &["backup", "on", "--password", PASSWORD])
        .starts_with("backup on"));
    assert!(w
        .zoen("ana-third", &["recover", "@ana", "--password", PASSWORD])
        .contains("restored"));
}

#[tokio::test]
async fn a_generation_backup_still_restores_after_object_version_upgrade() {
    use sqlx::Connection;
    let mut w = world("backup_v2_upgrade").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    w.zoen("ana", &["group", "Casa", "@bruno", "--readable"]);
    let message = "história antes de migrar os objetos do backup";
    w.zoen("ana", &["send", "Casa", message]);
    w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    let identity = w.id_of("ana").await;
    let generation = w.scalar("SELECT generation FROM backup_vaults").await;
    let sha = w.scalar("SELECT blob_sha FROM backup_vaults").await;
    let current_key = w.scalar("SELECT blob_key FROM backup_vaults").await;
    let old_key = format!("backups/v2/{identity}/{sha}");
    w.stop_relay();
    // Version 0021 named generation-bearing objects by their ciphertext digest.
    std::fs::rename(
        w.dir.join("blobs").join(current_key),
        w.dir.join("blobs").join(&old_key),
    )
    .unwrap();
    let mut db = sqlx::postgres::PgConnection::connect(&w.db_url)
        .await
        .unwrap();
    sqlx::raw_sql(
        "DELETE FROM _sqlx_migrations WHERE version = 23;
         ALTER TABLE backup_vaults DROP COLUMN blob_key;",
    )
    .execute(&mut db)
    .await
    .unwrap();
    drop(db);
    w.start_relay();
    assert_eq!(
        w.scalar("SELECT blob_key FROM backup_vaults").await,
        old_key
    );
    assert_eq!(
        w.scalar("SELECT generation FROM backup_vaults").await,
        generation
    );
    assert!(w
        .zoen("ana-new", &["recover", "@ana", "--password", PASSWORD])
        .contains("restored"));
    assert!(w.zoen("ana-new", &["read", "Casa"]).contains(message));
    assert!(w
        .zoen("ana-new", &["backup", "now"])
        .starts_with("backup on"));
    assert_ne!(
        w.scalar("SELECT blob_key FROM backup_vaults").await,
        old_key
    );
    assert!(w
        .zoen("ana-third", &["recover", "@ana", "--password", PASSWORD])
        .contains("restored"));
    assert!(w.zoen("ana-third", &["read", "Casa"]).contains(message));
}

#[tokio::test]
async fn a_lost_phone_comes_back_from_a_password_backup() {
    let w = world("backup").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    w.zoen(
        "ana",
        &["dm", "@bruno", "oi Bruno, guarda o mapa da trilha"],
    );
    w.zoen("bruno", &["send", "@ana", "guardei!"]);
    w.zoen("ana", &["group", "Trilha", "@bruno", "--readable"]);
    w.zoen("ana", &["send", "Trilha", "sábado 7h no portão"]);
    w.zoen("ana", &["sync"]);

    // A password that's too short is refused before anything leaves the phone.
    let short = w
        .try_zoen("ana", &["backup", "on", "--password", "curta"])
        .unwrap_err();
    assert!(short.contains("8"), "{short}");

    let on = w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    assert!(on.starts_with("backup on mode=password"), "{on}");
    assert_eq!(w.zoen("ana", &["backup", "status"]).trim(), on.trim());

    // The relay holds a vault and a sealed object: no message, name, key or password.
    assert_eq!(
        w.count("SELECT count(*) FROM backup_vaults WHERE mode = 'passphrase' AND oprf_key IS NOT NULL AND NOT locked").await,
        1
    );
    let files = backup_files(&w);
    assert_eq!(files.len(), 1);
    for secret in [
        "guarda o mapa",
        "guardei",
        "sábado 7h",
        "Trilha",
        "Ana",
        PASSWORD,
    ] {
        assert!(!contains(&files[0], secret), "backup leaks {secret:?}");
    }
    assert_eq!(
        w.count(&format!(
            "SELECT count(*) FROM backup_vaults WHERE kdf::text LIKE '%{PASSWORD}%' OR encode(wrapped_key, 'escape') LIKE '%{PASSWORD}%'"
        ))
        .await,
        0
    );

    // The phone is gone. On a new one, a wrong password is refused...
    let wrong = w
        .try_zoen(
            "ana-new",
            &["recover", "@ana", "--password", "trilha no domingo"],
        )
        .unwrap_err();
    assert!(wrong.contains("Wrong password"), "{wrong}");
    // ...and a backup that doesn't exist looks exactly the same (no way to probe for one).
    let nobody = w
        .try_zoen("nobody", &["recover", "@carla", "--password", PASSWORD])
        .unwrap_err();
    assert!(nobody.contains("Wrong password"), "{nobody}");

    // The right password brings the account back, as a new device.
    let restored = w.zoen("ana-new", &["recover", "@ana", "--password", PASSWORD]);
    assert!(restored.starts_with("@ana ("), "{restored}");
    assert!(restored.contains("connection=online"), "{restored}");
    assert_eq!(
        w.count("SELECT count(*) FROM devices d JOIN identities i ON i.id = d.identity WHERE i.handle = 'ana'").await,
        2,
        "a new device of the same account"
    );
    assert_eq!(
        w.count("SELECT guesses::bigint FROM backup_vaults").await,
        0,
        "a success resets the guesses"
    );

    // History is there, including the end-to-end chat the relay can't read.
    let dm = w.zoen("ana-new", &["read", "@bruno"]);
    assert!(
        dm.contains("Ana: oi Bruno, guarda o mapa da trilha"),
        "{dm}"
    );
    assert!(dm.contains("Bruno: guardei!"), "{dm}");
    assert!(w
        .zoen("ana-new", &["read", "Trilha"])
        .contains("Ana: sábado 7h no portão"));
    assert!(w.zoen("ana-new", &["whoami"]).starts_with("@ana Ana"));

    // And the conversation goes on from the new phone.
    w.zoen("ana-new", &["send", "Trilha", "voltei, celular novo"]);
    assert!(w
        .zoen("bruno", &["read", "Trilha"])
        .contains("Ana: voltei, celular novo"));
    w.zoen("bruno", &["send", "Trilha", "bem-vinda de volta"]);
    assert!(w
        .zoen("ana-new", &["read", "Trilha"])
        .contains("Bruno: bem-vinda de volta"));

    // The new phone keeps backing up with the same key, and can turn it off.
    assert!(w
        .zoen("ana-new", &["backup", "now"])
        .starts_with("backup on mode=password"));
    w.zoen("ana-new", &["backup", "off"]);
    assert_eq!(w.count("SELECT count(*) FROM backup_vaults").await, 0);
    assert!(backup_files(&w).is_empty());
}

#[tokio::test]
async fn ten_wrong_passwords_lock_a_backup_for_good() {
    let w = world("backup_lock").await;
    w.init("carla", "Carla");
    w.zoen("carla", &["backup", "on", "--password", PASSWORD]);
    for i in 0..9 {
        let e = w
            .try_zoen(
                "thief",
                &["recover", "@carla", "--password", &format!("chute {i}")],
            )
            .unwrap_err();
        assert!(e.contains("Wrong password"), "guess {i}: {e}");
    }
    let tenth = w
        .try_zoen("thief", &["recover", "@carla", "--password", "chute 9"])
        .unwrap_err();
    assert!(
        tenth.contains("Wrong password") || tenth.contains("can no longer"),
        "{tenth}"
    );
    // Now even the right password can't open it: the vault destroyed the key.
    let late = w
        .try_zoen("carla-new", &["recover", "@carla", "--password", PASSWORD])
        .unwrap_err();
    assert!(late.contains("can no longer be opened"), "{late}");
    assert_eq!(
        w.count("SELECT count(*) FROM backup_vaults WHERE locked AND oprf_key IS NULL")
            .await,
        1
    );
    // Turning it on again starts a fresh vault.
    assert!(w
        .zoen("carla", &["backup", "on", "--password", "outra senha boa"])
        .starts_with("backup on"));
    assert!(w
        .zoen(
            "carla-2",
            &["recover", "@carla", "--password", "outra senha boa"]
        )
        .contains("restored"));
}

#[tokio::test]
async fn recovery_key_survives_bad_attempts_and_heals_legacy_lockout() {
    let w = World::with_env("backup_rk_lockout", &[("ZOEN_BACKUP_VAULT_KEY", VAULT_KEY)]).await;
    w.init("ana", "Ana");
    let out = w.zoen("ana", &["backup", "on", "--recovery-key"]);
    let key = out
        .lines()
        .find_map(|line| line.strip_prefix("recovery-key\t"))
        .expect("recovery key");
    let generation = w.scalar("SELECT generation FROM backup_vaults").await;
    let object = w.scalar("SELECT blob_key FROM backup_vaults").await;
    for attempt in 0..12 {
        let (status, body) = restore_post(
            &w,
            "/v1/backup/restore/open",
            serde_json::json!({
                "handle": "ana", "auth_key": "55".repeat(32), "generation": generation,
            }),
        )
        .await;
        assert_eq!(status, 403, "attempt {attempt}: {body:?}");
    }
    assert_eq!(
        w.count(
            "SELECT count(*) FROM backup_vaults WHERE guesses = 0 AND NOT armed AND NOT locked"
        )
        .await,
        1
    );
    assert_eq!(w.scalar("SELECT blob_key FROM backup_vaults").await, object);
    assert!(w
        .zoen("ana-new", &["recover", "@ana", "--recovery-key", key])
        .contains("restored"));
    assert_eq!(
        w.count(
            "WITH changed AS (UPDATE backup_vaults SET locked = true, guesses = 10, armed = true
             RETURNING identity) SELECT count(*) FROM changed"
        )
        .await,
        1
    );
    assert!(w
        .zoen("ana-third", &["recover", "@ana", "--recovery-key", key])
        .contains("restored"));
    assert_eq!(
        w.count(
            "SELECT count(*) FROM backup_vaults WHERE guesses = 0 AND NOT armed AND NOT locked"
        )
        .await,
        1,
        "the correct key must heal lockout left by an older relay"
    );
}

#[tokio::test]
async fn a_recovery_key_restores_without_the_vault() {
    let w = world("backup_rk").await;
    w.init("davi", "Davi");
    w.init("eva", "Eva");
    w.zoen("davi", &["group", "Viagem", "@eva", "--readable"]);
    w.zoen("davi", &["send", "Viagem", "passagens compradas"]);
    let out = w.zoen("davi", &["backup", "on", "--recovery-key"]);
    let key = out
        .lines()
        .find_map(|l| l.strip_prefix("recovery-key\t"))
        .unwrap_or_else(|| panic!("{out}"))
        .to_string();
    assert_eq!(key.chars().filter(|c| c.is_ascii_digit()).count(), 64);
    assert_eq!(
        w.count(
            "SELECT count(*) FROM backup_vaults WHERE mode = 'recovery_key' AND oprf_key IS NULL"
        )
        .await,
        1
    );
    // One wrong digit is a wrong key.
    let mut bad = key.clone().into_bytes();
    let i = bad.iter().position(|b| b.is_ascii_digit()).unwrap();
    bad[i] = if bad[i] == b'9' { b'0' } else { bad[i] + 1 };
    let bad = String::from_utf8(bad).unwrap();
    assert!(w
        .try_zoen("davi-new", &["recover", "@davi", "--recovery-key", &bad])
        .unwrap_err()
        .contains("Wrong password or recovery key"));
    let restored = w.zoen("davi-new", &["recover", "@davi", "--recovery-key", &key]);
    assert!(restored.contains("restored"), "{restored}");
    assert!(w
        .zoen("davi-new", &["read", "Viagem"])
        .contains("Davi: passagens compradas"));
}

async fn restored_device_rejoins_encrypted_chats(recovery_key: bool) {
    let w = world(if recovery_key {
        "backup_e2e_rk"
    } else {
        "backup_e2e_pw"
    })
    .await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    let ana = w.id_of("ana").await;
    let old_device = w
        .scalar(&format!(
            "SELECT device FROM devices WHERE identity = '{ana}'"
        ))
        .await;
    let space = w
        .zoen("bruno", &["group", "Cofre", "@ana"])
        .trim()
        .to_string();
    let before = "antes de perder o celular: tucano";
    w.zoen("bruno", &["send", "Cofre", before]);
    assert!(w.zoen("ana", &["read", "Cofre"]).contains(before));
    let (flag, secret) = if recovery_key {
        let out = w.zoen("ana", &["backup", "on", "--recovery-key"]);
        (
            "--recovery-key",
            out.lines()
                .find_map(|l| l.strip_prefix("recovery-key\t"))
                .expect("recovery key")
                .to_string(),
        )
    } else {
        w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
        ("--password", PASSWORD.to_string())
    };

    // The old phone stays offline throughout recovery. Bruno is the surviving admin.
    let restored = w.zoen("ana-new", &["recover", "@ana", flag, &secret]);
    assert!(restored.contains("connection=online"), "{restored}");
    let new_device = w
        .scalar(&format!(
            "SELECT device FROM devices WHERE identity = '{ana}' AND device <> '{old_device}'"
        ))
        .await;
    assert!(w.events_in(&space).await.iter().any(|ev| {
        ev.env.author() == ana
            && matches!(ev.env.body(), Some(EventBody::DeviceJoining { device }) if device == new_device)
    }), "the restored device never requested its own fresh MLS leaf");
    w.sync_until("bruno", |s| s.contains("pending=0"));
    w.sync_until("ana-new", |s| s.contains("pending=0"));
    let after = "depois de recuperar: maracujá";
    w.zoen("bruno", &["send", "Cofre", after]);
    let history = w.zoen("ana-new", &["read", "Cofre"]);
    assert!(
        history.contains(before) && history.contains(after),
        "{history}"
    );
    let reply = "celular novo também envia: bem-te-vi";
    w.zoen("ana-new", &["send", "Cofre", reply]);
    assert!(w.zoen("bruno", &["read", "Cofre"]).contains(reply));

    // The recovered account can revoke the lost phone and keep the encrypted chat going.
    w.zoen("ana-new", &["unlink", &old_device]);
    w.sync_until("ana-new", |s| s.contains("pending=0"));
    let revoked = w.try_zoen("ana", &["sync"]).unwrap_err();
    assert!(revoked.contains("unlinked"), "{revoked}");
    let last = "depois de revogar o celular antigo";
    w.zoen("bruno", &["send", "Cofre", last]);
    assert!(w.zoen("ana-new", &["read", "Cofre"]).contains(last));
    for who in ["ana-new", "bruno"] {
        let verified = w.zoen(who, &["verify"]);
        assert!(!verified.contains("BROKEN"), "{who}: {verified}");
    }
    assert!(!backup_files(&w).iter().any(|blob| contains(blob, before)));
}

#[tokio::test]
async fn a_restored_password_device_rejoins_encrypted_chats() {
    restored_device_rejoins_encrypted_chats(false).await;
}

#[tokio::test]
async fn a_restored_recovery_key_device_rejoins_encrypted_chats() {
    restored_device_rejoins_encrypted_chats(true).await;
}

#[tokio::test]
async fn a_stale_backup_does_not_restore_removed_membership() {
    let w = world("backup_removed").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    let ana = w.id_of("ana").await;
    let space = w
        .zoen("bruno", &["group", "Revogado", "@ana"])
        .trim()
        .to_string();
    let before = "história de quando eu era membro";
    w.zoen("bruno", &["send", "Revogado", before]);
    assert!(w.zoen("ana", &["read", "Revogado"]).contains(before));
    w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    w.zoen("bruno", &["remove", "Revogado", "@ana"]);
    let after = "segredo depois de remover a Ana";
    w.zoen("bruno", &["send", "Revogado", after]);

    w.zoen("ana-new", &["recover", "@ana", "--password", PASSWORD]);
    w.zoen("bruno", &["sync"]);
    let history = w.zoen("ana-new", &["read", "Revogado"]);
    assert!(history.contains(before), "{history}");
    assert!(
        !history.contains(after),
        "removed membership returned: {history}"
    );
    assert!(
        !w.events_in(&space).await.iter().any(|ev| {
            ev.env.author() == ana && matches!(ev.env.body(), Some(EventBody::DeviceJoining { .. }))
        }),
        "the relay accepted a removed member's recovery join request"
    );
}

async fn peerless_restore(recovery_key: bool) {
    let w = world(if recovery_key {
        "backup_no_peer_rk"
    } else {
        "backup_no_peer_pw"
    })
    .await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    let space = w
        .zoen("ana", &["group", "Sem aparelhos", "@bruno"])
        .trim()
        .to_string();
    let before = "história antes de perder os dois aparelhos";
    w.zoen("ana", &["send", "Sem aparelhos", before]);
    assert!(w.zoen("bruno", &["read", "Sem aparelhos"]).contains(before));
    let ana = w.id_of("ana").await;
    let bruno = w.id_of("bruno").await;
    let old_ana = device_key(&w, "ana").id();
    let old_bruno = device_key(&w, "bruno").id();
    let mut secrets = Vec::new();
    for who in ["ana", "bruno"] {
        w.sync_until(who, |s| s.contains("pending=0"));
        let secret = if recovery_key {
            let out = w.zoen(who, &["backup", "on", "--recovery-key"]);
            out.lines()
                .find_map(|l| l.strip_prefix("recovery-key\t"))
                .expect("recovery key")
                .to_string()
        } else {
            w.zoen(who, &["backup", "on", "--password", PASSWORD]);
            PASSWORD.to_string()
        };
        secrets.push(secret);
    }
    let initial = w.events_in(&space).await;
    assert!(
        initial.iter().any(|ev| ev.env.recovery().is_some()),
        "no recoverable context was durably published"
    );
    assert_eq!(w.sessions_online(), 0);
    // Remove databases, encrypted group state AND vaults. No original process or device
    // survives; only authenticated server backups can recover the identities.
    for who in ["ana", "bruno"] {
        std::fs::remove_dir_all(w.dir.join(who)).unwrap();
    }
    let flag = if recovery_key {
        "--recovery-key"
    } else {
        "--password"
    };
    assert!(w
        .zoen("ana-new", &["recover", "@ana", flag, &secrets[0]])
        .contains("restored"));
    let new_ana = device_key(&w, "ana-new").id();
    assert_ne!(new_ana, old_ana);
    let after_first = "primeiro aparelho recuperado envia sem outro membro online";
    w.zoen("ana-new", &["send", "Sem aparelhos", after_first]);
    w.sync_until("ana-new", |s| s.contains("pending=0"));
    let first_recovery = w.events_in(&space).await;
    assert!(
        first_recovery
            .iter()
            .skip(initial.len())
            .any(|ev| ev.env.device() == Some(new_ana.as_str()) && ev.env.recovery().is_some()),
        "the first recovered device never sequenced its external join"
    );
    assert!(
        !w.dir.join("bruno-new").exists(),
        "a second recovering peer must not assist the first join"
    );
    assert!(w
        .zoen("bruno-new", &["recover", "@bruno", flag, &secrets[1]])
        .contains("restored"));
    let new_bruno = device_key(&w, "bruno-new").id();
    assert_ne!(new_bruno, old_bruno);
    w.zoen(
        "bruno-new",
        &[
            "send",
            "Sem aparelhos",
            "segundo aparelho recuperado responde",
        ],
    );
    w.sync_until("ana-new", |s| s.contains("pending=0"));
    let after_both = "mensagem nova depois de recuperar os dois";
    w.zoen("ana-new", &["send", "Sem aparelhos", after_both]);
    assert!(w
        .zoen("bruno-new", &["read", "Sem aparelhos"])
        .contains(after_both));
    let reply = "resposta cifrada dos dois aparelhos novos";
    w.zoen("bruno-new", &["send", "Sem aparelhos", reply]);
    let history = w.zoen("ana-new", &["read", "Sem aparelhos"]);
    assert!(
        history.contains(before) && history.contains(reply),
        "{history}"
    );
    for who in ["ana-new", "bruno-new"] {
        assert!(!w.zoen(who, &["verify"]).contains("BROKEN"));
    }
    let events = w.events_in(&space).await;
    assert!(
        !events
            .iter()
            .skip(initial.len())
            .any(|ev| matches!(ev.env.device(), Some(d) if d == old_ana || d == old_bruno)),
        "an old device wrote after its state was destroyed"
    );
    assert_eq!(w.count(&format!("SELECT count(*) FROM devices WHERE (identity = '{ana}' AND device = '{new_ana}') OR (identity = '{bruno}' AND device = '{new_bruno}')")).await, 2);
    assert!(!backup_files(&w).iter().any(|blob| contains(blob, before)));
    eprintln!("peerless recovery: both original databases/vaults destroyed; first fresh device externally joined alone; both fresh devices exchanged and verified encrypted messages");
}

#[tokio::test]
async fn both_lost_devices_recover_encrypted_chat_without_a_surviving_mls_peer() {
    peerless_restore(false).await;
}

#[tokio::test]
async fn both_lost_devices_recover_with_recovery_keys_without_a_surviving_mls_peer() {
    peerless_restore(true).await;
}

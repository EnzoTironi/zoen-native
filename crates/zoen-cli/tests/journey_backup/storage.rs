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
        protocol::BACKUP_UPLOAD_MAGIC.as_slice(),
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

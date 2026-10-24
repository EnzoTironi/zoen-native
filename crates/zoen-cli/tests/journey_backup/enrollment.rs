#[tokio::test]
async fn restore_enrollment_requires_current_recovery_authority_and_never_revives_revoked_keys(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use event_log::{Author, Signer};
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
    let device = Author::device(&root, Signer::generate()?);
    let cert = device.cert.as_deref().unwrap();
    let request = serde_json::json!({
        "handle": "ana", "auth_key": auth, "generation": generation,
        "device": device.key.id(), "cert": cert,
        "sig": device.key.sign(&protocol::backup_enroll_message(
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
        .sign(&protocol::backup_enroll_message(
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
    let next = Author::device(&root, Signer::generate()?);
    let cert = next.cert.as_deref().unwrap();
    let stale = serde_json::json!({
        "handle": "ana", "auth_key": auth, "generation": generation,
        "device": next.key.id(), "cert": cert,
        "sig": next.key.sign(&protocol::backup_enroll_message(
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
    Ok(())
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
        protocol::BACKUP_UPLOAD_MAGIC.as_slice(),
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
async fn key_only_restore_never_spends_password_guesses() {
    let w = world("backup_key_only_no_password_guesses").await;
    w.init("ana", "Ana");
    w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    let before = w.scalar("SELECT guesses::text FROM backup_vaults").await;
    w.init("bruno", "Bruno");
    // Explicit key-only mode must reject even a valid OPRF point before accounting.
    let point = "e2f2ae0a6abc4e71a884a961c500515f58e30b6aa582dd8db6a65945e08d2d76";
    for request in [
        serde_json::json!({"handle": "ana", "recovery_only": true, "blinded": point}),
        // The native client omits the point, protecting older relays as well.
        serde_json::json!({"handle": "ana", "recovery_only": true, "blinded": null}),
    ] {
        let (status, body) = restore_post(&w, "/v1/backup/restore/start", request.clone()).await;
        assert_eq!(status, 400);
        for handle in ["bruno", "nonexistent"] {
            let mut decoy_request = request.clone();
            decoy_request["handle"] = serde_json::json!(handle);
            let response = restore_post(&w, "/v1/backup/restore/start", decoy_request).await;
            assert_eq!(
                response,
                (status, body.clone()),
                "key-only existence leak for {handle}"
            );
        }
        assert_eq!(
            w.scalar("SELECT guesses::text FROM backup_vaults").await,
            before
        );
    }
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
                protocol::BACKUP_UPLOAD_MAGIC.as_slice(),
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

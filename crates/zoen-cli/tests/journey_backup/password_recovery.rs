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
    // A full private name tests profile secrecy without three-byte ciphertext collisions.
    const PROFILE_NAME: &str = "Ana - perfil privado do backup";
    w.init("ana", PROFILE_NAME);
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
        PROFILE_NAME,
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
        dm.contains(&format!(
            "{PROFILE_NAME}: oi Bruno, guarda o mapa da trilha"
        )),
        "{dm}"
    );
    assert!(dm.contains("Bruno: guardei!"), "{dm}");
    assert!(w
        .zoen("ana-new", &["read", "Trilha"])
        .contains(&format!("{PROFILE_NAME}: sábado 7h no portão")));
    assert!(w
        .zoen("ana-new", &["whoami"])
        .starts_with(&format!("@ana {PROFILE_NAME}")));

    // And the conversation goes on from the new phone.
    w.zoen("ana-new", &["send", "Trilha", "voltei, celular novo"]);
    assert!(w
        .zoen("bruno", &["read", "Trilha"])
        .contains(&format!("{PROFILE_NAME}: voltei, celular novo")));
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

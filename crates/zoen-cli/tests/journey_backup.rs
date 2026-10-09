//! Encrypted server backup (ADR 0046): Ana turns on a password backup, loses her phone and
//! gets her account and history back on a new one; the relay never holds anything readable;
//! ten wrong passwords lock a backup for good; a recovery key works without the vault.
//!
//!   ZOEN_TEST_PG=postgres://zoen@127.0.0.1:15433/postgres cargo test -p zoen-cli --test journey_backup

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

async fn signed_backup_write(
    w: &World,
    key: &roda_log::Signer,
    identity: &str,
    method: &str,
    op: &str,
    path: &str,
    body: &[u8],
) -> u16 {
    use sha2::{Digest, Sha256};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    let mut socket = tokio::net::TcpStream::connect(&relay).await.unwrap();
    socket.write_all(headers.as_bytes()).await.unwrap();
    socket.write_all(body).await.unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        socket.read_to_end(&mut response),
    )
    .await
    .unwrap()
    .unwrap();
    std::str::from_utf8(&response)
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
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
    let w = world("backup_legacy").await;
    w.init("ana", "Ana");
    w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    let identity = w.id_of("ana").await;
    let sha = w.scalar("SELECT blob_sha FROM backup_vaults").await;
    let legacy_path = w.dir.join("blobs/backups").join(&identity);
    let generated_path = w.dir.join("blobs/backups/v2").join(&identity);
    let object = std::fs::read(generated_path.join(sha)).unwrap();
    // The previous version stored one object directly at backups/{identity}.
    std::fs::remove_dir_all(&generated_path).unwrap();
    std::fs::write(&legacy_path, object).unwrap();
    assert_eq!(
        w.count(
            "WITH changed AS (UPDATE backup_vaults SET generation = NULL RETURNING identity)
         SELECT count(*) FROM changed",
        )
        .await,
        1
    );
    assert!(w
        .zoen("ana-new", &["recover", "@ana", "--password", PASSWORD])
        .contains("restored"));
    let refused = w.try_zoen("ana-new", &["backup", "now"]).unwrap_err();
    assert!(refused.contains("Set up backup again"), "{refused}");
    // Re-enabling upgrades the copy only after the new upload is durable.
    assert!(w
        .zoen("ana-new", &["backup", "on", "--password", PASSWORD])
        .starts_with("backup on"));
    assert!(w
        .zoen("ana-third", &["recover", "@ana", "--password", PASSWORD])
        .contains("restored"));
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

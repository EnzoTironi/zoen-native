//! Encrypted server backup (ADR 0046): Ana turns on a password backup, loses her phone and
//! gets her account and history back on a new one; the relay never holds anything readable;
//! ten wrong passwords lock a backup for good; a recovery key works without the vault.
//!
//!   ZOEN_TEST_PG=postgres://zoen@127.0.0.1:15433/postgres cargo test -p zoen-cli --test journey_backup

mod common;
use common::*;

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
    std::fs::read_dir(&dir)
        .map(|it| {
            it.flatten()
                .map(|e| std::fs::read(e.path()).unwrap())
                .collect()
        })
        .unwrap_or_default()
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
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

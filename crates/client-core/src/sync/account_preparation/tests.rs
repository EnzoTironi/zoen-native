use super::*;
use rusqlite::types::Value;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
type Snapshot = Vec<(String, Vec<String>)>;

// Canonical row contents, including blobs and auxiliary provider tables. File/WAL
// counters and the disposable cache are not an independent data authority.
fn snapshot(engine: &Engine) -> TestResult<Snapshot> {
    let connection = engine.store.conn();
    let mut names = connection.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
    let tables = names
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut result = Vec::new();
    for table in tables {
        let mut statement =
            connection.prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))?;
        let columns = statement.column_count();
        let mut rows = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|column| row.get::<_, Value>(column))
                    .collect::<Result<Vec<_>, _>>()
            })?
            .map(|row| row.map(|values| format!("{values:?}")))
            .collect::<Result<Vec<_>, _>>()?;
        rows.sort();
        result.push((table, rows));
    }
    Ok(result)
}

fn populated(path: &str) -> TestResult<Engine> {
    let mut engine = Engine::open(path)?;
    crate::seed::seed(&mut engine)?;
    let me = engine.me_id()?;
    engine.init_profile(&me, "Retained local profile")?;
    engine.put_media(b"retained private media", "image/png", 1, 1)?;
    engine
        .store
        .set_meta("account.preparation.retained", "unchanged")?;
    Ok(engine)
}

fn keys(failing_stage: Option<u8>) -> Result<AccountKeys, CryptoError> {
    let check = |stage| {
        if failing_stage == Some(stage) {
            Err(CryptoError::Entropy(getrandom::Error::UNSUPPORTED))
        } else {
            Ok(())
        }
    };
    AccountKeys::generate_with(
        true,
        || {
            check(0)?;
            Signer::generate()
        },
        || {
            check(1)?;
            AgreementKey::generate()
        },
        || {
            check(2)?;
            ProfileKey::generate()
        },
        || {
            check(3)?;
            Signer::generate()
        },
    )
}

#[test]
fn key_preparation_failure_preserves_all_local_rows_and_cold_reopen() -> TestResult {
    for stage in 0..4 {
        let directory = Directory::new()?;
        let path = directory.path().join("account.db");
        let path = path.to_str().ok_or("temporary path encoding")?;
        let mut engine = populated(path)?;
        let before = snapshot(&engine)?;
        let me = engine.me.clone();
        let result = engine.create_account_as_with_keys(
            "New person",
            "new_person",
            "https://relay.example",
            Signer::from_secret(&[71; 32]),
            None,
            false,
            || keys(Some(stage)),
        );
        assert!(
            matches!(result, Err(CoreError::Storage { ref message }) if message.contains("cryptographic entropy unavailable")),
            "stage {stage}"
        );
        assert!(engine.account().is_none());
        assert!(!engine.is_unlocked());
        assert_eq!(engine.me, me);
        assert_eq!(snapshot(&engine)?, before, "stage {stage}");
        drop(engine);
        let cold = Engine::open(path)?;
        assert!(cold.account().is_none());
        assert_eq!(cold.me, me);
        assert_eq!(snapshot(&cold)?, before, "cold stage {stage}");
    }
    Ok(())
}

#[test]
fn replacement_install_failure_rolls_back_wipe_and_partial_account() -> TestResult {
    let directory = Directory::new()?;
    let path = directory.path().join("account.db");
    let path = path.to_str().ok_or("temporary path encoding")?;
    let mut engine = populated(path)?;
    let before = snapshot(&engine)?;
    engine.store.conn().execute_batch("CREATE TEMP TRIGGER reject_account_install BEFORE INSERT ON meta WHEN NEW.key='account' BEGIN SELECT RAISE(ABORT, 'controlled account install failure'); END;")?;
    let result = engine.create_account_as_with_keys(
        "New person",
        "new_person",
        "https://relay.example",
        Signer::from_secret(&[72; 32]),
        None,
        false,
        || keys(None),
    );
    assert!(
        matches!(result, Err(CoreError::Storage { ref message }) if message.contains("controlled account install failure"))
    );
    assert!(engine.account().is_none());
    assert!(!engine.is_unlocked());
    assert_eq!(snapshot(&engine)?, before);
    assert!(engine.store.conn().is_autocommit());
    drop(engine);
    let cold = Engine::open(path)?;
    assert!(cold.account().is_none());
    assert_eq!(snapshot(&cold)?, before);
    Ok(())
}

#[test]
fn account_validation_precedes_key_preparation_and_keeps_existing_account() -> TestResult {
    use std::cell::Cell;
    let mut engine = Engine::open(":memory:")?;
    let called = Cell::new(false);
    for (name, handle) in [("", "new_person"), ("New person", "?")] {
        let before = snapshot(&engine)?;
        let result = engine.create_account_as_with_keys(
            name,
            handle,
            "https://relay.example",
            Signer::from_secret(&[73; 32]),
            None,
            false,
            || {
                called.set(true);
                keys(None)
            },
        );
        assert!(matches!(result, Err(CoreError::Invalid { .. })));
        assert!(!called.get());
        assert_eq!(snapshot(&engine)?, before);
    }
    let (_, device, _) =
        engine.create_account("Existing", "existing_person", "https://relay.example")?;
    let before = snapshot(&engine)?;
    let account = engine.account().cloned();
    let result = engine.create_account_as_with_keys(
        "Other",
        "other_person",
        "https://relay.example",
        Signer::from_secret(&[74; 32]),
        None,
        false,
        || {
            called.set(true);
            keys(None)
        },
    );
    assert!(matches!(result, Err(CoreError::Invalid { .. })));
    assert!(!called.get());
    assert_eq!(engine.account(), account.as_ref());
    assert_eq!(snapshot(&engine)?, before);
    assert!(engine.unlock(Some(device.to_vec()))?);
    Ok(())
}

#[test]
fn successful_replacement_uses_prepared_keys_and_survives_reopen() -> TestResult {
    let directory = Directory::new()?;
    let path = directory.path().join("account.db");
    let path = path.to_str().ok_or("temporary path encoding")?;
    let mut engine = populated(path)?;
    let root = Signer::from_secret(&[75; 32]);
    let prepared = keys(None)?;
    let device_id = prepared.device.id();
    let profile_hex = prepared.profile.to_hex();
    let agent_id = prepared
        .personal_agent
        .as_ref()
        .ok_or("prepared personal agent")?
        .id();
    let (_, device, agreement) = engine.create_account_as_with_keys(
        "New",
        "new_person",
        "https://relay.example",
        root.clone(),
        None,
        false,
        || Ok(prepared),
    )?;
    let account = engine.account().ok_or("installed account")?;
    assert_eq!(account.identity, root.id());
    assert_eq!(account.device, device_id);
    assert!(!account.registered);
    assert!(event_log::verify_sig(
        &root.id(),
        &event_log::device_cert_message(&device_id),
        &account.cert
    ));
    assert_eq!(
        engine
            .store
            .profile_key(&root.id())?
            .ok_or("profile key")?
            .key,
        profile_hex
    );
    assert!(engine.identities.contains_key(&agent_id));
    assert!(engine.store.meta("account.preparation.retained")?.is_none());
    assert!(engine.store.conn().is_autocommit());
    drop(engine);
    let mut cold = Engine::open(path)?;
    assert_eq!(cold.account().ok_or("cold account")?.identity, root.id());
    assert!(cold.unlock(Some(device.to_vec()))?);
    cold.unlock_profile(Some(agreement.to_vec()))?;
    assert_eq!(
        cold.store
            .profile_key(&root.id())?
            .ok_or("cold profile")?
            .key,
        profile_hex
    );
    assert!(cold.identities.contains_key(&agent_id));
    Ok(())
}

struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> TestResult<Self> {
        let path = std::env::temp_dir().join(types::new_id("zoen-account-preparation")?);
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path)?;
        Ok(Self(path))
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

use super::*;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

#[derive(Default)]
struct Vault {
    keys: Mutex<HashMap<String, Vec<u8>>>,
    refuse: bool,
}
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.keys.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        if self.refuse {
            return false;
        }
        self.keys.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.keys.lock().unwrap().remove(&key);
    }
}

#[derive(Default)]
struct InterruptedVault {
    held: Vault,
    writes: AtomicUsize,
    stop_after: AtomicUsize,
    stop_delete: std::sync::atomic::AtomicBool,
}
impl SecretVault for InterruptedVault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.held.load(key)
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        let saved = self.held.save(key, value);
        let write = self.writes.fetch_add(1, Ordering::SeqCst) + 1;
        if self.stop_after.load(Ordering::SeqCst) == write {
            panic!("simulated process stop after atomic vault write");
        }
        saved
    }
    fn delete(&self, key: String) {
        if key == VAULT_SETUP && self.stop_delete.load(Ordering::SeqCst) {
            panic!("simulated process stop after canonical commit");
        }
        self.held.delete(key);
    }
}

fn path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "zoen-provider-staging-{}.sqlite",
        types::new_id("test")
    ))
}

#[test]
fn staging_reopens_after_each_atomic_key_write_and_after_database_commit() {
    for boundary in 0..=4 {
        let path = path();
        let core = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
        let setup = ProviderSetup::new("Private initial name", "https://relay.example").unwrap();
        let identity = Signer::from_secret(&setup.root).id();
        let device = Signer::from_secret(&setup.device).id();
        let held = Arc::new(InterruptedVault::default());
        let vault: Arc<dyn SecretVault> = held.clone();
        setup.save(&vault).unwrap();
        if boundary > 0 {
            if boundary <= 3 {
                held.stop_after.store(boundary + 1, Ordering::SeqCst);
            } else {
                held.stop_delete.store(true, Ordering::SeqCst);
            }
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || core.resume_provider_setup(vault.clone())
            ))
            .is_err());
        }
        drop(core);
        held.stop_after.store(0, Ordering::SeqCst);
        held.stop_delete.store(false, Ordering::SeqCst);
        let reopened = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
        assert!(
            reopened.resume_provider_setup(vault.clone()).unwrap(),
            "boundary {boundary}"
        );
        let account = reopened.account().unwrap();
        assert_eq!(account.identity_id, identity);
        assert_eq!(account.device_id, device);
        assert!(account.unlocked && account.setup_pending && !account.registered);
        assert_eq!(account.name, setup.name);
        assert_eq!(reopened.lock().state.spaces.len(), 2);
        assert_eq!(reopened.lock().identities.len(), 2);
        assert_eq!(vault.load(VAULT_IDENTITY.into()), Some(setup.root.to_vec()));
        assert_eq!(vault.load(VAULT_DEVICE.into()), Some(setup.device.to_vec()));
        assert_eq!(
            vault.load(VAULT_AGREEMENT.into()),
            Some(setup.agreement.to_vec())
        );
        assert!(vault.load(VAULT_SETUP.into()).is_none());
        assert!(!reopened.resume_provider_setup(vault.clone()).unwrap());
        assert!(reopened.start_sync(None).is_err());
        drop(reopened);
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn staging_rolls_back_failed_canonical_install_then_retries_same_keys() {
    for (table, when) in [
        ("meta", "NEW.key = 'account'"),
        ("profile_keys", "1"),
        ("events", "1"),
    ] {
        let path = path();
        let core = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
        let setup = ProviderSetup::new("Private name", "https://relay.example").unwrap();
        let vault: Arc<dyn SecretVault> = Arc::new(Vault::default());
        setup.save(&vault).unwrap();
        core.lock().store.conn().execute_batch(&format!("CREATE TRIGGER fail_provider_install BEFORE INSERT ON {table} WHEN {when} BEGIN SELECT RAISE(ABORT, 'injected install failure'); END;")).unwrap();
        assert!(
            core.resume_provider_setup(vault.clone()).is_err(),
            "{table}"
        );
        assert!(core.account().is_none());
        assert!(core.lock().identities.is_empty());
        assert!(core.lock().state.spaces.is_empty());
        assert!(vault.load(VAULT_SETUP.into()).is_some());
        core.lock()
            .store
            .conn()
            .execute_batch("DROP TRIGGER fail_provider_install")
            .unwrap();
        drop(core);
        let reopened = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
        assert!(reopened.resume_provider_setup(vault).unwrap());
        assert_eq!(
            reopened.account().unwrap().identity_id,
            Signer::from_secret(&setup.root).id()
        );
        assert_eq!(reopened.lock().state.spaces.len(), 2);
        drop(reopened);
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn staging_repairs_only_its_own_unregistered_partial_account() {
    let core = Client::open(":memory:".into(), "en".into()).unwrap();
    let setup = ProviderSetup::new("Private name", "https://relay.example").unwrap();
    let root = Signer::from_secret(&setup.root);
    let device = Signer::from_secret(&setup.device);
    let metadata = crate::sync::AccountMeta {
        identity: root.id(),
        device: device.id(),
        cert: root.sign(&event_log::device_cert_message(&device.id())),
        relay_url: setup.relay.clone(),
        registered: false,
    };
    let vault: Arc<dyn SecretVault> = Arc::new(Vault::default());
    setup.save(&vault).unwrap();
    {
        let mut engine = core.lock();
        engine.store.set_meta(PENDING, "1").unwrap();
        engine
            .store
            .set_meta("account", &serde_json::to_string(&metadata).unwrap())
            .unwrap();
        engine.reload().unwrap();
        engine.unlock(Some(setup.device.to_vec())).unwrap();
        engine
            .append_at(
                "sp_partial_seed",
                &root.id(),
                1,
                types::EventBody::SpaceCreated {
                    title: "Zoen".into(),
                    kind: types::SpaceKind::Direct,
                    privacy: types::Privacy::EndToEnd,
                },
            )
            .unwrap();
    }
    assert!(core.resume_provider_setup(vault.clone()).unwrap());
    let profile = core.lock().store.profile_key(&root.id()).unwrap().unwrap();
    setup.save(&vault).unwrap(); // Commit succeeded; cleanup had not run.
    assert!(core.resume_provider_setup(vault.clone()).unwrap());
    assert_eq!(
        core.lock().store.profile_key(&root.id()).unwrap().unwrap(),
        profile
    );
    assert_eq!(core.lock().state.spaces.len(), 2);
    setup.save(&vault).unwrap();
    core.lock().set_registered(true).unwrap();
    assert!(core.resume_provider_setup(vault.clone()).is_err());
    assert!(core.account().unwrap().registered);
    assert!(vault.load(VAULT_SETUP.into()).is_some());
}

#[test]
fn staging_rejects_corruption_conflicting_keys_and_other_accounts_without_mutation() {
    let core = Client::open(":memory:".into(), "en".into()).unwrap();
    let held = Arc::new(Vault::default());
    let vault: Arc<dyn SecretVault> = held.clone();
    vault.save(VAULT_SETUP.into(), b"invalid journal".to_vec());
    assert!(core.resume_provider_setup(vault.clone()).is_err());
    assert_eq!(held.keys.lock().unwrap().len(), 1);
    let setup = ProviderSetup::new("Private name", "https://relay.example").unwrap();
    setup.save(&vault).unwrap();
    vault.save(VAULT_DEVICE.into(), vec![8; 32]);
    let before = held.keys.lock().unwrap().clone();
    assert!(core.resume_provider_setup(vault.clone()).is_err());
    assert_eq!(*held.keys.lock().unwrap(), before);
    assert!(core.account().is_none());
    held.keys.lock().unwrap().remove(VAULT_DEVICE);
    core.lock()
        .create_account("Existing person", "existing", "https://relay.example")
        .unwrap();
    let existing = core.account().unwrap();
    let before = held.keys.lock().unwrap().clone();
    assert!(core.resume_provider_setup(vault.clone()).is_err());
    assert_eq!(core.account().unwrap(), existing);
    assert_eq!(*held.keys.lock().unwrap(), before);
}

fn binding_server() -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        let mut first: Option<(String, String)> = None;
        for attempt in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut wire = Vec::new();
            let (header_end, length) = loop {
                let mut chunk = [0; 4096];
                let size = stream.read(&mut chunk).unwrap();
                assert!(size > 0);
                wire.extend_from_slice(&chunk[..size]);
                if let Some(end) = wire.windows(4).position(|window| window == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&wire[..end]);
                    assert!(headers.starts_with("POST /v1/auth/bind HTTP/1.1"));
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    break (end + 4, length);
                }
            };
            while wire.len() < header_end + length {
                let mut chunk = [0; 4096];
                let size = stream.read(&mut chunk).unwrap();
                assert!(size > 0);
                wire.extend_from_slice(&chunk[..size]);
            }
            let request: ProviderBindRequest =
                serde_json::from_slice(&wire[header_end..header_end + length]).unwrap();
            assert!(request.profile.name.is_empty() && request.profile.bio.is_empty());
            assert!(event_log::verify_sig(
                &request.profile.id,
                &event_log::device_cert_message(&request.device),
                &request.certificate
            ));
            assert!(event_log::verify_sig(
                &request.device,
                &provider_bind_message(
                    &request.proof_token,
                    &request.profile,
                    &request.device,
                    &request.certificate
                )
                .unwrap(),
                &request.signature
            ));
            if let Some((identity, device)) = &first {
                assert_eq!(identity, &request.profile.id);
                assert_eq!(device, &request.device);
            } else {
                first = Some((request.profile.id.clone(), request.device.clone()));
            }
            let body = serde_json::to_string(&ProviderBindResponse {
                identity_id: request.profile.id,
                handle: request.profile.handle,
            })
            .unwrap();
            let status = if attempt == 0 {
                "503 Service Unavailable"
            } else {
                "200 OK"
            };
            write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (origin, thread)
}

#[test]
fn provider_proofs_are_never_sent_to_cleartext_or_ambiguous_endpoints() {
    for bad in [
        "http://relay.example",
        "https://x:secret@example.com",
        "https://example.com/path",
        "https://example.com?token=x",
        "https://example.com#x",
        "file:///tmp/auth",
    ] {
        assert!(relay_base(bad).is_err(), "{bad}");
    }
    assert_eq!(
        relay_base("https://relay.example/").unwrap(),
        "https://relay.example"
    );
    assert_eq!(
        relay_base("http://127.0.0.1:8790").unwrap(),
        "http://127.0.0.1:8790"
    );
}

#[tokio::test]
async fn interrupted_signup_preserves_keys_across_reopen_until_signed_binding_confirms() {
    let (relay, server) = binding_server();
    let path = std::env::temp_dir().join(format!("zoen-provider-{}.sqlite", types::new_id("test")));
    let vault = Arc::new(Vault::default());
    let first = Client::open(path.to_string_lossy().into(), "pt-BR".into()).unwrap();
    let failure = first
        .provider_sign_in(
            relay.clone(),
            "a".repeat(64),
            "  Zoen Test  ".into(),
            vault.clone(),
        )
        .await
        .unwrap_err();
    assert!(!failure.to_string().contains(&"a".repeat(64)));
    let pending = first.account().unwrap();
    assert!(pending.setup_pending && !pending.registered);
    assert_eq!(pending.name, "Zoen Test");
    assert!(pending.handle.starts_with("zoen_"));
    assert_eq!(vault.keys.lock().unwrap().len(), 3);
    assert!(first.start_sync(None).is_err());
    drop(first);
    let reopened = Client::open(path.to_string_lossy().into(), "pt-BR".into()).unwrap();
    assert!(reopened.unlock(vault.clone()).unwrap());
    assert!(reopened.account().unwrap().setup_pending);
    let complete = reopened
        .provider_sign_in(
            relay,
            "b".repeat(64),
            "Changed provider name".into(),
            vault.clone(),
        )
        .await
        .unwrap();
    assert!(complete.registered && complete.unlocked && !complete.setup_pending);
    assert_eq!(pending.identity_id, complete.identity_id);
    assert_eq!(pending.device_id, complete.device_id);
    assert_eq!(complete.name, "Zoen Test");
    let snapshot = reopened
        .client_request(r#"{"op":"snapshot"}"#.into())
        .await
        .unwrap();
    for key in vault.keys.lock().unwrap().values() {
        assert!(!snapshot.contains(&hex::encode(key)));
    }
    server.join().unwrap();
    drop(reopened);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn unavailable_vault_or_invalid_proof_cannot_leave_an_account() {
    let core = Client::open(":memory:".into(), "en".into()).unwrap();
    let vault = Arc::new(Vault {
        refuse: true,
        ..Default::default()
    });
    assert!(core
        .provider_sign_in(
            "http://127.0.0.1:1".into(),
            "a".repeat(64),
            "Name".into(),
            vault.clone()
        )
        .await
        .is_err());
    assert!(core.account().is_none());
    assert!(!core.lock().provider_setup_pending());
    assert!(vault.keys.lock().unwrap().is_empty());
    assert!(core
        .provider_sign_in(
            "http://127.0.0.1:1".into(),
            "invalid".into(),
            "Name".into(),
            vault
        )
        .await
        .is_err());
    assert!(core.account().is_none());
}

#[tokio::test]
async fn existing_vault_material_is_never_overwritten_by_social_signup() {
    let core = Client::open(":memory:".into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    vault.save(VAULT_IDENTITY.into(), vec![7; 32]);
    assert!(core
        .provider_sign_in(
            "http://127.0.0.1:1".into(),
            "a".repeat(64),
            "Name".into(),
            vault.clone()
        )
        .await
        .is_err());
    assert!(core.account().is_none());
    assert_eq!(vault.load(VAULT_IDENTITY.into()), Some(vec![7; 32]));
}

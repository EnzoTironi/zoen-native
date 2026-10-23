use super::*;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{atomic::AtomicUsize, Mutex},
};

#[derive(Default)]
struct Vault {
    rows: Mutex<BTreeMap<String, Vec<u8>>>,
    writes: AtomicUsize,
    fail_at: AtomicUsize,
    accept_failure: AtomicBool,
    deletes: AtomicUsize,
    on_delete: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.rows.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, bytes: Vec<u8>) -> bool {
        let nth = self.writes.fetch_add(1, Ordering::SeqCst) + 1;
        let fail = self.fail_at.load(Ordering::SeqCst) == nth;
        if !fail || self.accept_failure.load(Ordering::SeqCst) {
            self.rows.lock().unwrap().insert(key, bytes);
        }
        !fail
    }
    fn delete(&self, key: String) {
        let hook = self.on_delete.lock().unwrap().take();
        if let Some(hook) = hook {
            hook();
        }
        self.deletes.fetch_add(1, Ordering::SeqCst);
        self.rows.lock().unwrap().remove(&key);
    }
}
struct Material {
    pending: Pending,
    device: Signer,
    boxed: IdentityBox,
}
impl Material {
    fn new() -> Self {
        let identity = Signer::from_secret(&[81; 32]);
        let device = Signer::from_secret(&[82; 32]);
        let (hpke_secret, hpke_public) = link::key_pair();
        let (_, primary) = link::key_pair();
        let pending = Pending {
            device_secret: hex::encode(device.secret()),
            hpke_secret: hex::encode(hpke_secret),
            hpke_public: hex::encode(hpke_public),
            secret: hex::encode([85; 32]),
            relay: "http://127.0.0.1:1".into(),
        };
        let fields = event_log::profile::ProfileFields {
            name: "Receiver fixture".into(),
            ..Default::default()
        };
        let boxed = IdentityBox {
            identity_secret: hex::encode(identity.secret()),
            agreement_secret: hex::encode([83; 32]),
            cert: identity.sign(&event_log::device_cert_message(&device.id())),
            name: "Receiver fixture".into(),
            handle: "receiver_fixture".into(),
            relay: pending.relay.clone(),
            primary: Signer::from_secret(&[86; 32]).id(),
            primary_hpke: hex::encode(primary),
            profile_key: hex::encode([84; 32]),
            profile_key_version: 1,
            profile_seen_version: 1,
            profile_fields: hex::encode(fields.to_bytes()),
        };
        Self {
            pending,
            device,
            boxed,
        }
    }
    fn vault(&self) -> Arc<Vault> {
        let vault = Arc::new(Vault::default());
        vault.rows.lock().unwrap().insert(
            link::VAULT_PENDING.into(),
            serde_json::to_vec(&self.pending).unwrap(),
        );
        vault
    }
    fn install(&self, client: &Client, vault: Arc<Vault>) -> Result<AccountDto, CoreError> {
        client.finish_link(
            &self.pending,
            &self.device,
            self.boxed.clone(),
            vault,
            &ExpectedIdentity::parse(None).unwrap(),
        )
    }
}
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(types::new_id("zoen-link-install"));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn open(&self) -> Arc<Client> {
        Client::open(
            self.0.join("device.sqlite").to_string_lossy().into(),
            "en".into(),
        )
        .unwrap()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn assert_not_installed(client: &Client) {
    assert!(client.account().is_none());
    let engine = client.lock();
    assert!(engine.net.author.is_none());
    assert!(engine.net.link.peers.is_empty());
    assert!(engine.store.meta("account").unwrap().is_none());
    assert!(engine.store.meta(META_PRIMARY).unwrap().is_none());
    assert!(engine
        .store
        .meta(META_INITIAL_HISTORY_PENDING)
        .unwrap()
        .is_none());
    assert_eq!(engine.store.event_count().unwrap(), 0);
}

#[test]
fn each_rejected_or_ambiguous_vault_write_keeps_keys_retryable_across_cold_reopen() {
    for accept_failure in [false, true] {
        for fail_at in 1..=4 {
            let directory = Directory::new();
            let material = Material::new();
            let vault = material.vault();
            let pending = vault.load(link::VAULT_PENDING.into()).unwrap();
            vault.fail_at.store(fail_at, Ordering::SeqCst);
            vault.accept_failure.store(accept_failure, Ordering::SeqCst);
            let client = directory.open();
            assert!(material.install(&client, vault.clone()).is_err());
            assert_eq!(vault.writes.load(Ordering::SeqCst), fail_at);
            assert_eq!(vault.deletes.load(Ordering::SeqCst), 0);
            assert_eq!(vault.load(link::VAULT_PENDING.into()).unwrap(), pending);
            assert_not_installed(&client);
            let retained = vault.rows.lock().unwrap().clone();
            drop(client);
            let client = directory.open();
            assert_not_installed(&client);
            vault.fail_at.store(0, Ordering::SeqCst);
            let account = material.install(&client, vault.clone()).unwrap();
            assert_eq!(account.identity_id, Signer::from_secret(&[81; 32]).id());
            assert_eq!(account.device_id, material.device.id());
            for (key, bytes) in retained
                .iter()
                .filter(|(key, _)| key.as_str() != link::VAULT_PENDING)
            {
                assert_eq!(
                    vault.load(key.clone()).as_ref(),
                    Some(bytes),
                    "accepted key bytes must not change on retry"
                );
            }
            assert_eq!(vault.load(VAULT_AGREEMENT.into()).unwrap(), vec![83; 32]);
            assert!(vault.load(link::VAULT_PENDING.into()).is_none());
            let peers: Vec<Peer> =
                serde_json::from_slice(&vault.load(VAULT_PEERS.into()).unwrap()).unwrap();
            assert_eq!(peers.len(), 1);
            drop(client);
            let client = directory.open();
            assert!(client.unlock(vault.clone()).unwrap());
            assert_eq!(client.account().unwrap().identity_id, account.identity_id);
            assert_eq!(client.account().unwrap().device_id, account.device_id);
            let engine = client.lock();
            assert_eq!(
                engine.net.profiles.agreement.as_ref().unwrap().secret(),
                [83; 32]
            );
            assert_eq!(
                engine
                    .store
                    .profile_key(&account.identity_id)
                    .unwrap()
                    .unwrap()
                    .key,
                material.boxed.profile_key
            );
        }
    }
}

#[test]
fn missing_or_invalid_incoming_agreement_and_profile_keys_never_generate_replacement_authority() {
    for (agreement, profile, version) in [
        ("".to_owned(), hex::encode([84; 32]), 1),
        ("not-hex".into(), hex::encode([84; 32]), 1),
        (hex::encode([83; 31]), hex::encode([84; 32]), 1),
        (hex::encode([83; 33]), hex::encode([84; 32]), 1),
        (hex::encode([83; 32]), "".into(), 0),
        (hex::encode([83; 32]), "invalid".into(), 1),
        (hex::encode([83; 32]), hex::encode([84; 32]), 0),
    ] {
        let mut material = Material::new();
        material.boxed.agreement_secret = agreement;
        material.boxed.profile_key = profile;
        material.boxed.profile_key_version = version;
        let vault = material.vault();
        let before = vault.rows.lock().unwrap().clone();
        let client = Client::open(":memory:".into(), "en".into()).unwrap();
        assert!(material.install(&client, vault.clone()).is_err());
        assert_eq!(vault.writes.load(Ordering::SeqCst), 0);
        assert_eq!(vault.deletes.load(Ordering::SeqCst), 0);
        assert_eq!(*vault.rows.lock().unwrap(), before);
        assert_not_installed(&client);
    }
}

#[test]
fn retained_foreign_vault_material_is_not_overwritten_by_a_new_link() {
    for key in [VAULT_IDENTITY, VAULT_DEVICE, VAULT_AGREEMENT] {
        let material = Material::new();
        let vault = material.vault();
        vault.rows.lock().unwrap().insert(key.into(), vec![99; 32]);
        let before = vault.rows.lock().unwrap().clone();
        let client = Client::open(":memory:".into(), "en".into()).unwrap();
        assert!(material.install(&client, vault.clone()).is_err());
        assert_eq!(*vault.rows.lock().unwrap(), before);
        assert_eq!(vault.writes.load(Ordering::SeqCst), 0);
        assert_not_installed(&client);
    }
}

#[test]
fn profile_storage_failure_rolls_back_before_any_vault_write_and_the_same_link_can_retry() {
    let material = Material::new();
    let vault = material.vault();
    let client = Client::open(":memory:".into(), "en".into()).unwrap();
    client.lock().store.conn().execute_batch(
        "CREATE TEMP TRIGGER refuse_link_profile BEFORE INSERT ON profile_keys BEGIN SELECT RAISE(ABORT, 'fixture profile failure'); END;"
    ).unwrap();
    assert!(matches!(
        material.install(&client, vault.clone()),
        Err(CoreError::Storage { .. })
    ));
    assert_eq!(vault.writes.load(Ordering::SeqCst), 0);
    assert_not_installed(&client);
    client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER refuse_link_profile")
        .unwrap();
    let account = material.install(&client, vault.clone()).unwrap();
    assert_eq!(account.device_id, material.device.id());
    assert_eq!(vault.load(VAULT_AGREEMENT.into()).unwrap(), vec![83; 32]);
}

#[test]
fn deleting_pending_and_capturing_the_install_receipt_keep_the_same_engine_guard() {
    let material = Material::new();
    let vault = material.vault();
    let client = Client::open(":memory:".into(), "en".into()).unwrap();
    let weak = Arc::downgrade(&client);
    *vault.on_delete.lock().unwrap() = Some(Box::new(move || {
        let client = weak.upgrade().unwrap();
        assert!(
            matches!(
                client.inner.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ),
            "owner replacement must be excluded while the old pending key is deleted"
        );
    }));
    let account = material.install(&client, vault).unwrap();
    assert_eq!(account.identity_id, Signer::from_secret(&[81; 32]).id());
    assert_eq!(account.device_id, material.device.id());
}

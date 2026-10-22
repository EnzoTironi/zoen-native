use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
#[derive(Default)]
struct Vault {
    reads: AtomicUsize,
    writes: AtomicUsize,
}
impl SecretVault for Vault {
    fn load(&self, _: String) -> Option<Vec<u8>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        None
    }
    fn save(&self, _: String, _: Vec<u8>) -> bool {
        self.writes.fetch_add(1, Ordering::SeqCst);
        true
    }
    fn delete(&self, _: String) {
        self.writes.fetch_add(1, Ordering::SeqCst);
    }
}
#[test]
fn provider_identity_mismatch_cannot_install_a_decrypted_link_or_write_secrets() {
    let core = Client::open(":memory:".into(), "en-US".into()).unwrap();
    let vault = Arc::new(Vault::default());
    let wrong = Signer::from_secret(&[91; 32]);
    let expected = ExpectedIdentity::parse(Some(Signer::from_secret(&[92; 32]).id())).unwrap();
    let device = Signer::from_secret(&[93; 32]);
    let pending = Pending {
        device_secret: hex::encode(device.secret()),
        hpke_secret: String::new(),
        hpke_public: String::new(),
        secret: hex::encode([94; 32]),
        relay: "http://localhost".into(),
    };
    let boxed = IdentityBox {
        identity_secret: hex::encode(wrong.secret()),
        agreement_secret: String::new(),
        cert: wrong.sign(&event_log::device_cert_message(&device.id())),
        name: "Wrong account".into(),
        handle: "wrong_account".into(),
        relay: pending.relay.clone(),
        primary: wrong.id(),
        primary_hpke: String::new(),
        profile_key: String::new(),
        profile_key_version: 0,
        profile_seen_version: 0,
        profile_fields: String::new(),
    };
    assert!(core
        .finish_link(&pending, &device, boxed, vault.clone(), &expected)
        .is_err());
    assert!(core.account().is_none());
    assert!(core.lock().store.meta("account").unwrap().is_none());
    assert_eq!(vault.reads.load(Ordering::SeqCst), 0);
    assert_eq!(vault.writes.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn malformed_provider_identity_is_refused_before_vault_reads_or_recovery_network() {
    let core = Client::open(":memory:".into(), "en-US".into()).unwrap();
    let vault = Arc::new(Vault::default());
    assert!(core
        .link_wait_expected(vault.clone(), 1, Some("invalid".into()))
        .await
        .is_err());
    assert!(core
        .restore_backup_expected(
            "not a URL".into(),
            "valid_handle".into(),
            "secret".into(),
            vault.clone(),
            false,
            Some("invalid".into())
        )
        .await
        .is_err());
    assert!(core.account().is_none());
    assert_eq!(vault.reads.load(Ordering::SeqCst), 0);
    assert_eq!(vault.writes.load(Ordering::SeqCst), 0);
}

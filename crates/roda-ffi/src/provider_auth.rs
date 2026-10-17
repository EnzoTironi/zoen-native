//! Social authentication provisions a fresh local identity, then binds its certified
//! device to a verified provider subject. An interrupted request retains those keys
//! and remains on onboarding until an idempotent server confirmation succeeds.

use std::{sync::Arc, time::Duration};

use roda_log::{profile::AgreementKey, Signer};
use roda_proto::provider_auth::{provider_bind_message, ProviderBindRequest, ProviderBindResponse};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::{
    api::invalid,
    profile::VAULT_AGREEMENT,
    sync::{VAULT_DEVICE, VAULT_IDENTITY},
    AccountDto, CoreError, RodaEngine, SecretVault,
};

const PENDING: &str = "provider_signup_pending";
pub(crate) const VAULT_SETUP: &str = "zoen.provider-setup.v1";

/// A single atomic vault record precedes every canonical database/key-slot write.
/// It contains no provider proof and is discarded only after the local install is
/// validated. A process can stop after any later write and replay these fixed keys.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProviderSetup {
    pub(crate) version: u8,
    pub(crate) root: [u8; 32],
    pub(crate) device: [u8; 32],
    pub(crate) agreement: [u8; 32],
    pub(crate) name: String,
    pub(crate) handle: String,
    pub(crate) relay: String,
}

impl Drop for ProviderSetup {
    fn drop(&mut self) {
        self.root.zeroize();
        self.device.zeroize();
        self.agreement.zeroize();
    }
}

impl ProviderSetup {
    fn new(name: &str, relay: &str) -> Self {
        let root = Signer::generate();
        Self {
            version: 1,
            root: root.secret(),
            device: Signer::generate().secret(),
            agreement: AgreementKey::generate().secret(),
            name: name.into(),
            handle: format!("zoen_{}", &root.id()[..16]),
            relay: relay.into(),
        }
    }

    fn validate(&self) -> Result<(), CoreError> {
        if self.version != 1
            || self.name.trim() != self.name
            || self.name.is_empty()
            || self.name.chars().count() > 64
            || self.name.chars().any(char::is_control)
            || self.relay.len() > 2048
            || relay_base(&self.relay)? != self.relay
            || self.handle != format!("zoen_{}", &Signer::from_secret(&self.root).id()[..16])
            || self.root == self.device
        {
            return Err(invalid(
                "A preparação desta conta está inválida. As chaves foram preservadas.",
            ));
        }
        Ok(())
    }

    pub(crate) fn load(vault: &Arc<dyn SecretVault>) -> Result<Option<Self>, CoreError> {
        let Some(bytes) = vault.load(VAULT_SETUP.into()).map(Zeroizing::new) else {
            return Ok(None);
        };
        if bytes.len() > 4096 {
            return Err(invalid(
                "A preparação desta conta está inválida. As chaves foram preservadas.",
            ));
        }
        let setup: Self = serde_json::from_slice(&bytes).map_err(|_| {
            invalid("A preparação desta conta está inválida. As chaves foram preservadas.")
        })?;
        setup.validate()?;
        Ok(Some(setup))
    }

    fn save(&self, vault: &Arc<dyn SecretVault>) -> Result<(), CoreError> {
        let bytes = Zeroizing::new(
            serde_json::to_vec(self)
                .map_err(|_| invalid("Não foi possível preparar as chaves."))?,
        );
        let saved = vault.save(VAULT_SETUP.into(), bytes.to_vec());
        let held = vault.load(VAULT_SETUP.into()).map(Zeroizing::new);
        if !saved || held.as_deref().map(|bytes| bytes.as_slice()) != Some(bytes.as_slice()) {
            return Err(invalid(
                "Não foi possível guardar as chaves deste aparelho.",
            ));
        }
        Ok(())
    }

    pub(crate) fn resume(
        &self,
        engine: &mut crate::engine::Engine,
        vault: &Arc<dyn SecretVault>,
    ) -> Result<(), CoreError> {
        self.validate()?;
        // Validate the whole scope before filling even one missing key slot.
        engine.validate_provider_setup(self)?;
        let keys = [
            (VAULT_IDENTITY, &self.root),
            (VAULT_DEVICE, &self.device),
            (VAULT_AGREEMENT, &self.agreement),
        ];
        for (key, secret) in keys {
            let held = vault.load(key.into()).map(Zeroizing::new);
            if held.as_ref().is_some_and(|held| held.as_slice() != secret) {
                return Err(invalid(
                    "Estas chaves pertencem a outra conta. Nenhuma chave foi substituída.",
                ));
            }
        }
        for (key, secret) in keys {
            let held = vault.load(key.into()).map(Zeroizing::new);
            if held.is_none() && !vault.save(key.into(), secret.to_vec()) {
                return Err(invalid(
                    "Não foi possível guardar as chaves deste aparelho. Tente novamente.",
                ));
            }
            let held = vault.load(key.into()).map(Zeroizing::new);
            if held.as_ref().map(|held| held.as_slice()) != Some(secret.as_slice()) {
                return Err(invalid(
                    "Não foi possível conferir as chaves deste aparelho. Tente novamente.",
                ));
            }
        }
        engine.install_provider_setup(self)?;
        engine.validate_installed_provider_setup(self)?;
        for (key, secret) in keys {
            let held = vault.load(key.into()).map(Zeroizing::new);
            if held.as_ref().map(|held| held.as_slice()) != Some(secret.as_slice()) {
                return Err(invalid(
                    "Não foi possível conferir as chaves deste aparelho. Tente novamente.",
                ));
            }
        }
        vault.delete(VAULT_SETUP.into());
        if vault.load(VAULT_SETUP.into()).map(Zeroizing::new).is_some() {
            return Err(invalid(
                "Não foi possível concluir a preparação. Suas chaves foram preservadas.",
            ));
        }
        Ok(())
    }
}

fn relay_base(value: &str) -> Result<String, CoreError> {
    let parsed = reqwest::Url::parse(value).map_err(|_| invalid("Servidor inválido."))?;
    let loopback = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if !(parsed.scheme() == "https" || (parsed.scheme() == "http" && loopback))
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !matches!(parsed.path(), "" | "/")
    {
        return Err(invalid("Use o endereço HTTPS do servidor."));
    }
    Ok(parsed.as_str().trim_end_matches('/').to_string())
}

impl RodaEngine {
    pub(crate) async fn provider_sign_in(
        &self,
        relay_url: String,
        proof_token: String,
        name: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<AccountDto, CoreError> {
        if proof_token.len() != 64 || !proof_token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid(
                "Confirme sua entrada com Apple ou Google novamente.",
            ));
        }
        let relay = relay_base(&relay_url)?;
        let name: String = name
            .trim()
            .chars()
            .filter(|c| !c.is_control())
            .take(64)
            .collect();
        let name = if name.trim().is_empty() {
            "Pessoa Zoen"
        } else {
            name.trim()
        };
        let request = {
            let mut e = self.lock();
            if let Some(setup) = ProviderSetup::load(&vault)? {
                if setup.relay != relay {
                    return Err(invalid("Esta entrada pertence a outro servidor."));
                }
                setup.resume(&mut e, &vault)?;
            } else if e.account().is_none() {
                if [VAULT_IDENTITY, VAULT_DEVICE, VAULT_AGREEMENT]
                    .iter()
                    .any(|key| vault.load((*key).into()).map(Zeroizing::new).is_some())
                {
                    return Err(invalid(
                        "Recupere a conta deste aparelho antes de criar outra.",
                    ));
                }
                let setup = ProviderSetup::new(name, &relay);
                e.validate_provider_setup(&setup)?;
                setup.save(&vault)?;
                setup.resume(&mut e, &vault)?;
            }
            let account = e
                .account()
                .ok_or_else(|| invalid("Conta indisponível."))?
                .clone();
            if relay_base(&account.relay_url)? != relay {
                return Err(invalid("Esta entrada pertence a outro servidor."));
            }
            let mut profile = e
                .my_profile()
                .ok_or_else(|| invalid("Perfil indisponível."))?;
            // Names and bios of people are encrypted by the existing profile protocol.
            profile.name.clear();
            profile.bio.clear();
            let author = e
                .net
                .author
                .as_ref()
                .ok_or_else(|| invalid("Desbloqueie este aparelho."))?;
            let message =
                provider_bind_message(&proof_token, &profile, &account.device, &account.cert)
                    .map_err(|_| invalid("Não foi possível preparar a entrada."))?;
            ProviderBindRequest {
                proof_token,
                profile,
                device: account.device,
                certificate: account.cert,
                signature: author.key.sign(&message),
            }
        };
        let body = serde_json::to_vec(&request)
            .map_err(|_| invalid("Não foi possível preparar a entrada."))?;
        let http = crate::https::client()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| invalid("Não foi possível conectar ao servidor."))?;
        let mut response = http.post(format!("{relay}/v1/auth/bind"))
            .header("content-type", "application/json")
            .body(body).send().await.map_err(|_| invalid("Não foi possível concluir a entrada. Suas chaves foram preservadas; tente novamente."))?;
        if !response.status().is_success() {
            return Err(invalid(
                if response.status() == reqwest::StatusCode::CONFLICT {
                    "Esta conta já existe. Recupere o acesso pelo seu outro aparelho ou backup."
                } else {
                    "Confirme sua entrada com Apple ou Google novamente. Suas chaves foram preservadas."
                },
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| invalid("Resposta de entrada incompleta. Tente novamente."))?
        {
            if bytes.len().saturating_add(chunk.len()) > 8192 {
                return Err(invalid("Resposta de entrada inválida."));
            }
            bytes.extend_from_slice(&chunk);
        }
        let confirmed: ProviderBindResponse =
            serde_json::from_slice(&bytes).map_err(|_| invalid("Resposta de entrada inválida."))?;
        {
            let mut e = self.lock();
            let account = e
                .account()
                .ok_or_else(|| invalid("Esta sessão foi encerrada."))?;
            if account.identity != request.profile.id
                || account.device != request.device
                || confirmed.identity_id != account.identity
                || confirmed.handle != request.profile.handle
            {
                return Err(invalid(
                    "A conta mudou durante a entrada. Abra o Zoen novamente.",
                ));
            }
            e.set_registered(true)?;
            e.store.meta_delete(PENDING)?;
        }
        self.account_dto()
            .ok_or_else(|| invalid("Conta indisponível."))
    }
}

#[cfg(test)]
mod tests {
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
            roda_types::new_id("test")
        ))
    }

    #[test]
    fn staging_reopens_after_each_atomic_key_write_and_after_database_commit() {
        for boundary in 0..=4 {
            let path = path();
            let core = RodaEngine::open(path.to_string_lossy().into(), "en".into()).unwrap();
            let setup = ProviderSetup::new("Private initial name", "https://relay.example");
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
            let reopened = RodaEngine::open(path.to_string_lossy().into(), "en".into()).unwrap();
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
            let core = RodaEngine::open(path.to_string_lossy().into(), "en".into()).unwrap();
            let setup = ProviderSetup::new("Private name", "https://relay.example");
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
            let reopened = RodaEngine::open(path.to_string_lossy().into(), "en".into()).unwrap();
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
        let core = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
        let setup = ProviderSetup::new("Private name", "https://relay.example");
        let root = Signer::from_secret(&setup.root);
        let device = Signer::from_secret(&setup.device);
        let metadata = crate::sync::AccountMeta {
            identity: root.id(),
            device: device.id(),
            cert: root.sign(&roda_log::device_cert_message(&device.id())),
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
                    roda_types::EventBody::SpaceCreated {
                        title: "Zoen".into(),
                        kind: roda_types::SpaceKind::Direct,
                        privacy: roda_types::Privacy::EndToEnd,
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
        let core = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
        let held = Arc::new(Vault::default());
        let vault: Arc<dyn SecretVault> = held.clone();
        vault.save(VAULT_SETUP.into(), b"invalid journal".to_vec());
        assert!(core.resume_provider_setup(vault.clone()).is_err());
        assert_eq!(held.keys.lock().unwrap().len(), 1);
        let setup = ProviderSetup::new("Private name", "https://relay.example");
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
                assert!(roda_log::verify_sig(
                    &request.profile.id,
                    &roda_log::device_cert_message(&request.device),
                    &request.certificate
                ));
                assert!(roda_log::verify_sig(
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
        let path = std::env::temp_dir().join(format!(
            "zoen-provider-{}.sqlite",
            roda_types::new_id("test")
        ));
        let vault = Arc::new(Vault::default());
        let first = RodaEngine::open(path.to_string_lossy().into(), "pt-BR".into()).unwrap();
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
        let reopened = RodaEngine::open(path.to_string_lossy().into(), "pt-BR".into()).unwrap();
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
        let core = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
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
        let core = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
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
}

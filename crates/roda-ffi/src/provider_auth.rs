//! Social authentication provisions a fresh local identity, then binds its certified
//! device to a verified provider subject. An interrupted request retains those keys
//! and remains on onboarding until an idempotent server confirmation succeeds.

use std::{sync::Arc, time::Duration};

use roda_proto::provider_auth::{provider_bind_message, ProviderBindRequest, ProviderBindResponse};

use crate::{
    api::invalid,
    profile::VAULT_AGREEMENT,
    sync::{VAULT_DEVICE, VAULT_IDENTITY},
    AccountDto, CoreError, RodaEngine, SecretVault,
};

const PENDING: &str = "provider_signup_pending";

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
        let name = if name.is_empty() {
            "Pessoa Zoen"
        } else {
            &name
        };
        let request = {
            let mut e = self.lock();
            if e.account().is_none() {
                if [VAULT_IDENTITY, VAULT_DEVICE, VAULT_AGREEMENT]
                    .iter()
                    .any(|key| vault.load((*key).into()).is_some())
                {
                    return Err(invalid(
                        "Recupere a conta deste aparelho antes de criar outra.",
                    ));
                }
                let (root, device, agreement) = e.create_provider_account(name, &relay)?;
                if !vault.save(VAULT_IDENTITY.into(), root.to_vec())
                    || !vault.save(VAULT_DEVICE.into(), device.to_vec())
                    || !vault.save(VAULT_AGREEMENT.into(), agreement.to_vec())
                {
                    for key in [VAULT_IDENTITY, VAULT_DEVICE, VAULT_AGREEMENT] {
                        vault.delete(key.into());
                    }
                    e.wipe()?;
                    e.net = Default::default();
                    return Err(invalid(
                        "Não foi possível guardar as chaves deste aparelho.",
                    ));
                }
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
        sync::Mutex,
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

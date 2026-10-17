//! Encrypted backup in the app API (ADR 0046): turn it on with a password or a recovery
//! key, back up now, turn it off, and restore on a new device.

use std::{sync::Arc, time::Duration};

use roda_proto::{backup_message, BACKUP_UPLOAD_MAGIC};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    api::{AccountDto, SecretVault},
    backup::{self, Header, Kdf, Keys, VAULT_BACKUP, VAULT_BACKUP_GENERATION},
    i18n::t,
    net::http_base,
    profile::VAULT_AGREEMENT,
    sync::{VAULT_DEVICE, VAULT_IDENTITY},
    CoreError, RodaEngine,
};

const STATUS_META: &str = "backup.status";

#[derive(Debug, Clone, PartialEq, uniffi::Record, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct BackupStatusDto {
    pub enabled: bool,
    /// "password" or "recovery_key" (empty while off).
    pub mode: String,
    /// When the last backup reached the server (ms since 1970; 0 = never).
    pub last_backup_ms: i64,
    pub bytes: u64,
}

fn invalid(reason: impl Into<String>) -> CoreError {
    CoreError::Invalid {
        reason: reason.into(),
    }
}

fn offline(e: impl std::fmt::Display) -> CoreError {
    invalid(format!(
        "{} ({e})",
        t(
            "Sem conexão com o servidor agora.",
            "Can't reach the server right now."
        )
    ))
}

fn http() -> Result<reqwest::Client, CoreError> {
    // Restore runs before any sync session has installed the TLS provider.
    let _ = rustls::crypto::ring::default_provider().install_default();
    crate::https::client()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(offline)
}

/// Turns a relay refusal into words for people.
async fn refused(r: reqwest::Response) -> CoreError {
    let status = r.status().as_u16();
    let body = r.text().await.unwrap_or_default();
    match status {
        403 if body.contains("device") => invalid(t(
            "Este aparelho foi desvinculado. Use um aparelho autorizado.",
            "This device was unlinked. Use an authorized device.",
        )),
        403 => invalid(t(
            "Senha ou chave de recuperação errada.",
            "Wrong password or recovery key.",
        )),
        423 => invalid(t(
            "Muitas tentativas erradas. Este backup não pode mais ser aberto.",
            "Too many wrong attempts. This backup can no longer be opened.",
        )),
        404 => invalid(t(
            "Não encontramos um backup para esse @.",
            "We couldn't find a backup for that @.",
        )),
        429 => invalid(t(
            "Muitas tentativas. Espere um pouco e tente de novo.",
            "Too many attempts. Wait a bit and try again.",
        )),
        503 if body.contains("not configured") => invalid(t(
            "Backup com senha ainda não está disponível neste servidor.",
            "Password backup isn't available on this server yet.",
        )),
        _ => invalid(format!("backup: {status} {body}")),
    }
}

/// What a signed backup write needs, read under the lock and released before any await.
struct Creds {
    base: String,
    relay_name: String,
    identity: String,
    key: roda_log::Signer,
}

async fn signed(
    http: &reqwest::Client,
    c: &Creds,
    method: reqwest::Method,
    path: &str,
    op: &str,
    body: Vec<u8>,
) -> Result<reqwest::Response, CoreError> {
    let ts = crate::engine::now_ms();
    let sha = hex::encode(Sha256::digest(&body));
    let sig = c
        .key
        .sign(&backup_message(&c.relay_name, &c.identity, op, &sha, ts));
    let r = http
        .request(method, format!("{}{path}", c.base))
        .header("x-zoen-device", c.key.id())
        .header("x-zoen-ts", ts.to_string())
        .header("x-zoen-sig", sig)
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(offline)?;
    if r.status().is_success() {
        Ok(r)
    } else {
        Err(refused(r).await)
    }
}

#[derive(Deserialize)]
struct Evaluated {
    evaluated: String,
}

#[derive(Deserialize)]
struct Started {
    identity: String,
    mode: String,
    kdf: serde_json::Value,
    evaluated: Option<String>,
    generation: Option<String>,
}

#[derive(Deserialize)]
struct Opened {
    wrapped_key: String,
    generation: Option<String>,
}

async fn json<T: serde::de::DeserializeOwned>(r: reqwest::Response) -> Result<T, CoreError> {
    let b = r.bytes().await.map_err(offline)?;
    serde_json::from_slice(&b).map_err(|_| invalid("backup: bad answer from the server"))
}

fn hex32(s: &str) -> Result<[u8; 32], CoreError> {
    hex::decode(s)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| invalid("backup: bad answer from the server"))
}

impl RodaEngine {
    fn backup_creds(&self) -> Result<Creds, CoreError> {
        let e = self.lock();
        let acct = e.account().cloned().ok_or_else(|| invalid("no account"))?;
        let key = e
            .net
            .author
            .as_ref()
            .map(|a| a.key.clone())
            .ok_or_else(|| invalid("locked"))?;
        let relay_name = e.store.meta("relay_name")?.ok_or_else(|| {
            invalid(t(
                "Conecte ao servidor uma vez antes de ligar o backup.",
                "Connect to the server once before turning on backup.",
            ))
        })?;
        Ok(Creds {
            base: http_base(&acct.relay_url),
            relay_name,
            identity: acct.identity,
            key,
        })
    }

    fn save_status(&self, s: &BackupStatusDto) -> Result<(), CoreError> {
        let j = serde_json::to_string(s).map_err(|e| invalid(e.to_string()))?;
        self.lock().store.set_meta(STATUS_META, &j)?;
        Ok(())
    }

    async fn put_vault(
        &self,
        c: &Creds,
        mode: &str,
        keys: &Keys,
        kdf: serde_json::Value,
        generation: [u8; 32],
        vault: &Arc<dyn SecretVault>,
    ) -> Result<(), CoreError> {
        let k = match vault
            .load(VAULT_BACKUP.into())
            .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok())
        {
            Some(k) => k,
            None => backup::new_backup_key(),
        };
        let body = serde_json::json!({
            "generation": hex::encode(generation),
            "mode": mode,
            "verifier": hex::encode(keys.verifier()),
            "wrapped_key": hex::encode(backup::wrap_key(keys, &c.identity, &k)),
            "kdf": kdf,
        });
        signed(
            &http()?,
            c,
            reqwest::Method::PUT,
            "/v1/backup/vault",
            "vault",
            body.to_string().into_bytes(),
        )
        .await?;
        if !vault.save(VAULT_BACKUP.into(), k.to_vec())
            || !vault.save(VAULT_BACKUP_GENERATION.into(), generation.to_vec())
        {
            return Err(invalid(t(
                "Não deu para guardar a chave no Keychain.",
                "Couldn't store the key in the Keychain.",
            )));
        }
        Ok(())
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl RodaEngine {
    /// Turns on the encrypted backup protected by a password (8+ characters). The server
    /// counts wrong guesses and, after 10, the backup can never be opened again.
    pub async fn backup_turn_on_password(
        &self,
        password: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<BackupStatusDto, CoreError> {
        if password.chars().count() < backup::MIN_PASSWORD_CHARS {
            return Err(invalid(t(
                "A senha precisa ter pelo menos 8 caracteres.",
                "The password needs at least 8 characters.",
            )));
        }
        let c = self.backup_creds()?;
        let b = backup::blind(&password);
        let generation = backup::new_backup_key();
        let body = serde_json::json!({
            "blinded": hex::encode(b.blinded), "generation": hex::encode(generation),
        });
        let r = signed(
            &http()?,
            &c,
            reqwest::Method::POST,
            "/v1/backup/oprf",
            "oprf",
            body.to_string().into_bytes(),
        )
        .await?;
        let ev: Evaluated = json(r).await?;
        let kdf = Kdf::default();
        let password2 = password.clone();
        let identity = c.identity.clone();
        let evaluated = hex32(&ev.evaluated)?;
        let kdf2 = kdf.clone();
        // Argon2id with 64 MiB takes a moment: off the async threads.
        let language = self.lang;
        let keys = tokio::task::spawn_blocking(move || {
            crate::i18n::with_lang(language, || {
                backup::password_keys(&password2, &b, &evaluated, &identity, &kdf2)
            })
        })
        .await
        .map_err(|e| invalid(e.to_string()))??;
        self.put_vault(
            &c,
            "passphrase",
            &keys,
            serde_json::to_value(&kdf).unwrap_or_default(),
            generation,
            &vault,
        )
        .await?;
        self.save_status(&BackupStatusDto {
            enabled: true,
            mode: "password".into(),
            ..Default::default()
        })?;
        self.backup_now(vault).await
    }

    /// Turns on the encrypted backup protected by a 64-digit recovery key, which this
    /// returns once: only the person keeps it.
    pub async fn backup_turn_on_recovery_key(
        &self,
        vault: Arc<dyn SecretVault>,
    ) -> Result<String, CoreError> {
        let rk = self.backup_create_recovery_key();
        self.backup_enable_recovery_key(rk.clone(), vault).await?;
        Ok(rk)
    }

    /// Activates a key the person has already saved and confirmed. Retrying with the
    /// same key is safe after an interrupted upload; the UI must retain it on failure.
    pub async fn backup_enable_recovery_key(
        &self,
        recovery_key: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<BackupStatusDto, CoreError> {
        let keys = backup::recovery_keys(&recovery_key)?;
        let c = self.backup_creds()?;
        self.put_vault(
            &c,
            "recovery_key",
            &keys,
            serde_json::json!({"alg": "hkdf-sha256", "v": 1}),
            backup::new_backup_key(),
            &vault,
        )
        .await?;
        self.save_status(&BackupStatusDto {
            enabled: true,
            mode: "recovery_key".into(),
            ..Default::default()
        })?;
        self.backup_now(vault).await
    }

    /// Seals this device's history and keys and sends them to the server.
    pub async fn backup_now(
        &self,
        vault: Arc<dyn SecretVault>,
    ) -> Result<BackupStatusDto, CoreError> {
        let k: [u8; 32] = vault
            .load(VAULT_BACKUP.into())
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| invalid(t("O backup está desligado.", "Backup is off.")))?;
        let generation: [u8; 32] = vault
            .load(VAULT_BACKUP_GENERATION.into())
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| {
                invalid(t(
                    "Configure o backup novamente neste aparelho.",
                    "Set up backup again on this device.",
                ))
            })?;
        let c = self.backup_creds()?;
        let secret = |name: &str| -> Result<String, CoreError> {
            vault
                .load(name.into())
                .map(hex::encode)
                .ok_or_else(|| invalid("missing key"))
        };
        let sealed = {
            let e = self.lock();
            let acct = e.account().cloned().ok_or_else(|| invalid("no account"))?;
            let header = Header {
                version: 1,
                identity: acct.identity.clone(),
                relay_url: acct.relay_url.clone(),
                identity_secret: secret(VAULT_IDENTITY)?,
                agreement_secret: secret(VAULT_AGREEMENT)?,
                created_ms: crate::engine::now_ms(),
            };
            let db = e.backup_snapshot()?;
            backup::seal_payload(&k, &header, &db)?
        };
        let bytes = sealed.len() as u64;
        let upload = [BACKUP_UPLOAD_MAGIC.as_slice(), &generation, &sealed].concat();
        signed(
            &http()?,
            &c,
            reqwest::Method::PUT,
            "/v1/backup/blob",
            "blob",
            upload,
        )
        .await?;
        let mut s = self.backup_status();
        s.enabled = true;
        s.last_backup_ms = crate::engine::now_ms();
        s.bytes = bytes;
        self.save_status(&s)?;
        Ok(s)
    }

    /// Turns the backup off and deletes it from the server.
    pub async fn backup_turn_off(&self, vault: Arc<dyn SecretVault>) -> Result<(), CoreError> {
        let c = self.backup_creds()?;
        signed(
            &http()?,
            &c,
            reqwest::Method::DELETE,
            "/v1/backup",
            "delete",
            Vec::new(),
        )
        .await?;
        vault.delete(VAULT_BACKUP.into());
        vault.delete(VAULT_BACKUP_GENERATION.into());
        self.lock().store.meta_delete(STATUS_META)?;
        Ok(())
    }

    /// On a device without an account: finds @handle's backup on `relay_url`, opens it with
    /// the password or the recovery key, installs the history and makes this device a new
    /// device of that account.
    pub async fn restore_backup(
        &self,
        relay_url: String,
        handle: String,
        secret: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<AccountDto, CoreError> {
        self.restore_backup_expected(relay_url, handle, secret, vault, false, None)
            .await
    }

    /// Recovery UI never submits a password or consumes password guesses.
    pub async fn restore_backup_with_recovery_key(
        &self,
        relay_url: String,
        handle: String,
        recovery_key: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<AccountDto, CoreError> {
        self.restore_backup_expected(relay_url, handle, recovery_key, vault, true, None)
            .await
    }
}

impl RodaEngine {
    pub(crate) async fn restore_backup_expected(
        &self,
        relay_url: String,
        handle: String,
        secret: String,
        vault: Arc<dyn SecretVault>,
        recovery_only: bool,
        expected_identity_id: Option<String>,
    ) -> Result<AccountDto, CoreError> {
        let expected = crate::link_api::ExpectedIdentity::parse(expected_identity_id)?;
        if recovery_only {
            backup::recovery_keys(&secret)?;
        }
        if self.lock().account().is_some() {
            return Err(invalid(t(
                "Este aparelho já tem uma conta.",
                "This device already has an account.",
            )));
        }
        let handle = roda_proto::normalize_handle(&handle)
            .ok_or_else(|| invalid(t("Esse @ não parece certo.", "That @ doesn't look right.")))?;
        let base = http_base(&relay_url);
        let http = http()?;
        let b = backup::blind(&secret);
        let r = http
            .post(format!("{base}/v1/backup/restore/start"))
            .header("content-type", "application/json")
            .body(
                serde_json::json!({
                    "handle": handle,
                    // No password OPRF point: even older relays reject this without
                    // spending a password guess when the vault is password-backed.
                    "blinded": if recovery_only { None } else { Some(hex::encode(b.blinded)) },
                    "recovery_only": recovery_only,
                })
                .to_string(),
            )
            .send()
            .await
            .map_err(offline)?;
        if !r.status().is_success() {
            return Err(refused(r).await);
        }
        let s: Started = json(r).await?;
        if roda_log::parse_device_key(&s.identity).is_none() {
            return Err(invalid("backup identity"));
        }
        expected.check(&s.identity)?;
        if recovery_only && s.mode != "recovery_key" {
            return Err(invalid(t(
                "Esta cópia usa senha. Use um aparelho que já esteja conectado.",
                "This backup uses a password. Use a device already signed in.",
            )));
        }
        let keys = match s.mode.as_str() {
            "recovery_key" => backup::recovery_keys(&secret)?,
            _ => {
                let kdf: Kdf = serde_json::from_value(s.kdf.clone())
                    .map_err(|_| invalid("unsupported backup format"))?;
                let evaluated = hex32(s.evaluated.as_deref().unwrap_or_default())?;
                let identity = s.identity.clone();
                let language = self.lang;
                tokio::task::spawn_blocking(move || {
                    crate::i18n::with_lang(language, || {
                        backup::password_keys(&secret, &b, &evaluated, &identity, &kdf)
                    })
                })
                .await
                .map_err(|e| invalid(e.to_string()))??
            }
        };
        let auth = hex::encode(keys.auth);
        let r = http
            .post(format!("{base}/v1/backup/restore/open"))
            .header("content-type", "application/json")
            .body(
                serde_json::json!({
                    "handle": handle, "auth_key": auth, "generation": s.generation,
                })
                .to_string(),
            )
            .send()
            .await
            .map_err(offline)?;
        if !r.status().is_success() {
            return Err(refused(r).await);
        }
        let o: Opened = json(r).await?;
        if o.generation != s.generation {
            return Err(invalid("backup configuration changed; restart restore"));
        }
        let generation = o.generation.as_deref().map(hex32).transpose()?;
        let k = backup::unwrap_key(
            &keys,
            &s.identity,
            &hex::decode(&o.wrapped_key).map_err(|_| invalid("backup"))?,
        )?;
        let mut download_url =
            reqwest::Url::parse(&format!("{base}/v1/backup/restore/blob")).map_err(offline)?;
        download_url
            .query_pairs_mut()
            .append_pair("handle", &handle);
        if let Some(generation) = &s.generation {
            download_url
                .query_pairs_mut()
                .append_pair("generation", generation);
        }
        let r = http
            .get(download_url)
            .header("x-zoen-backup-auth", &auth)
            .send()
            .await
            .map_err(offline)?;
        if !r.status().is_success() {
            return Err(refused(r).await);
        }
        let sealed = r.bytes().await.map_err(offline)?;
        let (mut header, db) = backup::open_payload(&k, &s.identity, &sealed)?;
        // Restore onto the relay the person chose (the backup may predate a move).
        header.relay_url = relay_url.clone();
        let fail = || {
            invalid(t(
                "Não deu para guardar a chave no Keychain.",
                "Couldn't store the key in the Keychain.",
            ))
        };
        let root = hex::decode(&header.identity_secret).map_err(|_| invalid("backup"))?;
        let agreement = hex::decode(&header.agreement_secret).map_err(|_| invalid("backup"))?;
        let root_secret: [u8; 32] = root
            .as_slice()
            .try_into()
            .map_err(|_| invalid("backup identity"))?;
        let root_signer = roda_log::Signer::from_secret(&root_secret);
        expected.check(&root_signer.id())?;
        if root_signer.id() != s.identity {
            return Err(invalid("backup identity"));
        }
        let device = roda_log::Signer::generate();
        let author = roda_log::Author::device(&root_signer, device.clone());
        let cert = author.cert.as_deref().expect("device certificate");
        let configuration = s
            .generation
            .as_deref()
            .ok_or_else(|| invalid("backup server does not support device recovery"))?;
        let signature = device.sign(&roda_proto::backup_enroll_message(
            &s.identity,
            &device.id(),
            cert,
            configuration,
        ));
        let r = http
            .post(format!("{base}/v1/backup/restore/enroll"))
            .header("content-type", "application/json")
            .body(
                serde_json::json!({
                    "handle": handle,
                    "auth_key": auth,
                    "generation": configuration,
                    "device": device.id(),
                    "cert": cert,
                    "sig": signature,
                })
                .to_string(),
            )
            .send()
            .await
            .map_err(offline)?;
        if !r.status().is_success() {
            return Err(refused(r).await);
        }
        {
            let mut e = self.lock();
            let device = e.restore_snapshot(&header, &db, device)?;
            if !vault.save(VAULT_IDENTITY.into(), root)
                || !vault.save(VAULT_DEVICE.into(), device.to_vec())
                || !vault.save(VAULT_AGREEMENT.into(), agreement.clone())
                || !vault.save(VAULT_BACKUP.into(), k.to_vec())
            {
                let _ = e.wipe();
                let _ = e.store.meta_delete("account");
                return Err(fail());
            }
            if let Some(generation) = generation {
                if !vault.save(VAULT_BACKUP_GENERATION.into(), generation.to_vec()) {
                    let _ = e.wipe();
                    let _ = e.store.meta_delete("account");
                    return Err(fail());
                }
            } else {
                vault.delete(VAULT_BACKUP_GENERATION.into());
            }
            e.unlock_profile(Some(agreement))?;
            let status = BackupStatusDto {
                enabled: true,
                mode: if s.mode == "recovery_key" {
                    "recovery_key".into()
                } else {
                    "password".into()
                },
                last_backup_ms: header.created_ms,
                bytes: sealed.len() as u64,
            };
            if let Ok(j) = serde_json::to_string(&status) {
                e.store.set_meta(STATUS_META, &j)?;
            }
        }
        self.account_dto()
            .ok_or_else(|| invalid("account after restore"))
    }
}

#[uniffi::export]
impl RodaEngine {
    /// Generates locally, before activation or network access. Never persisted or logged.
    pub fn backup_create_recovery_key(&self) -> String {
        backup::new_recovery_key()
    }

    /// Whether the backup is on, how, and when it last ran.
    pub fn backup_status(&self) -> BackupStatusDto {
        self.lock()
            .store
            .meta(STATUS_META)
            .ok()
            .flatten()
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod pin_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[derive(Default)]
    struct Vault(AtomicUsize);
    impl SecretVault for Vault {
        fn load(&self, _: String) -> Option<Vec<u8>> {
            None
        }
        fn save(&self, _: String, _: Vec<u8>) -> bool {
            self.0.fetch_add(1, Ordering::SeqCst);
            true
        }
        fn delete(&self, _: String) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[tokio::test]
    async fn mismatched_backup_identity_stops_after_start_before_authentication_or_enrollment() {
        let wrong = roda_log::Signer::from_secret(&[81; 32]).id();
        let expected = roda_log::Signer::from_secret(&[82; 32]).id();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                counted.fetch_add(1, Ordering::SeqCst);
                let mut bytes = vec![0; 8192];
                let _ = socket.read(&mut bytes).await.unwrap();
                let body=serde_json::json!({"identity":wrong,"mode":"recovery_key","kdf":null,"evaluated":null,"generation":"aa".repeat(32)}).to_string();
                let response=format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let core = RodaEngine::open(":memory:".into(), "en-US".into()).unwrap();
        let vault = Arc::new(Vault::default());
        let result = core
            .restore_backup_expected(
                format!("http://{address}"),
                "valid_handle".into(),
                "password".into(),
                vault.clone(),
                false,
                Some(expected),
            )
            .await;
        assert!(
            matches!(result,Err(CoreError::Invalid {reason}) if reason.contains("another account"))
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(core.account().is_none());
        assert_eq!(vault.0.load(Ordering::SeqCst), 0);
        server.abort();
    }
}

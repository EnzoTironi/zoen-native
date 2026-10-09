//! Encrypted backup in the app API (ADR 0046): turn it on with a password or a recovery
//! key, back up now, turn it off, and restore on a new device.

use std::{sync::Arc, time::Duration};

use roda_proto::backup_message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    api::{AccountDto, SecretVault},
    backup::{self, Header, Kdf, Keys, VAULT_BACKUP},
    i18n::t,
    net::http_base,
    profile::VAULT_AGREEMENT,
    sync::{VAULT_DEVICE, VAULT_IDENTITY},
    CoreError, RodaEngine,
};

const STATUS_META: &str = "backup.status";

#[derive(Debug, Clone, PartialEq, uniffi::Record, Serialize, Deserialize, Default)]
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
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(offline)
}

/// Turns a relay refusal into words for people.
async fn refused(r: reqwest::Response) -> CoreError {
    let status = r.status().as_u16();
    let body = r.text().await.unwrap_or_default();
    match status {
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
}

#[derive(Deserialize)]
struct Opened {
    wrapped_key: String,
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
        if !vault.save(VAULT_BACKUP.into(), k.to_vec()) {
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
        let body = serde_json::json!({ "blinded": hex::encode(b.blinded) });
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
        let keys = tokio::task::spawn_blocking(move || {
            backup::password_keys(&password2, &b, &evaluated, &identity, &kdf2)
        })
        .await
        .map_err(|e| invalid(e.to_string()))??;
        self.put_vault(
            &c,
            "passphrase",
            &keys,
            serde_json::to_value(&kdf).unwrap_or_default(),
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
        let c = self.backup_creds()?;
        let rk = backup::new_recovery_key();
        let keys = backup::recovery_keys(&rk)?;
        self.put_vault(
            &c,
            "recovery_key",
            &keys,
            serde_json::json!({"alg": "hkdf-sha256", "v": 1}),
            &vault,
        )
        .await?;
        self.save_status(&BackupStatusDto {
            enabled: true,
            mode: "recovery_key".into(),
            ..Default::default()
        })?;
        self.backup_now(vault).await?;
        Ok(rk)
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
        signed(
            &http()?,
            &c,
            reqwest::Method::PUT,
            "/v1/backup/blob",
            "blob",
            sealed,
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
                serde_json::json!({"handle": handle, "blinded": hex::encode(b.blinded)})
                    .to_string(),
            )
            .send()
            .await
            .map_err(offline)?;
        if !r.status().is_success() {
            return Err(refused(r).await);
        }
        let s: Started = json(r).await?;
        let keys = match s.mode.as_str() {
            "recovery_key" => backup::recovery_keys(&secret)?,
            _ => {
                let kdf: Kdf = serde_json::from_value(s.kdf.clone())
                    .map_err(|_| invalid("unsupported backup format"))?;
                let evaluated = hex32(s.evaluated.as_deref().unwrap_or_default())?;
                let identity = s.identity.clone();
                tokio::task::spawn_blocking(move || {
                    backup::password_keys(&secret, &b, &evaluated, &identity, &kdf)
                })
                .await
                .map_err(|e| invalid(e.to_string()))??
            }
        };
        let auth = hex::encode(keys.auth);
        let r = http
            .post(format!("{base}/v1/backup/restore/open"))
            .header("content-type", "application/json")
            .body(serde_json::json!({"handle": handle, "auth_key": auth}).to_string())
            .send()
            .await
            .map_err(offline)?;
        if !r.status().is_success() {
            return Err(refused(r).await);
        }
        let o: Opened = json(r).await?;
        let k = backup::unwrap_key(
            &keys,
            &s.identity,
            &hex::decode(&o.wrapped_key).map_err(|_| invalid("backup"))?,
        )?;
        let r = http
            .get(format!("{base}/v1/backup/restore/blob?handle={handle}"))
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
        {
            let mut e = self.lock();
            let device = e.restore_snapshot(&header, &db)?;
            if !vault.save(VAULT_IDENTITY.into(), root)
                || !vault.save(VAULT_DEVICE.into(), device.to_vec())
                || !vault.save(VAULT_AGREEMENT.into(), agreement.clone())
                || !vault.save(VAULT_BACKUP.into(), k.to_vec())
            {
                let _ = e.wipe();
                let _ = e.store.meta_delete("account");
                return Err(fail());
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

//! Existing backup restore lifecycle.
use super::{
    backup, hex32, http, http_base, invalid, json, offline, refused, t, AccountDto, Arc,
    BackupStatusDto, Client, CoreError, Kdf, Opened, SecretVault, Started, STATUS_META,
    VAULT_AGREEMENT, VAULT_BACKUP, VAULT_BACKUP_GENERATION, VAULT_DEVICE, VAULT_IDENTITY,
};

impl Client {
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
        let handle = protocol::normalize_handle(&handle)
            .ok_or_else(|| invalid(t("Esse @ não parece certo.", "That @ doesn't look right.")))?;
        let base = http_base(&relay_url);
        let http = http()?;
        let blinded = backup::blind(&secret)?;
        let r = http
            .post(format!("{base}/v1/backup/restore/start"))
            .header("content-type", "application/json")
            .body(
                serde_json::json!({
                    "handle": handle,
                    // No password OPRF point: even older relays reject this without
                    // spending a password guess when the vault is password-backed.
                    "blinded": if recovery_only { None } else { Some(hex::encode(blinded.blinded)) },
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
        let started: Started = json(r).await?;
        if event_log::parse_device_key(&started.identity).is_none() {
            return Err(invalid("backup identity"));
        }
        expected.check(&started.identity)?;
        if recovery_only && started.mode != "recovery_key" {
            return Err(invalid(t(
                "Esta cópia usa senha. Use um aparelho que já esteja conectado.",
                "This backup uses a password. Use a device already signed in.",
            )));
        }
        let keys = match started.mode.as_str() {
            "recovery_key" => backup::recovery_keys(&secret)?,
            _ => {
                let kdf: Kdf = serde_json::from_value(started.kdf.clone())
                    .map_err(|_| invalid("unsupported backup format"))?;
                let evaluated = hex32(started.evaluated.as_deref().unwrap_or_default())?;
                let identity = started.identity.clone();
                let language = self.lang;
                tokio::task::spawn_blocking(move || {
                    crate::i18n::with_lang(language, || {
                        backup::password_keys(&secret, &blinded, &evaluated, &identity, &kdf)
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
                    "handle": handle, "auth_key": auth, "generation": started.generation,
                })
                .to_string(),
            )
            .send()
            .await
            .map_err(offline)?;
        if !r.status().is_success() {
            return Err(refused(r).await);
        }
        let opened: Opened = json(r).await?;
        if opened.generation != started.generation {
            return Err(invalid("backup configuration changed; restart restore"));
        }
        let generation = opened.generation.as_deref().map(hex32).transpose()?;
        let backup_key = backup::unwrap_key(
            &keys,
            &started.identity,
            &hex::decode(&opened.wrapped_key).map_err(|_| invalid("backup"))?,
        )?;
        let mut download_url =
            reqwest::Url::parse(&format!("{base}/v1/backup/restore/blob")).map_err(offline)?;
        download_url
            .query_pairs_mut()
            .append_pair("handle", &handle);
        if let Some(generation) = &started.generation {
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
        let (mut header, db) = backup::open_payload(&backup_key, &started.identity, &sealed)?;
        // Prove the authenticated payload can be imported before enrolling a
        // device or touching this device's existing data or vault.
        backup::preflight_snapshot(&header, &db)?;
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
        let root_signer = event_log::Signer::from_secret(&root_secret);
        expected.check(&root_signer.id())?;
        if root_signer.id() != started.identity {
            return Err(invalid("backup identity"));
        }
        let device = event_log::Signer::generate()?;
        let author = event_log::Author::device(&root_signer, device.clone());
        let cert = author
            .cert
            .as_deref()
            .ok_or_else(|| invalid("device certificate unavailable"))?;
        let configuration = started
            .generation
            .as_deref()
            .ok_or_else(|| invalid("backup server does not support device recovery"))?;
        let signature = device.sign(&protocol::backup_enroll_message(
            &started.identity,
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
                || !vault.save(VAULT_BACKUP.into(), backup_key.to_vec())
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
                mode: if started.mode == "recovery_key" {
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

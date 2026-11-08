//! Snapshot upload inside the one backup mutation admission; no nested lock.
use super::*;
impl Client {
    pub(super) async fn backup_now_locked(
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
            e.native_surface_owner(&c.identity, &c.key.id())?;
            let acct = e.account().cloned().ok_or_else(|| invalid("no account"))?;
            if http_base(&acct.relay_url) != c.base {
                return Err(invalid("backup account changed"));
            }
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
        self.check_backup_creds(&c)?;
        let mut s = self.backup_status();
        s.in_progress = false;
        s.enabled = true;
        s.last_backup_ms = crate::engine::now_ms();
        s.bytes = bytes;
        self.save_status(&s)?;
        Ok(s)
    }
}

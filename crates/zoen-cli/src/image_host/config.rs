//! Operator-only configuration. Content requests cannot select vault locators,
//! principals, prices, routing, continuity storage or deployment evidence.
use crate::file_vault::FileVault;
use client_core::SecretVault;
use serde::Deserialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
use zeroize::Zeroizing;
use zoen_models::{EndpointPolicy, GatewayConfig, ModelLimits};
use zoen_runtime::{
    ImageDeliveryTarget, ImageHostConfig, ImagePriceProfile, PriceProfile, RuntimeConfig,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    pub namespace: String,
    pub agent: String,
    pub device: String,
    pub home: PathBuf,
    pub continuity: PathBuf,
    pub postgres_key: String,
    pub evidence_key: String,
    pub fdb_cluster_file: Option<String>,
    pub source_cell: String,
    pub relay_url: String,
    pub relay_key: String,
    pub key_version: u32,
    pub definition_version: String,
    pub image_price: ImagePriceProfile,
    pub image_credential_key: String,
    pub chat_price: PriceProfile,
    pub chat_credential_key: String,
}
impl Config {
    pub(super) fn read(path: &Path) -> Result<Self, String> {
        let metadata =
            std::fs::symlink_metadata(path).map_err(|_| "image host configuration unavailable")?;
        if !metadata.is_file() || metadata.len() > 32 * 1024 {
            return Err("invalid image host configuration file".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.mode() & 0o077 != 0 {
                return Err("image host configuration must be operator private (0600)".into());
            }
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|_| "configuration unavailable")?
            .take(32 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "configuration unavailable")?;
        if bytes.len() > 32 * 1024 {
            return Err("configuration too large".into());
        }
        let config: Self =
            serde_json::from_slice(&bytes).map_err(|_| "invalid image host configuration")?;
        if !config.home.is_absolute()
            || !config.continuity.is_absolute()
            || config.home == config.continuity
        {
            return Err("use distinct absolute host and continuity directories".into());
        }
        config
            .chat_price
            .digest()
            .map_err(|_| "invalid trusted chat profile")?;
        config
            .image_price
            .digest()
            .map_err(|_| "invalid trusted image profile")?;
        Ok(config)
    }
    pub(super) fn runtime(&self, vault: &FileVault) -> Result<RuntimeConfig, String> {
        let evidence = secret(vault, &self.evidence_key)?;
        let evidence_key: [u8; 32] = evidence
            .as_slice()
            .try_into()
            .map_err(|_| "invalid evidence key")?;
        let endpoint = self
            .chat_price
            .endpoint
            .strip_suffix("/chat/completions")
            .ok_or("invalid chat profile")?;
        Ok(RuntimeConfig {
            postgres_url: text(vault, &self.postgres_key)?,
            fdb_cluster_file: self.fdb_cluster_file.clone(),
            namespace: self.namespace.clone(),
            relay_cell: Some(self.source_cell.clone()),
            evidence_key,
            price: self.chat_price.clone(),
            gateway: GatewayConfig {
                base_url: endpoint.into(),
                endpoint_policy: EndpointPolicy::Https,
                model: self.chat_price.model.clone(),
                credential_ref: self.chat_price.credential_ref.clone(),
                api_key: text(vault, &self.chat_credential_key)?,
                limits: ModelLimits {
                    max_request_bytes: self
                        .chat_price
                        .max_request_bytes
                        .try_into()
                        .map_err(|_| "request bound")?,
                    max_response_bytes: self
                        .chat_price
                        .max_response_bytes
                        .try_into()
                        .map_err(|_| "response bound")?,
                    max_output_tokens: self.chat_price.max_output_tokens,
                    timeout: Duration::from_secs(90),
                    connect_timeout: Duration::from_secs(10),
                },
            },
        })
    }
    pub(super) fn host(&self) -> Result<ImageHostConfig, String> {
        Ok(ImageHostConfig {
            agent: self.agent.clone(),
            device: self.device.clone(),
            definition_version: self.definition_version.clone(),
            price: self.image_price.clone(),
            credential_key: self.image_credential_key.clone(),
            timeout: Duration::from_secs(90),
            connect_timeout: Duration::from_secs(10),
            target: ImageDeliveryTarget::configured(
                self.relay_url.clone(),
                self.relay_key.clone(),
                self.source_cell.clone(),
            )
            .map_err(|_| "invalid image route")?,
        })
    }
}
fn secret(vault: &FileVault, key: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    vault
        .load(key.into())
        .map(Zeroizing::new)
        .ok_or_else(|| "configured vault secret unavailable".into())
}
fn text(vault: &FileVault, key: &str) -> Result<String, String> {
    let bytes = secret(vault, key)?;
    std::str::from_utf8(&bytes)
        .map(str::to_owned)
        .map_err(|_| "configured vault secret malformed".into())
}

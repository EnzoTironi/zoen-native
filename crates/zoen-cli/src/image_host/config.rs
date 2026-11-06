//! Operator-only configuration. Content requests cannot select vault locators,
//! principals, prices, routing, continuity storage or deployment evidence.
use crate::file_vault::FileVault;
use crate::runtime_host_config::{read_private, Connection};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use zoen_models::EndpointPolicy;
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
    pub(super) fn enrollment_scope(&self) -> crate::provision::EnrollmentScope {
        crate::provision::EnrollmentScope {
            namespace: self.namespace.clone(),
            agent: self.agent.clone(),
            device: self.device.clone(),
            key_version: self.key_version,
        }
    }

    pub(super) fn read(path: &Path) -> Result<Self, String> {
        let config: Self = read_private(path)?;
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
        Connection {
            namespace: &self.namespace,
            source: &self.source_cell,
            fdb_cluster_file: self.fdb_cluster_file.clone(),
            postgres_key: &self.postgres_key,
            evidence_key: &self.evidence_key,
            credential_key: &self.chat_credential_key,
            price: &self.chat_price,
            endpoint_policy: EndpointPolicy::Https,
        }
        .runtime(vault)
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

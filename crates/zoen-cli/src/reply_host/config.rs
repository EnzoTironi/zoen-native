//! Operator-owned routing, pricing, device enrollment and policy configuration.
use crate::{
    file_vault::FileVault,
    gate_transport::ReqwestTransport,
    runtime_host_config::{read_private, text, Connection},
};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use zoen_models::EndpointPolicy;
use zoen_runtime::{ImageDeliveryTarget, PriceProfile, ReplyHostConfig, RuntimeConfig};

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
    pub chat_price: PriceProfile,
    pub chat_credential_key: String,
    #[serde(default)]
    pub chat_endpoint_policy: ChatEndpoint,
    pub gate: GateConfig,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ChatEndpoint {
    #[default]
    Https,
    LoopbackFixture,
}
#[derive(Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum GateConfig {
    Clef {
        account: String,
        credential_key: String,
    },
    /// Explicit operator acceptance of the existing offline policy floor.
    Offline,
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
        let value: Self = read_private(path)?;
        if !value.home.is_absolute()
            || !value.continuity.is_absolute()
            || value.home == value.continuity
        {
            return Err("use distinct absolute host and continuity directories".into());
        }
        value
            .chat_price
            .digest()
            .map_err(|_| "invalid trusted chat profile")?;
        if let GateConfig::Clef {
            account,
            credential_key,
        } = &value.gate
        {
            if account.len() != 32
                || !account.bytes().all(|b| b.is_ascii_hexdigit())
                || credential_key.is_empty()
            {
                return Err("invalid Clef operator configuration".into());
            }
        }
        Ok(value)
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
            endpoint_policy: match self.chat_endpoint_policy {
                ChatEndpoint::Https => EndpointPolicy::Https,
                ChatEndpoint::LoopbackFixture => EndpointPolicy::LoopbackFixture,
            },
        }
        .runtime(vault)
    }
    pub(super) fn host(&self) -> Result<ReplyHostConfig, String> {
        Ok(ReplyHostConfig {
            agent: self.agent.clone(),
            device: self.device.clone(),
            target: ImageDeliveryTarget::configured(
                self.relay_url.clone(),
                self.relay_key.clone(),
                self.source_cell.clone(),
            )
            .map_err(|_| "invalid reply route")?,
        })
    }
    pub(super) fn gate(&self, vault: &FileVault) -> Result<gate::Gate, String> {
        match &self.gate {
            GateConfig::Offline => Ok(gate::Gate::default()),
            GateConfig::Clef {
                account,
                credential_key,
            } => {
                let token = text(vault, credential_key)?;
                if token.is_empty() || token.len() > 4096 || token.chars().any(char::is_control) {
                    return Err("invalid Clef credential".into());
                }
                Ok(gate::Gate {
                    remote: Some(Box::new(gate::Clef {
                        account: account.clone(),
                        token,
                        model: gate::ClefModel::Flash,
                        transport: Arc::new(ReqwestTransport::new()?),
                    })),
                    ..gate::Gate::default()
                })
            }
        }
    }
}

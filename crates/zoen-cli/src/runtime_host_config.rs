//! Shared operator-only config IO and runtime construction. Never tool input.
use crate::file_vault::FileVault;
use client_core::SecretVault;
use serde::de::DeserializeOwned;
use std::{io::Read, path::Path, time::Duration};
use zeroize::Zeroizing;
use zoen_models::{EndpointPolicy, GatewayConfig, ModelLimits};
use zoen_runtime::{PriceProfile, RuntimeConfig};

pub(crate) fn read_private<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let before = std::fs::symlink_metadata(path).map_err(|_| "host configuration unavailable")?;
    if !before.is_file() || before.len() > 32768 {
        return Err("invalid host configuration file".into());
    }
    let file = std::fs::File::open(path).map_err(|_| "host configuration unavailable")?;
    let actual = file
        .metadata()
        .map_err(|_| "host configuration unavailable")?;
    if !actual.is_file() || actual.len() > 32768 {
        return Err("invalid host configuration file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != actual.dev()
            || before.ino() != actual.ino()
            || before.mode() & 0o077 != 0
            || actual.mode() & 0o077 != 0
        {
            return Err("host configuration must be a stable operator-private file (0600)".into());
        }
    }
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(32769)
        .read_to_end(&mut bytes)
        .map_err(|_| "host configuration unavailable")?;
    if bytes.len() > 32768 {
        return Err("host configuration too large".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "invalid host configuration".into())
}

pub(crate) fn secret(vault: &FileVault, key: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    vault
        .load(key.into())
        .map(Zeroizing::new)
        .ok_or_else(|| "configured vault secret unavailable".into())
}
pub(crate) fn text(vault: &FileVault, key: &str) -> Result<String, String> {
    let bytes = secret(vault, key)?;
    std::str::from_utf8(&bytes)
        .map(str::to_owned)
        .map_err(|_| "configured vault secret malformed".into())
}

pub(crate) struct Connection<'a> {
    pub namespace: &'a str,
    pub source: &'a str,
    pub fdb_cluster_file: Option<String>,
    pub postgres_key: &'a str,
    pub evidence_key: &'a str,
    pub credential_key: &'a str,
    pub price: &'a PriceProfile,
    pub endpoint_policy: EndpointPolicy,
}
impl Connection<'_> {
    pub(crate) fn runtime(&self, vault: &FileVault) -> Result<RuntimeConfig, String> {
        self.price
            .digest()
            .map_err(|_| "invalid trusted chat profile")?;
        let evidence = secret(vault, self.evidence_key)?;
        let evidence_key = evidence
            .as_slice()
            .try_into()
            .map_err(|_| "invalid evidence key")?;
        let endpoint = self
            .price
            .endpoint
            .strip_suffix("/chat/completions")
            .ok_or("invalid chat endpoint")?;
        Ok(RuntimeConfig {
            postgres_url: text(vault, self.postgres_key)?,
            fdb_cluster_file: self.fdb_cluster_file.clone(),
            namespace: self.namespace.into(),
            relay_cell: Some(self.source.into()),
            evidence_key,
            price: self.price.clone(),
            gateway: GatewayConfig {
                base_url: endpoint.into(),
                endpoint_policy: self.endpoint_policy,
                model: self.price.model.clone(),
                credential_ref: self.price.credential_ref.clone(),
                api_key: text(vault, self.credential_key)?,
                limits: ModelLimits {
                    max_request_bytes: self
                        .price
                        .max_request_bytes
                        .try_into()
                        .map_err(|_| "request bound")?,
                    max_response_bytes: self
                        .price
                        .max_response_bytes
                        .try_into()
                        .map_err(|_| "response bound")?,
                    max_output_tokens: self.price.max_output_tokens,
                    timeout: Duration::from_secs(90),
                    connect_timeout: Duration::from_secs(10),
                },
            },
        })
    }
}

#[cfg(test)]
#[path = "runtime_host_config/tests.rs"]
mod tests;

//! Operator-only single-host managed device bindings, outside restored SQL/FDB.
//! Nothing in a client request can select a directory, witness or deployment.
mod config;
pub use config::CONFIG_ENV;

use relay_admission::managed_images::{AnchorReader, Checkpoint, Deployment, GenerationGuard};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

#[derive(Clone)]
pub struct ManagedDevice {
    deployment: Deployment,
    anchor: AnchorReader,
}

#[derive(Clone, Default)]
pub struct ManagedDevices {
    devices: BTreeMap<String, Arc<ManagedDevice>>,
}

impl ManagedDevices {
    /// Trusted host configuration only; open never creates replacement history.
    pub fn open(
        configured: impl IntoIterator<Item = (Deployment, PathBuf)>,
    ) -> anyhow::Result<Self> {
        let mut registry = Self::default();
        let mut namespaces = std::collections::BTreeSet::new();
        for (deployment, directory) in configured {
            anyhow::ensure!(
                directory.is_absolute()
                    && protocol::runtime_binding::valid_namespace(&deployment.namespace)
                    && protocol::runtime_binding::valid_namespace(&deployment.source)
                    && deployment.device.len() == 64
                    && deployment
                        .device
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                "invalid managed device configuration"
            );
            anyhow::ensure!(
                !registry.devices.contains_key(&deployment.device)
                    && namespaces.insert(deployment.namespace.clone()),
                "duplicate managed device configuration"
            );
            let anchor = AnchorReader::open(&directory, deployment.clone())?;
            registry.devices.insert(
                deployment.device.clone(),
                Arc::new(ManagedDevice { deployment, anchor }),
            );
        }
        Ok(registry)
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// Read-only configured binding; this is not an effect permit.
    pub fn get(&self, device: &str) -> Option<Arc<ManagedDevice>> {
        self.devices.get(device).cloned()
    }

    pub(crate) fn source_matches(&self, cell: &str) -> bool {
        self.devices
            .values()
            .all(|entry| entry.deployment.source == cell)
    }
}

impl ManagedDevice {
    pub(crate) fn deployment(&self) -> &Deployment {
        &self.deployment
    }

    pub(crate) fn anchor(&self) -> &AnchorReader {
        &self.anchor
    }

    pub(crate) fn upload_guard(
        &self,
        expected: &Checkpoint,
    ) -> Result<GenerationGuard, &'static str> {
        self.anchor
            .generation_guard(expected)
            .map_err(|_| "managed generation refused")
    }
}

#[cfg(test)]
pub(crate) mod tests;

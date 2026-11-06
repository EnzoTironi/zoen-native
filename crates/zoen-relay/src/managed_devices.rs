//! Operator-only single-host managed device bindings, outside restored SQL/FDB.
//! Nothing in a client request can select a directory, witness or deployment.
mod config;
mod live;
mod runtimes;
pub use config::CONFIG_ENV;
pub use runtimes::{ManagedPrincipal, ManagedRuntime};

use relay_admission::managed_images::{AnchorReader, Checkpoint, Deployment, GenerationGuard};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, RwLock},
};

#[derive(Clone)]
pub struct ManagedDevice {
    deployment: Deployment,
    anchor: AnchorReader,
}

#[derive(Clone, Default)]
pub struct ManagedDevices {
    devices: Arc<RwLock<BTreeMap<String, Arc<ManagedDevice>>>>,
    live: Option<Arc<live::Source>>,
}

impl ManagedDevices {
    fn snapshot(&self) -> std::sync::RwLockReadGuard<'_, BTreeMap<String, Arc<ManagedDevice>>> {
        match self.devices.read() {
            Ok(map) => map,
            // Writers prepare and validate a whole replacement before the one
            // atomic assignment. A panic cannot leave a partially mutated map.
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Trusted host configuration only; open never creates replacement history.
    pub fn open(
        configured: impl IntoIterator<Item = (Deployment, PathBuf)>,
    ) -> anyhow::Result<Self> {
        Self::open_with_runtimes(configured, [])
    }

    /// Multiple explicitly certified principals can share the same independent
    /// runtime checkpoint. A namespace cannot mix device and runtime anchors.
    pub fn open_with_runtimes(
        configured: impl IntoIterator<Item = (Deployment, PathBuf)>,
        runtimes: impl IntoIterator<Item = ManagedRuntime>,
    ) -> anyhow::Result<Self> {
        let mut devices = BTreeMap::new();
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
                !devices.contains_key(&deployment.device)
                    && namespaces.insert(deployment.namespace.clone()),
                "duplicate managed device configuration"
            );
            let anchor = AnchorReader::open(&directory, deployment.clone())?;
            devices.insert(
                deployment.device.clone(),
                Arc::new(ManagedDevice { deployment, anchor }),
            );
        }
        Self::add_runtimes(&mut devices, runtimes, &mut namespaces)?;
        Ok(Self {
            devices: Arc::new(RwLock::new(devices)),
            live: None,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.snapshot().is_empty() && self.live.is_none()
    }

    /// Read-only configured binding; this is not an effect permit.
    pub fn get(&self, device: &str) -> Option<Arc<ManagedDevice>> {
        if let Some(found) = self.snapshot().get(device).cloned() {
            return Some(found);
        }
        if let Some(source) = &self.live {
            // A failed append keeps every previously admitted principal. A new
            // unadmitted device still cannot obtain a managed Runtime binding.
            let _ = source.refresh(self);
        }
        self.snapshot().get(device).cloned()
    }

    pub(crate) fn source_matches(&self, cell: &str) -> bool {
        self.snapshot()
            .values()
            .all(|entry| entry.deployment.source == cell)
            && self
                .live
                .as_ref()
                .is_none_or(|source| source.bind_source(cell))
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
            .historical_generation_guard(expected)
            .map_err(|_| "managed generation refused")
    }
}

#[cfg(test)]
pub(crate) mod tests;

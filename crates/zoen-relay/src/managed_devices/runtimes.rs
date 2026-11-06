//! Trusted runtime configuration shares one anchor across explicitly listed
//! principals. It cannot enroll a device, join a space, or grant a publication.
#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::perf,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::{AnchorReader, Deployment, ManagedDevice, ManagedDevices};
pub use relay_admission::managed_images::{ManagedPrincipal, ManagedRuntime};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

impl ManagedDevices {
    pub(super) fn add_runtimes(
        devices: &mut BTreeMap<String, Arc<ManagedDevice>>,
        configured: impl IntoIterator<Item = ManagedRuntime>,
        namespaces: &mut BTreeSet<String>,
    ) -> anyhow::Result<()> {
        for runtime in configured {
            let binding = &runtime.deployment;
            anyhow::ensure!(
                runtime.anchor_directory.is_absolute()
                    && protocol::runtime_binding::valid_namespace(&binding.namespace)
                    && protocol::runtime_binding::valid_namespace(&binding.source)
                    && namespaces.insert(binding.namespace.clone())
                    && !runtime.principals.is_empty()
                    && devices.len().saturating_add(runtime.principals.len()) <= 256,
                "invalid managed runtime configuration"
            );
            let anchor = AnchorReader::open_runtime(&runtime.anchor_directory, binding.clone())?;
            for principal in runtime.principals {
                anyhow::ensure!(
                    !principal.agent.is_empty()
                        && principal.agent.len() <= 256
                        && !principal.agent.chars().any(char::is_control)
                        && principal.device.len() == 64
                        && principal
                            .device
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                        && !devices.contains_key(&principal.device),
                    "invalid or duplicate managed runtime principal"
                );
                let deployment = Deployment {
                    namespace: binding.namespace.clone(),
                    witness: binding.witness.clone(),
                    source: binding.source.clone(),
                    agent: principal.agent,
                    device: principal.device,
                };
                devices.insert(
                    deployment.device.clone(),
                    Arc::new(ManagedDevice {
                        deployment,
                        anchor: anchor.clone(),
                    }),
                );
            }
        }
        Ok(())
    }
}

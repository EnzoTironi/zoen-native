//! One independent checkpoint domain for all principals sharing a runtime's
//! SQL/FDB pairing. Principal authorization still belongs to native custody
//! and the relay directory, never to these inert deployment locators.
#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::perf,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::state::{digest, Deployment};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeDeployment {
    pub namespace: String,
    pub witness: String,
    pub source: String,
}

impl std::fmt::Debug for RuntimeDeployment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RuntimeDeployment { private pairing redacted }")
    }
}

impl RuntimeDeployment {
    pub(super) fn valid(&self) -> bool {
        locator(&self.namespace) && locator(&self.source) && digest(&self.witness)
    }
}

/// The untagged legacy representation is deliberately preserved. Schema 2
/// remains device-scoped; schema 3 cannot be reopened through that constructor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum Binding {
    Device(Deployment),
    Runtime(RuntimeDeployment),
}

impl Binding {
    pub(super) fn valid(&self) -> bool {
        match self {
            Self::Device(deployment) => deployment.valid(),
            Self::Runtime(deployment) => deployment.valid(),
        }
    }

    pub(super) fn schema(&self) -> u8 {
        match self {
            Self::Device(_) => 2,
            Self::Runtime(_) => 3,
        }
    }

    pub(super) fn genesis_domain(&self) -> &'static str {
        match self {
            Self::Device(_) => "managed-images/genesis/1",
            Self::Runtime(_) => "runtime-continuity/genesis/1",
        }
    }

    pub(super) fn effect_domain(&self) -> &'static str {
        match self {
            Self::Device(_) => "managed-images/effect/1",
            Self::Runtime(_) => "runtime-continuity/effect/1",
        }
    }

    pub(super) fn namespace(&self) -> &str {
        match self {
            Self::Device(deployment) => &deployment.namespace,
            Self::Runtime(deployment) => &deployment.namespace,
        }
    }

    pub(super) fn source(&self) -> &str {
        match self {
            Self::Device(deployment) => &deployment.source,
            Self::Runtime(deployment) => &deployment.source,
        }
    }

    pub(super) fn principal(&self, agent: &str, device: &str) -> bool {
        match self {
            Self::Device(deployment) => deployment.agent == agent && deployment.device == device,
            Self::Runtime(_) => locator(agent) && locator(device),
        }
    }
}

fn locator(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

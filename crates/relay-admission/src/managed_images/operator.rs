//! Private operator routing configuration, outside SQL/FDB restore domains.
//! These locators never enroll an identity or authorize a financial effect.
use super::{Deployment, RuntimeDeployment};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorConfig {
    #[serde(default)]
    pub live: bool,
    pub devices: Vec<OperatorDevice>,
    #[serde(default)]
    pub runtimes: Vec<ManagedRuntime>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorDevice {
    pub deployment: Deployment,
    pub anchor_directory: PathBuf,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedPrincipal {
    pub agent: String,
    pub device: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedRuntime {
    pub deployment: RuntimeDeployment,
    pub anchor_directory: PathBuf,
    pub principals: Vec<ManagedPrincipal>,
}

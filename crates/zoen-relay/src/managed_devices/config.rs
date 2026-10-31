//! Read once from trusted operator startup. Never exposed as a request option.
use super::*;
use serde::Deserialize;
use std::{fs::File, io::Read, path::Path};

pub const CONFIG_ENV: &str = "ZOEN_MANAGED_DEVICE_CONFIG";
const MAX_CONFIG_BYTES: u64 = 65_536;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OperatorConfig {
    devices: Vec<OperatorDevice>,
    #[serde(default)]
    runtimes: Vec<ManagedRuntime>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OperatorDevice {
    deployment: Deployment,
    anchor_directory: PathBuf,
}

fn invalid() -> anyhow::Error {
    anyhow::anyhow!("invalid or unavailable managed device operator configuration")
}
fn private_file(path: &Path) -> anyhow::Result<File> {
    // Require a private regular file; never resolve client input against CWD.
    // Normal OS ancestor aliases (such as macOS /var) do not select a deployment.
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(invalid());
    }
    if !std::fs::symlink_metadata(path)
        .map_err(|_| invalid())?
        .is_file()
    {
        return Err(invalid());
    }
    let parent = path.parent().ok_or_else(invalid)?;
    let directory = std::fs::symlink_metadata(parent).map_err(|_| invalid())?;
    let file = File::open(path).map_err(|_| invalid())?;
    let metadata = file.metadata().map_err(|_| invalid())?;
    if !directory.is_dir() || !metadata.is_file() {
        return Err(invalid());
    }
    #[cfg(unix)]
    let private = {
        use std::os::unix::fs::PermissionsExt;
        directory.permissions().mode() & 0o077 == 0 && metadata.permissions().mode() & 0o077 == 0
    };
    #[cfg(not(unix))]
    let private = false;
    if !private {
        return Err(invalid());
    }
    Ok(file)
}

impl ManagedDevices {
    /// Only operator environment selects this file. Errors never echo its path,
    /// contents, deployment witness or a serde error that could include input.
    pub fn from_env() -> anyhow::Result<Self> {
        let configured = std::env::var_os(CONFIG_ENV).map(PathBuf::from);
        Self::load_operator_file(configured.as_deref())
    }

    pub fn load_operator_file(path: Option<&Path>) -> anyhow::Result<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let mut bytes = Vec::new();
        private_file(path)?
            .take(MAX_CONFIG_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid())?;
        if bytes.len() as u64 > MAX_CONFIG_BYTES {
            return Err(invalid());
        }
        let parsed: OperatorConfig = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if parsed.devices.len() > 256 || parsed.runtimes.len() > 256 {
            return Err(invalid());
        }
        Self::open_with_runtimes(
            parsed
                .devices
                .into_iter()
                .map(|entry| (entry.deployment, entry.anchor_directory)),
            parsed.runtimes,
        )
        .map_err(|_| invalid())
    }
}

#[cfg(test)]
mod tests;

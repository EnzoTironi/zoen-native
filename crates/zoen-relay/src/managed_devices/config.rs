//! Trusted operator startup and optional monotonic live admission.
use super::*;
use relay_admission::managed_images::OperatorConfig;
use std::{fs::File, io::Read, path::Path};

pub const CONFIG_ENV: &str = "ZOEN_MANAGED_DEVICE_CONFIG";
const MAX_CONFIG_BYTES: u64 = 65_536;

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
    let before = std::fs::symlink_metadata(path).map_err(|_| invalid())?;
    if !before.is_file() {
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
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        directory.permissions().mode() & 0o077 == 0
            && before.permissions().mode() & 0o077 == 0
            && metadata.permissions().mode() & 0o077 == 0
            && before.dev() == metadata.dev()
            && before.ino() == metadata.ino()
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
        let observed = super::live::Stamp::read(path)?;
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
        let live = parsed.live;
        let mut registry = Self::open_with_runtimes(
            parsed
                .devices
                .into_iter()
                .map(|entry| (entry.deployment, entry.anchor_directory)),
            parsed.runtimes,
        )
        .map_err(|_| invalid())?;
        if live {
            if super::live::Stamp::read(path)? != observed {
                return Err(invalid());
            }
            registry.live = Some(Arc::new(super::live::Source::new(path, observed)));
        }
        Ok(registry)
    }
}

#[cfg(test)]
mod tests;

//! Append trusted operator routing only after certified native initialization.
use crate::{
    agent_host_lock::HostLock, file_vault::FileVault, runtime_host_config::read_private_limit,
};
use relay_admission::managed_images::{
    Deployment, ManagedPrincipal, ManagedRuntime, OperatorConfig, RuntimeDeployment,
};
use std::path::Path;

pub(super) fn register(path: &Path, deployment: Deployment, anchor: &Path) -> Result<(), String> {
    if path.file_name().and_then(|name| name.to_str()) == Some("agent-host.lock") {
        return Err("operator admission file cannot be its lock".into());
    }
    let directory = path
        .parent()
        .ok_or("operator admission directory unavailable")?;
    let metadata = std::fs::symlink_metadata(directory)
        .map_err(|_| "operator admission directory unavailable")?;
    if !metadata.is_dir() {
        return Err("invalid operator admission directory".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("operator admission directory must be private".into());
        }
    }
    let _lock = HostLock::acquire(directory)?;
    let mut config: OperatorConfig = read_private_limit(path, 65536)?;
    if !append(&mut config, deployment, anchor)? {
        return Ok(());
    }
    let bytes =
        serde_json::to_vec(&config).map_err(|_| "operator admission encoding unavailable")?;
    if bytes.len() > 65536 {
        return Err("operator admission profile exceeded".into());
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid operator admission filename")?;
    FileVault {
        dir: directory.into(),
    }
    .write_bytes(name, &bytes)
    .map_err(|_| "operator admission save uncertain; resume retained initialization".into())
}

fn append(
    config: &mut OperatorConfig,
    deployment: Deployment,
    anchor: &Path,
) -> Result<bool, String> {
    if !config.live || !anchor.is_absolute() {
        return Err("live operator admission required".into());
    }
    let mut count = config.devices.len();
    if config.devices.iter().any(|entry| {
        entry.deployment.namespace == deployment.namespace
            || entry.deployment.device == deployment.device
    }) {
        return Err("operator admission conflicts with an existing device".into());
    }
    let binding = RuntimeDeployment {
        namespace: deployment.namespace,
        witness: deployment.witness,
        source: deployment.source,
    };
    let principal = ManagedPrincipal {
        agent: deployment.agent,
        device: deployment.device,
    };
    for runtime in &config.runtimes {
        count = count.saturating_add(runtime.principals.len());
        if let Some(existing) = runtime
            .principals
            .iter()
            .find(|entry| entry.device == principal.device)
        {
            if *existing == principal
                && runtime.deployment == binding
                && runtime.anchor_directory == anchor
            {
                return Ok(false);
            }
            return Err("operator principal cannot be reassigned".into());
        }
    }
    if count >= 256 || config.runtimes.len() > 256 {
        return Err("operator admission profile exceeded".into());
    }
    if let Some(runtime) = config
        .runtimes
        .iter_mut()
        .find(|entry| entry.deployment.namespace == binding.namespace)
    {
        if runtime.deployment != binding || runtime.anchor_directory != anchor {
            return Err("operator runtime binding cannot change".into());
        }
        runtime.principals.push(principal);
    } else {
        if config.runtimes.len() == 256 {
            return Err("operator admission profile exceeded".into());
        }
        config.runtimes.push(ManagedRuntime {
            deployment: binding,
            anchor_directory: anchor.into(),
            principals: vec![principal],
        });
    }
    Ok(true)
}

#[cfg(test)]
#[path = "admission/tests.rs"]
mod tests;

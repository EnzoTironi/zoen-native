//! Bounded trusted subprocess transport shared by model-content and typed controls.
use super::*;
use crate::image_protocol::{Command, MAX_INPUT, MAX_OUTPUT};
use std::{path::PathBuf, process::Stdio};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn paths() -> Result<(PathBuf, PathBuf), String> {
    let binary = PathBuf::from(env("ZOEN_IMAGE_HOST_BIN").ok_or("image host unavailable")?);
    let config =
        PathBuf::from(env("ZOEN_IMAGE_HOST_CONFIG").ok_or("image configuration unavailable")?);
    if !binary.is_absolute() || !config.is_absolute() {
        return Err("image host paths must be absolute".into());
    }
    Ok((binary, config))
}
pub(super) fn configured() -> bool {
    paths().is_ok()
}
pub(super) fn command_bytes(command: &Command) -> Result<Vec<u8>, String> {
    command.validate()?;
    let bytes = serde_json::to_vec(command).map_err(|_| "invalid image command")?;
    if bytes.len() > MAX_INPUT {
        return Err("image command too large".into());
    }
    Ok(bytes)
}
pub(super) async fn execute<T: serde::de::DeserializeOwned>(command: Command) -> Result<T, String> {
    let bytes = command_bytes(&command)?;
    let (binary, config) = paths()?;
    let mut process = tokio::process::Command::new(binary);
    process
        .arg("--config")
        .arg(config)
        .arg("execute")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    // Local dylib lookup is infrastructure, not content or a provider credential.
    for name in ["DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"] {
        if let Some(path) = std::env::var_os(name) {
            process.env(name, path);
        }
    }
    let mut child = process.spawn().map_err(|_| "image host could not start")?;
    let operation = async {
        let mut input = child.stdin.take().ok_or("image host input unavailable")?;
        input
            .write_all(&bytes)
            .await
            .map_err(|_| "image host input failed")?;
        drop(input);
        let output = child.stdout.take().ok_or("image host output unavailable")?;
        let mut bytes = Vec::new();
        output
            .take(MAX_OUTPUT as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "image host output failed")?;
        if bytes.len() > MAX_OUTPUT {
            return Err("image host response too large".into());
        }
        let status = child
            .wait()
            .await
            .map_err(|_| "image host status unavailable")?;
        if !status.success() {
            return Err(
                "image execution unavailable; original review and financial attempt retained"
                    .into(),
            );
        }
        serde_json::from_slice(&bytes).map_err(|_| "invalid image host response".into())
    };
    tokio::time::timeout(Duration::from_secs(150), operation)
        .await
        .map_err(|_| "image host response uncertain; recover the original review")?
}

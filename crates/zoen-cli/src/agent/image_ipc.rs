//! Bounded trusted subprocess transport shared by model-content and typed controls.
use super::*;
mod failure;
use crate::image_protocol::{Command, MAX_INPUT, MAX_OUTPUT};
pub(super) use failure::Failure;
use std::{path::PathBuf, process::Stdio};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(super) fn reply_paths() -> Result<Option<(PathBuf, PathBuf)>, String> {
    match (env("ZOEN_REPLY_HOST_BIN"), env("ZOEN_REPLY_HOST_CONFIG")) {
        (None, None) => Ok(None),
        (Some(binary), Some(config)) => {
            let (binary, config) = (PathBuf::from(binary), PathBuf::from(config));
            if !binary.is_absolute() || !config.is_absolute() {
                return Err("reply host paths must be absolute".into());
            }
            Ok(Some((binary, config)))
        }
        _ => Err("incomplete reply host configuration".into()),
    }
}
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
    matches!(reply_paths(), Ok(Some(_))) || paths().is_ok()
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
    execute_observed(command)
        .await
        .map_err(|failure| failure.message().into())
}

pub(super) async fn execute_observed<T: serde::de::DeserializeOwned>(
    command: Command,
) -> Result<T, Failure> {
    let bytes = command_bytes(&command).map_err(|_| Failure::Input)?;
    let account_control = match &command {
        Command::PolicyImport { .. } => true,
        Command::PolicyContext { signed, .. } => signed.inquiry.purpose.is_some(),
        _ => false,
    };
    let reply = if account_control {
        reply_paths().map_err(|_| Failure::Configuration)?
    } else {
        None
    };
    let is_reply = reply.is_some();
    let (binary, config) = match reply {
        Some(paths) => paths,
        None => paths().map_err(|_| Failure::Configuration)?,
    };
    run_process(
        binary,
        config,
        if is_reply { "owner-control" } else { "execute" },
        &bytes,
    )
    .await
}

pub(super) async fn reply_setup<T: serde::de::DeserializeOwned>(
    action: &str,
    bytes: &[u8],
) -> Result<T, String> {
    if bytes.len() > MAX_INPUT {
        return Err("reply setup too large".into());
    }
    let (binary, config) = reply_paths()?.ok_or("reply host unavailable")?;
    run_process(binary, config, action, bytes)
        .await
        .map_err(|e| e.message().into())
}

pub(super) fn process(binary: PathBuf, config: PathBuf, action: &str) -> tokio::process::Command {
    let mut process = tokio::process::Command::new(binary);
    process
        .arg("--config")
        .arg(config)
        .arg(action)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    for name in ["DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"] {
        if let Some(path) = std::env::var_os(name) {
            process.env(name, path);
        }
    }
    process
}

async fn run_process<T: serde::de::DeserializeOwned>(
    binary: PathBuf,
    config: PathBuf,
    action: &str,
    bytes: &[u8],
) -> Result<T, Failure> {
    let mut process = process(binary, config, action);
    let mut child = process.spawn().map_err(|_| Failure::Spawn)?;
    let operation = async {
        let mut input = child.stdin.take().ok_or(Failure::InputUnavailable)?;
        input
            .write_all(bytes)
            .await
            .map_err(|_| Failure::InputWrite)?;
        drop(input);
        let output = child.stdout.take().ok_or(Failure::OutputUnavailable)?;
        let mut bytes = Vec::new();
        output
            .take(MAX_OUTPUT as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| Failure::OutputRead)?;
        if bytes.len() > MAX_OUTPUT {
            return Err(Failure::OutputLimit);
        }
        let status = child.wait().await.map_err(|_| Failure::Wait)?;
        decode_response(status.success(), &bytes)
    };
    tokio::time::timeout(Duration::from_secs(150), operation)
        .await
        .map_err(|_| Failure::Timeout)?
}

fn decode_response<T: serde::de::DeserializeOwned>(
    success: bool,
    bytes: &[u8],
) -> Result<T, Failure> {
    if !success {
        return Err(Failure::NonzeroExit);
    }
    serde_json::from_slice(bytes).map_err(|_| Failure::Malformed)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_child_or_malformed_output_cannot_become_an_authenticated_policy_reply() {
        let refused = br#"{"status":"options_refused","inquiry_digest":"same"}"#;
        assert!(matches!(
            decode_response::<crate::image_protocol::PolicyProgress>(false, refused),
            Err(Failure::NonzeroExit)
        ));
        assert!(matches!(
            decode_response::<crate::image_protocol::PolicyProgress>(
                true,
                b"private-config/secret/not-json"
            ),
            Err(Failure::Malformed)
        ));
        let unknown = br#"{"status":"unavailable","operation":"context","code":"caller-controlled-private-text"}"#;
        assert!(matches!(
            decode_response::<crate::image_protocol::PolicyProgress>(true, unknown),
            Err(Failure::Malformed)
        ));
        assert!(matches!(
            decode_response::<crate::image_protocol::PolicyProgress>(true, refused),
            Ok(crate::image_protocol::PolicyProgress::OptionsRefused { .. })
        ));
    }
}

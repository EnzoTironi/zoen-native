//! Isolated production host. No Client/SQLite workspace is opened by this bin.
mod agent_host_lock;
#[path = "image_host/config.rs"]
mod config;
mod file_vault;
mod image_protocol;
#[path = "image_host/policy.rs"]
mod policy;
#[path = "image_host/provision.rs"]
mod provision;

use client_core::SecretVault;
use image_protocol::{Command, Progress, MAX_INPUT, MAX_OUTPUT};
use std::{
    io::{Read, Write},
    path::Path,
    sync::Arc,
};
use zoen_runtime::{
    ImageDeliveryProgress, ImageHostProgress, ImageReviewProgress, ImageRuntimeHost,
    RuntimeAuthority,
};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = if args.len() == 3 && args[0] == "--config" {
        run(Path::new(&args[1]), &args[2])
    } else {
        Err(
            "usage: zoen-image-host --config <private-config.json> provision|initialize|execute"
                .into(),
        )
    };
    if let Err(error) = result {
        eprintln!("image host: {error}");
        std::process::exit(1);
    }
}
fn run(path: &Path, action: &str) -> Result<(), String> {
    if !matches!(action, "provision" | "initialize" | "execute") {
        return Err("unsupported image host action".into());
    }
    let config = config::Config::read(path)?;
    let _lock = agent_host_lock::HostLock::acquire(&config.home)?;
    let vault = Arc::new(file_vault::FileVault {
        dir: config.home.join("vault"),
    });
    if action == "provision" {
        return provision::run(&config, &vault);
    }
    provision::validate_installed(&config, &vault)?;
    let command = if action == "execute" {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(MAX_INPUT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "image command unavailable")?;
        if bytes.len() > MAX_INPUT {
            return Err("image command too large".into());
        }
        let command: Command =
            serde_json::from_slice(&bytes).map_err(|_| "invalid image command")?;
        command.validate()?;
        let agent = match &command {
            Command::Stage { agent, .. }
            | Command::Advance { agent, .. }
            | Command::PolicyImport { agent, .. }
            | Command::PolicyContext { agent, .. } => agent,
        };
        if agent != &config.agent {
            return Err("image tool agent differs from configured principal".into());
        }
        Some(command)
    } else {
        None
    };
    let runtime_config = config.runtime(&vault)?;
    // SAFETY: this executable owns its only process-wide FDB network. It is
    // started once before any FDB calls and outlives the entire Tokio runtime.
    let network = unsafe { foundationdb::boot() };
    let executor = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|_| "image executor unavailable")?;
    let result = executor.block_on(async {
        let secrets: Arc<dyn SecretVault> = vault.clone();
        let authority = RuntimeAuthority::open_with_native_device(
            runtime_config,
            secrets.clone(),
            &config.agent,
            &config.device,
            config.key_version,
        )
        .await
        .map_err(|e| e.to_string())?;
        // The managed continuity constructor independently checks actual SQL,
        // FDB and retained local anchor; no Boolean can turn admission on.
        let authority = if action == "initialize" {
            match authority
                .inspect_native_device(&config.agent, &config.device)
                .await
            {
                Ok(_) => {
                    authority
                        .open_managed_image_continuity(
                            &config.continuity,
                            &config.agent,
                            &config.device,
                        )
                        .await
                }
                Err(zoen_runtime::RuntimeError::CoreAuthorityUnavailable) => {
                    authority
                        .initialize_managed_image_continuity(
                            &config.continuity,
                            &config.agent,
                            &config.device,
                        )
                        .await
                }
                Err(error) => return Err(error.to_string()),
            }
        } else {
            authority
                .open_managed_image_continuity(&config.continuity, &config.agent, &config.device)
                .await
        }
        .map_err(|e| e.to_string())?;
        if action == "initialize" {
            authority
                .initialize_native_device(&config.agent, &config.device)
                .await
                .map_err(|e| e.to_string())?;
            authority
                .prepare_native_key_packages(&config.agent, &config.device)
                .await
                .map_err(|e| e.to_string())?;
            let publication = authority
                .publish_native_key_packages(&config.agent, &config.device)
                .await
                .map_err(|e| e.to_string())?;
            if !publication.published {
                return Err("initial key-package publication not confirmed".into());
            }
            println!("native key packages published; invite this agent to a new conversation");
            return Ok(());
        }
        let host_config = config.host()?;
        let command = command.ok_or("image command missing")?;
        if matches!(
            &command,
            Command::PolicyImport { .. } | Command::PolicyContext { .. }
        ) {
            // Read/options and explicit owner-signed installation cannot mint an
            // Images or general output capability. Native directory checks and
            // the existing policy ledger still authorize the exact operation.
            let host = ImageRuntimeHost::configured(Arc::new(authority), secrets, host_config)
                .map_err(|e| e.to_string())?;
            return emit(&policy::execute(&host, command).await?);
        }
        let authority = authority
            .qualify_managed_images_output(host_config.target.clone())
            .await
            .map_err(|e| e.to_string())?;
        let host = ImageRuntimeHost::configured(Arc::new(authority), secrets, host_config)
            .map_err(|e| e.to_string())?;
        let progress = match command {
            Command::Stage {
                space,
                source,
                prompt,
                ..
            } => match host
                .stage(space, source, prompt)
                .await
                .map_err(|e| e.to_string())?
            {
                ImageReviewProgress::CatchingUp { .. } => Progress::CatchingUp,
                ImageReviewProgress::Published {
                    request,
                    content_hash,
                    ..
                } => Progress::Review {
                    request,
                    content_hash,
                },
            },
            Command::Advance { space, request, .. } => match host
                .advance(&space, &request)
                .await
                .map_err(|e| e.to_string())?
            {
                ImageHostProgress::CatchingUp { .. } => Progress::CatchingUp,
                ImageHostProgress::AwaitingRecovery { run, .. } => Progress::Recovery { run },
                ImageHostProgress::Output {
                    run,
                    output,
                    delivery,
                } => match delivery {
                    ImageDeliveryProgress::CatchingUp { .. } => Progress::CatchingUp,
                    ImageDeliveryProgress::Accepted { item, .. } => Progress::Delivered {
                        run,
                        item,
                        sha256: output.sha256,
                    },
                },
            },
            Command::PolicyImport { .. } | Command::PolicyContext { .. } => {
                return Err("owner control dispatch differs".into())
            }
        };
        emit(&progress)
    });
    drop(executor);
    drop(network);
    result
}

fn emit(value: &impl serde::Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "host response unavailable")?;
    if bytes.len() > MAX_OUTPUT {
        return Err("host response exceeded limit".into());
    }
    std::io::stdout()
        .write_all(&bytes)
        .map_err(|_| "host response unavailable".into())
}

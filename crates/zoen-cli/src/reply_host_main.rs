//! Dedicated certified native reply executor; never opens a Client MLS workspace.
#[path = "reply_host/admission.rs"]
mod admission;
mod agent_host_lock;
#[path = "reply_host/config.rs"]
mod config;
#[path = "reply_host/control.rs"]
mod control;
mod file_vault;
mod gate_transport;
#[path = "reply_host/owner_control.rs"]
mod owner_control;
mod owner_policy_protocol;
mod owner_policy_response;
#[path = "reply_host/protocol.rs"]
mod protocol;
#[path = "image_host/provision.rs"]
mod provision;
mod reply_enrollment_protocol;
mod runtime_host_config;
#[path = "reply_host/worker.rs"]
mod worker;
use client_core::SecretVault;
use std::{
    io::{BufRead, Read, Write},
    path::Path,
    sync::Arc,
};
use zoen_runtime::{ReplyRuntimeHost, RuntimeAuthority};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = if args.len() == 3 && args[0] == "--config" {
        run(Path::new(&args[1]), &args[2])
    } else {
        Err("usage: zoen-reply-host --config <private-config.json> provision|initialize|execute|serve|watch|control|owner-control".into())
    };
    if let Err(error) = result {
        eprintln!("reply host: {error}");
        std::process::exit(1)
    }
}
fn run(path: &Path, action: &str) -> Result<(), String> {
    if !matches!(
        action,
        "provision"
            | "prepare-managed"
            | "initialize"
            | "execute"
            | "serve"
            | "watch"
            | "supervised-watch"
            | "control"
            | "owner-control"
    ) {
        return Err("unsupported reply host action".into());
    }
    let mut config = config::Config::read(path)?;
    if action == "prepare-managed" {
        let request: reply_enrollment_protocol::Request = serde_json::from_slice(&read_command()?)
            .map_err(|_| "invalid managed enrollment request")?;
        if request.agent != config.agent || request.relay_url != config.relay_url {
            return Err("managed enrollment scope differs".into());
        }
        if !config.supervised {
            return emit(&reply_enrollment_protocol::Response::External);
        }
        let _lock = agent_host_lock::HostLock::acquire(&config.home)?;
        let vault = file_vault::FileVault {
            dir: config.home.join("vault"),
        };
        let code = provision::prepare(&config.enrollment_scope(), &vault)?;
        return emit(&reply_enrollment_protocol::Response::Prepared {
            code: code.encode(),
        });
    }
    if action == "owner-control" {
        return emit(&owner_control::execute(&config, &read_command()?)?);
    }
    if action == "control" {
        return emit(&control::request(&config.home, &read_command()?)?);
    }
    let _lock = agent_host_lock::HostLock::acquire(&config.home)?;
    let vault = Arc::new(file_vault::FileVault {
        dir: config.home.join("vault"),
    });
    if action == "provision" {
        return provision::run(&config.enrollment_scope(), &vault);
    }
    config.resolve_device(&vault)?;
    let runtime_config = config.runtime(&vault)?;
    // SAFETY: this executable starts exactly one FDB network before its first
    // database call. All authorities and the executor end before the guard.
    let network = unsafe { foundationdb::boot() };
    let executor = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|_| "reply executor unavailable")?;
    let result = executor.block_on(async {
        let secrets: Arc<dyn SecretVault> = vault.clone();
        let authority = RuntimeAuthority::open_with_native_device(
            runtime_config,
            secrets,
            &config.agent,
            &config.device,
            config.key_version,
        )
        .await
        .map_err(|e| e.to_string())?;
        let authority = if action == "initialize" {
            // initialize_runtime_continuity only accepts a fresh paired store or
            // the same interrupted genesis; it never adopts existing run state.
            match authority
                .initialize_runtime_continuity(&config.continuity)
                .await
            {
                Ok(authority) => Ok(authority),
                Err(zoen_runtime::RuntimeError::RestoreUnreconciled) => {
                    // A used store cannot be initialized. Reopen it only through
                    // the independent full continuity/inventory verification.
                    let secrets: Arc<dyn SecretVault> = vault.clone();
                    RuntimeAuthority::open_with_native_device(
                        config.runtime(&vault)?,
                        secrets,
                        &config.agent,
                        &config.device,
                        config.key_version,
                    )
                    .await
                    .map_err(|e| e.to_string())?
                    .open_runtime_continuity(&config.continuity)
                    .await
                }
                Err(error) => Err(error),
            }
        } else {
            authority.open_runtime_continuity(&config.continuity).await
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
            if !authority
                .publish_native_key_packages(&config.agent, &config.device)
                .await
                .map_err(|e| e.to_string())?
                .published
            {
                return Err("initial key-package publication not confirmed".into());
            }
            if let Some(path) = &config.admission_config {
                let deployment = authority
                    .native_deployment(&config.agent, &config.device)
                    .await
                    .map_err(|_| "native admission scope unavailable")?;
                admission::register(path, deployment, &config.continuity)?;
            }
            return emit(&serde_json::json!({"status":"initialized","device":config.device}));
        }
        let host_config = config.host()?;
        let gate = config.gate(&vault)?;
        let authority = Arc::new(
            authority
                .qualify_shared_reply_output(host_config.target.clone())
                .await
                .map_err(|e| e.to_string())?,
        );
        let host = ReplyRuntimeHost::configured(authority.clone(), host_config, gate)
            .map_err(|e| e.to_string())?;
        if matches!(action, "watch" | "supervised-watch") {
            return worker::run(&host, &config, &vault, action == "supervised-watch").await;
        }
        if action == "execute" {
            let bytes = read_command()?;
            return dispatch(&host, &bytes).await;
        }
        // The supervising worker controls waking; serving does not poll or
        // repeatedly evaluate held replies. Durable state remains in authority.
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        loop {
            let mut bytes = Vec::new();
            let read = (&mut input)
                .take(protocol::MAX_INPUT as u64 + 1)
                .read_until(b'\n', &mut bytes)
                .map_err(|_| "command unavailable")?;
            if read == 0 {
                return Ok(());
            }
            dispatch(&host, &bytes).await?;
        }
    });
    drop(executor);
    drop(network);
    result
}
fn read_command() -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(protocol::MAX_INPUT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "command unavailable")?;
    if bytes.len() > protocol::MAX_INPUT {
        return Err("reply command too large".into());
    }
    Ok(bytes)
}
async fn dispatch(host: &ReplyRuntimeHost, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > protocol::MAX_INPUT {
        return Err("reply command too large".into());
    }
    let command = serde_json::from_slice(bytes).map_err(|_| "invalid reply command")?;
    emit(&protocol::execute(host, command).await?)
}
fn emit(value: &impl serde::Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "reply progress unavailable")?;
    if bytes.len() > protocol::MAX_OUTPUT {
        return Err("reply progress too large".into());
    }
    let mut output = std::io::stdout().lock();
    output
        .write_all(&bytes)
        .and_then(|()| output.write_all(b"\n"))
        .and_then(|()| output.flush())
        .map_err(|_| "reply progress unavailable".into())
}

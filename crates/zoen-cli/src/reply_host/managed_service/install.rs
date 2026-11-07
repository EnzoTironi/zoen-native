use super::*;
use crate::{agent_host_lock::HostLock, provision::EnrollmentScope};
use client_core::Client;
use homes::Homes;
use serde::Deserialize;
use std::process::Stdio;
use zoen_runtime::ReplyRuntimeHost;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Principal {
    agent: String,
    device: String,
}

pub(super) async fn restore(
    authority: &RuntimeAuthority,
    config: &Config,
    pool: &PgPool,
    record: &Record,
) -> Result<(), String> {
    let homes = Homes::locate(config, record)?;
    let bytes = match (FileVault {
        dir: homes.executor.clone(),
    })
    .read_bytes("principal.json")
    {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("retained native principal unreadable".into()),
    };
    homes::private(&homes.executor)?;
    let principal: Principal =
        serde_json::from_slice(&bytes).map_err(|_| "invalid retained native principal")?;
    if principal.agent != record.agent || !::protocol::managed_agents::digest(&principal.device) {
        return Err("retained native principal differs".into());
    }
    let scope = EnrollmentScope {
        namespace: config.namespace.clone(),
        agent: principal.agent.clone(),
        device: principal.device.clone(),
        key_version: config.key_version,
    };
    crate::provision::validate_installed(&scope, &homes.executor_vault())?;
    let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE identity=$1 AND device=$2 AND revoked_at IS NULL)")
        .bind(&principal.agent).bind(&principal.device).fetch_one(pool).await.map_err(|_| "device directory unavailable")?;
    if active {
        authority
            .attach_native_device(homes.executor_vault(), &principal.agent, &principal.device)
            .await
            .map_err(|e| e.to_string())?;
    }
    // Revoked principals grant no effects. Continuity recovery must independently
    // refuse an unrecoverable pending intent; it must never silently discard it.
    Ok(())
}

struct Sync(Arc<Client>);
impl Drop for Sync {
    fn drop(&mut self) {
        self.0.stop_sync();
    }
}

pub(super) async fn activate(
    base: &Config,
    custody: &Arc<FileVault>,
    pool: &PgPool,
    authority: &mut Authority,
    record: &Record,
    homes: Homes,
) -> Result<jobs::Job, String> {
    let request = record.request.as_ref().ok_or("request")?;
    let account_lock = HostLock::acquire(&homes.sponsor)?;
    let vault = homes.account_vault();
    let client = Client::open(
        homes.sponsor.join("zoen.sqlite").to_string_lossy().into(),
        "en".into(),
    )
    .map_err(|_| "retained agent account unavailable")?;
    if !client
        .unlock(vault.clone())
        .map_err(|_| "retained agent keys unavailable")?
    {
        return Err("retained agent keys unavailable".into());
    }
    let account = client.account().ok_or("agent account")?;
    if account.identity_id != record.agent
        || !account.registered
        || client.agent_owner().as_deref() != Some(record.owner.as_str())
        || account.relay_url != base.relay_url
    {
        return Err("retained agent enrollment differs".into());
    }
    let sync = Sync(client.clone());
    client
        .start_sync(None)
        .map_err(|_| "agent sync unavailable")?;
    if client.wait_until_idle(15000).await.state != "online" {
        return Err("agent directory unavailable".into());
    }
    let executor_lock = HostLock::acquire(&homes.executor)?;
    let native_vault = homes.executor_vault();
    let mut config = base.clone();
    config.managed_service = None;
    config.agent = record.agent.clone();
    config.home = homes.executor.clone();
    let code = crate::provision::prepare_in_custody(
        &config.enrollment_scope(),
        &native_vault,
        Some(custody),
    )?;
    config.device = code.device.clone();
    homes::retain(
        &FileVault {
            dir: homes.executor.clone(),
        },
        "principal.json",
        &serde_json::to_vec(&Principal {
            agent: config.agent.clone(),
            device: config.device.clone(),
        })
        .map_err(|_| "principal encoding unavailable")?,
    )?;
    client
        .link_runtime_device(code.encode(), vault)
        .await
        .map_err(|_| "native device certification unavailable")?;
    authority
        .get()?
        .attach_native_device(native_vault.clone(), &config.agent, &config.device)
        .await
        .map_err(|e| e.to_string())?;
    authority
        .get()?
        .initialize_native_device(&config.agent, &config.device)
        .await
        .map_err(|e| e.to_string())?;
    authority
        .get()?
        .prepare_native_key_packages(&config.agent, &config.device)
        .await
        .map_err(|e| e.to_string())?;
    if !authority
        .get()?
        .publish_native_key_packages(&config.agent, &config.device)
        .await
        .map_err(|e| e.to_string())?
        .published
    {
        return Err("native key packages not confirmed".into());
    }
    let deployment = authority
        .get()?
        .native_deployment(&config.agent, &config.device)
        .await
        .map_err(|e| e.to_string())?;
    crate::admission::register(
        config
            .admission_config
            .as_ref()
            .ok_or("admission configuration")?,
        deployment,
        &config.continuity,
    )?;
    let shared = authority.qualify(&config).await?;
    let host = ReplyRuntimeHost::configured(shared, config.host()?, base.gate(custody)?)
        .map_err(|e| e.to_string())?;
    homes::retain(
        &FileVault {
            dir: homes.executor.clone(),
        },
        "reply-config.json",
        &serde_json::to_vec(&config).map_err(|_| "reply configuration encoding unavailable")?,
    )?;
    drop(sync);
    drop(client);
    drop(account_lock);
    let settings = base.managed_service.as_ref().ok_or("service settings")?;
    let binary = std::env::current_exe().map_err(|_| "reply executable unavailable")?;
    let companion = jobs::command(settings, &homes)
        .arg("agent-controls")
        .env("ZOEN_REPLY_HOST_BIN", binary)
        .env(
            "ZOEN_REPLY_HOST_CONFIG",
            homes.executor.join("reply-config.json"),
        )
        .env("ZOEN_CONTROL_PARENT_PIPE", "1")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|_| "owner control companion unavailable")?;
    let (send, receive) = tokio::sync::oneshot::channel();
    let worker_vault = custody.clone();
    let worker = tokio::task::spawn_local(async move {
        let _lock = executor_lock;
        crate::worker::run_ready(&host, &config, &worker_vault, false, None, Some(send)).await
    });
    let job = jobs::Job::Active { companion, worker };
    receive.await.map_err(|_| "native worker did not start")?;
    provision::host::registered(
        pool,
        &record.owner,
        request,
        &record.agent,
        &record.offer_digest,
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(job)
}

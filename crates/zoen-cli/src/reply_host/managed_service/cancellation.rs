//! The host, not a UI timeout or missing mailbox, decides whether consent won.
use super::*;
use crate::{agent_enrollment_vault::preflight_slots, agent_host_lock::HostLock};
use client_core::Client;

pub(super) async fn resolve(
    config: &Config,
    custody: &Arc<FileVault>,
    pool: &PgPool,
    authority: &mut Authority,
    record: &Record,
    allow_install: bool,
) -> Result<Option<jobs::Job>, String> {
    let homes = homes::Homes::prepare(config, record)?;
    let request = record.request.as_ref().ok_or("request")?;
    {
        let _lock = HostLock::acquire(&homes.sponsor)?;
        let private = FileVault {
            dir: homes.sponsor.join("vault"),
        };
        preflight_slots(&private)?;
        let vault = homes.account_vault();
        let client = Client::open(
            homes.sponsor.join("zoen.sqlite").to_string_lossy().into(),
            "en".into(),
        )
        .map_err(|_| "cancellation home unavailable")?;
        // Unlock an accepted account before canonical registration recovery.
        if client.account().is_some()
            && !client
                .unlock(vault.clone())
                .map_err(|_| "cancellation keys unavailable")?
        {
            return Err("cancellation keys unavailable".into());
        }
        if client
            .cancel_managed_agent_offer(record, &config.relay_url, vault.clone())
            .await
            .map_err(|_| "cancellation unconfirmed; enrollment retained")?
        {
            provision::cancel::confirmed(pool, &record.owner, request)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(None);
        }
        // The same operator command owns registration/definition recovery.
        // Release this home lock before handing its retained setup to that command.
    }
    if !allow_install {
        return Err("service capacity unavailable; consumed consent retained".into());
    }
    let settings = config.managed_service.as_ref().ok_or("settings")?;
    let mut child = jobs::command(settings, &homes)
        .arg("agent-enroll-wait")
        .arg("--timeout")
        .arg("20000")
        .env("ZOEN_ENROLLMENT_PARENT_PIPE", "1")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| "accepted enrollment recovery unavailable")?;
    // Child::wait closes its stdin by default. Hold this existing lifetime pipe
    // ourselves so a healthy parent does not accidentally retire the recovery.
    let _parent = child
        .stdin
        .take()
        .ok_or("enrollment lifetime pipe unavailable")?;
    if !child
        .wait()
        .await
        .map_err(|_| "accepted enrollment reap unavailable")?
        .success()
    {
        return Err("accepted enrollment recovery unconfirmed; retained setup preserved".into());
    }

    install::activate(config, custody, pool, authority, record, homes)
        .await
        .map(Some)
}

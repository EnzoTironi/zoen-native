//! Managed enrollment coordinates existing account and native-runtime contracts.
//! One process owns one custody/continuity writer; each agent keeps separate keys.
#[path = "managed_service/homes.rs"]
mod homes;
#[path = "managed_service/install.rs"]
mod install;
#[path = "managed_service/jobs.rs"]
mod jobs;
use crate::{
    config::{Config, ManagedServiceSettings as Settings},
    file_vault::FileVault,
};
use ::protocol::managed_agents::{Record, Service};
use client_core::SecretVault;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};
use zoen_relay::agents::provision;
use zoen_runtime::{RuntimeAuthority, RuntimeConfig};

fn service(config: &Config) -> Result<Service, String> {
    let settings = config
        .managed_service
        .as_ref()
        .ok_or("managed service settings required")?;
    let value = Service {
        id: settings.id.clone(),
        template_sha256: hex::encode(Sha256::digest(settings.definition.as_bytes())),
    };
    if !value.valid()
        || !config.agent.is_empty()
        || !config.device.is_empty()
        || config.supervised
        || config.admission_config.is_none()
        || !settings.zoen_binary.is_absolute()
        || settings.definition.len() > 1024
    {
        return Err("invalid managed service scope".into());
    }
    zoen_agentd::Definition::decode(settings.definition.as_bytes())
        .map_err(|_| "invalid managed service definition")?;
    homes::private(&config.home)?;
    Ok(value)
}

/// Bootstrap stays exclusively owned until its first successful route handshake.
struct Authority {
    bootstrap: Option<RuntimeAuthority>,
    live: Option<Arc<RuntimeAuthority>>,
}
impl Authority {
    fn get(&self) -> Result<&RuntimeAuthority, String> {
        self.live
            .as_deref()
            .or(self.bootstrap.as_ref())
            .ok_or_else(|| "service authority unavailable".into())
    }
    async fn qualify(&mut self, config: &Config) -> Result<Arc<RuntimeAuthority>, String> {
        let target = config.host()?.target;
        if let Some(live) = &self.live {
            live.qualify_shared_reply_device(target, &config.agent, &config.device)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(live.clone());
        }
        self.bootstrap
            .as_mut()
            .ok_or("service authority unavailable")?
            .qualify_shared_reply_output_in_place(target)
            .await
            .map_err(|e| e.to_string())?;
        let live = Arc::new(
            self.bootstrap
                .take()
                .ok_or("service authority unavailable")?,
        );
        self.live = Some(live.clone());
        Ok(live)
    }
}

pub(super) async fn run(
    config: Config,
    vault: Arc<FileVault>,
    runtime: RuntimeConfig,
    initialize: bool,
) -> Result<(), String> {
    let service = service(&config)?;
    homes::retain(
        &vault,
        "service-configuration.json",
        &serde_json::to_vec(&config).map_err(|_| "service configuration unavailable")?,
    )?;
    let pool = PgPool::connect(&runtime.postgres_url)
        .await
        .map_err(|_| "service directory unavailable")?;
    let secrets: Arc<dyn SecretVault> = vault.clone();
    let authority = RuntimeAuthority::open_native_service(runtime, secrets, config.key_version)
        .await
        .map_err(|e| e.to_string())?;
    if initialize {
        authority
            .initialize_runtime_continuity(&config.continuity)
            .await
            .map_err(|e| e.to_string())?;
        return crate::emit(&serde_json::json!({"status":"service_initialized"}));
    }
    // Attach retained valid credentials BEFORE recovering pending native intents.
    let mut after: Option<(String, String)> = None;
    loop {
        let records = provision::host::page(
            &pool,
            &service,
            after
                .as_ref()
                .map(|(owner, operation)| (owner.as_str(), operation.as_str())),
        )
        .await
        .map_err(|e| e.to_string())?;
        for record in &records {
            install::restore(&authority, &config, &pool, record).await?;
        }
        let Some(last) = records.last() else {
            break;
        };
        after = Some((
            last.owner.clone(),
            last.request.as_ref().ok_or("request")?.operation.clone(),
        ));
    }
    let authority = authority
        .open_runtime_continuity(&config.continuity)
        .await
        .map_err(|e| e.to_string())?;
    let mut authority = Authority {
        bootstrap: Some(authority),
        live: None,
    };
    let mut jobs = BTreeMap::<(String, String), jobs::Job>::new();
    provision::heartbeat(&pool, &service)
        .await
        .map_err(|e| e.to_string())?;
    let heartbeat = tokio::task::spawn_local(heartbeats(pool.clone(), service.clone()));
    let mut cursor: Option<(String, String)> = None;
    crate::emit(&serde_json::json!({"status":"service_ready", "service":service.id}))?;
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        if heartbeat.is_finished() {
            return Err("service heartbeat unavailable".into());
        }
        let records = provision::host::page(
            &pool,
            &service,
            cursor
                .as_ref()
                .map(|(owner, operation)| (owner.as_str(), operation.as_str())),
        )
        .await
        .map_err(|e| e.to_string())?;
        for record in &records {
            if !matches!(record.state.as_str(), "queued" | "offered" | "registered") {
                continue;
            }
            let request = record.request.as_ref().ok_or("request")?;
            let key = (record.owner.clone(), request.operation.clone());
            let registered = if let Some(job) = jobs.get_mut(&key) {
                match job.state()? {
                    jobs::State::Running => continue,
                    jobs::State::Registered => true,
                    jobs::State::Stopped => false,
                }
            } else {
                false
            };
            jobs.remove(&key);
            if jobs.len() >= 256 {
                continue;
            }
            match jobs::advance(&config, &vault, &pool, &mut authority, record, registered).await {
                Ok(Some(job)) => {
                    jobs.insert(key, job);
                }
                Ok(None) => {}
                Err(reason) => {
                    crate::emit(
                        &serde_json::json!({"status":"enrollment_deferred", "operation":request.operation, "reason":reason}),
                    )?;
                }
            }
        }
        cursor = records.last().map(|last| {
            (
                last.owner.clone(),
                last.request
                    .as_ref()
                    .map(|request| request.operation.clone())
                    .unwrap_or_default(),
            )
        });
        tokio::select! { _ = interval.tick() => {}, signal = tokio::signal::ctrl_c() => {
            signal.map_err(|_| "service signal unavailable")?; break;
        } }
    }
    drop(jobs);
    heartbeat.abort();
    let _ = heartbeat.await;
    pool.close().await;
    Ok(())
}

async fn heartbeats(pool: PgPool, service: Service) -> Result<(), provision::Error> {
    let mut timer = tokio::time::interval(Duration::from_secs(10));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        timer.tick().await;
        provision::heartbeat(&pool, &service).await?;
    }
}

#[cfg(test)]
#[path = "managed_service/tests.rs"]
mod tests;

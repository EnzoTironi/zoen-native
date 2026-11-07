use super::*;
use crate::agent_host_lock::HostLock;
use client_core::{AgentEnrollmentHostRequest, Client};
use std::process::Stdio;
use tokio::process::{Child, Command};

pub(super) enum State {
    Running,
    Registered,
    Stopped,
}
pub(super) enum Job {
    Enrollment(Child),
    Active {
        companion: Child,
        worker: tokio::task::JoinHandle<Result<(), String>>,
    },
}
impl Job {
    pub fn state(&mut self) -> Result<State, String> {
        match self {
            Self::Enrollment(child) => match child
                .try_wait()
                .map_err(|_| "enrollment process unavailable")?
            {
                None => Ok(State::Running),
                Some(status) if status.success() => Ok(State::Registered),
                Some(_) => Ok(State::Stopped),
            },
            Self::Active {
                companion, worker, ..
            } => {
                if worker.is_finished()
                    || companion
                        .try_wait()
                        .map_err(|_| "control companion unavailable")?
                        .is_some()
                {
                    Ok(State::Stopped)
                } else {
                    Ok(State::Running)
                }
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        if let Self::Active { worker, .. } = self {
            worker.abort();
        }
    }
}

pub(super) fn command(settings: &Settings, homes: &homes::Homes) -> Command {
    let mut command = Command::new(&settings.zoen_binary);
    command
        .arg("--home")
        .arg(&homes.sponsor)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    for name in ["DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"] {
        if let Some(path) = std::env::var_os(name) {
            command.env(name, path);
        }
    }
    command
}

pub(super) async fn advance(
    config: &Config,
    custody: &Arc<FileVault>,
    pool: &PgPool,
    authority: &mut Authority,
    record: &Record,
    registered: bool,
) -> Result<Option<Job>, String> {
    let homes = homes::Homes::prepare(config, record)?;
    let settings = config.managed_service.as_ref().ok_or("settings")?;
    let request = record.request.as_ref().ok_or("request")?;
    if record.state == "queued" {
        let _lock = HostLock::acquire(&homes.sponsor)?;
        let client = Client::open(
            homes.sponsor.join("zoen.sqlite").to_string_lossy().into(),
            "en".into(),
        )
        .map_err(|_| "enrollment account unavailable")?;
        let offer = client
            .prepare_agent_enrollment(
                AgentEnrollmentHostRequest {
                    expected_owner_id: record.owner.clone(),
                    relay_url: config.relay_url.clone(),
                    name: request.name.clone(),
                    handle: request.handle.clone(),
                    definition_json: settings.definition.clone(),
                },
                homes.account_vault(),
            )
            .map_err(|_| "original enrollment unavailable")?;
        let sealed = offer
            .seal_managed(&record.owner, request, &config.relay_url)
            .map_err(|_| "enrollment delivery unavailable")?;
        provision::host::retain_offer(
            pool,
            &record.owner,
            request,
            provision::host::Offer {
                agent: &sealed.agent,
                digest: &sealed.offer_digest,
                sealed: &sealed.sealed_offer,
                expires_ms: sealed.expires_ms,
            },
        )
        .await
        .map_err(|e| e.to_string())?;
        return Ok(None);
    }
    if record.state == "offered" && !registered {
        let child = command(settings, &homes)
            .arg("agent-enroll-wait")
            .arg("--timeout")
            .arg("900000")
            .env("ZOEN_ENROLLMENT_PARENT_PIPE", "1")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|_| "enrollment process unavailable")?;
        return Ok(Some(Job::Enrollment(child)));
    }
    install::activate(config, custody, pool, authority, record, homes)
        .await
        .map(Some)
}

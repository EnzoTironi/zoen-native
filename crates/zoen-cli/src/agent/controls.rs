//! Normal owner-control service, independent of content/model/editor workers.
//! Existing Core, IPC and Runtime gates remain the only authority for effects.
use super::{owner_policy, startup, Arc, Client, Duration, Instant, Readiness, Source};
use std::{collections::BTreeSet, path::Path};

pub(crate) const COMMAND: &str = "agent-controls";

pub(crate) fn validate_args(args: &[String]) -> Result<(), String> {
    let mut command = false;
    let mut flags = BTreeSet::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            COMMAND if !command => command = true,
            "--home" | "--timeout" | "--for" if flags.insert(arg) => {
                let value = args.next().ok_or("agent_controls_invalid_arguments")?;
                if value.is_empty() || value.starts_with("--") {
                    return Err("agent_controls_invalid_arguments".into());
                }
                if arg != "--home" {
                    let number = value
                        .parse::<u64>()
                        .ok()
                        .filter(|number| *number > 0)
                        .ok_or("agent_controls_positive_duration_required")?;
                    let duration = if arg == "--for" {
                        Duration::from_secs(number)
                    } else {
                        Duration::from_millis(number)
                    };
                    if Instant::now().checked_add(duration).is_none() {
                        return Err("agent_controls_duration_out_of_range".into());
                    }
                }
            }
            _ => return Err("agent_controls_invalid_arguments".into()),
        }
    }
    if !command {
        return Err("agent_controls_invalid_arguments".into());
    }
    Ok(())
}

pub(crate) fn preflight(command: &str, args: &[String], home: &Path) -> Result<(), String> {
    if command != COMMAND {
        // The legacy global parser leaves unknown leading options as `command`.
        // Refuse that malformed control invocation before it can create a home.
        if command.starts_with('-') && args.iter().any(|arg| arg == COMMAND) {
            return Err("agent_controls_invalid_arguments".into());
        }
        return Ok(());
    }
    validate_args(args)?;
    crate::agent_inspect::existing_home(home)
        .map_err(|_| "agent_controls_existing_home_required".into())
}

pub(crate) async fn dispatch(
    command: &str,
    client: &Arc<Client>,
    home: &Path,
    readiness: &Readiness<'_>,
    for_secs: Option<u64>,
    source: Option<Source>,
) -> Result<(), String> {
    match command {
        "agent-host" => super::host(client, home, readiness, for_secs, source).await,
        COMMAND if source.is_none() => host(client, readiness, for_secs).await,
        _ => Err("agent_controls_invalid_mode".into()),
    }
}

#[derive(PartialEq, Eq)]
struct Scope {
    agent: String,
    device: String,
    owner: String,
    relay: String,
}
impl Scope {
    fn current(client: &Client) -> Result<Self, String> {
        let account = client.account().ok_or("agent_controls_missing_account")?;
        if !account.registered || !account.unlocked || account.setup_pending {
            return Err("agent_controls_account_unavailable".into());
        }
        Ok(Self {
            agent: account.identity_id,
            device: account.device_id,
            owner: client
                .agent_owner()
                .ok_or("agent_controls_not_owned_agent")?,
            relay: account.relay_url,
        })
    }

    fn check(&self, client: &Client) -> Result<(), String> {
        if *self != Self::current(client)? {
            return Err("agent_controls_scope_changed".into());
        }
        Ok(())
    }

    async fn state(&self, client: &Client) -> Result<(), String> {
        self.check(client)?;
        let state = client
            .agent_state()
            .await
            .map_err(|_| "agent_controls_state_unavailable")?;
        self.check(client)?;
        if !matches!(state.status.as_str(), "active" | "paused")
            || !matches!(state.kind.as_str(), "User" | "Creator")
        {
            return Err("agent_controls_state_unavailable".into());
        }
        Ok(())
    }
}

pub(super) async fn host(
    client: &Arc<Client>,
    readiness: &Readiness<'_>,
    for_secs: Option<u64>,
) -> Result<(), String> {
    let scope = Scope::current(client)?;
    let duration = for_secs.map(Duration::from_secs);
    if duration.is_some_and(|duration| {
        duration.is_zero() || Instant::now().checked_add(duration).is_none()
    }) {
        return Err("agent_controls_duration_out_of_range".into());
    }
    client.stop_sync();
    client
        .start_sync(None)
        .map_err(|_| "agent_controls_sync_unavailable")?;
    // Read-only startup: never seed/advance content cursors or construct providers.
    startup::settled_cursors(client, 15_000).await?;
    scope.state(client).await?;
    readiness.publish()?;
    let mut worker = owner_policy::start(client, &scope.agent, &scope.device);
    let deadline = duration.and_then(|duration| Instant::now().checked_add(duration));
    let outcome: Result<(), String> = async {
        loop {
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Ok(());
            }
            scope.state(client).await?;
            let wait = deadline.map_or(Duration::from_secs(5), |deadline| {
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_secs(5))
            });
            tokio::time::sleep(wait).await;
        }
    }
    .await;
    // Complete cancellation before main drops the home lock or stops this channel.
    let stopped = worker.stop().await;
    outcome?;
    stopped
}

#[cfg(test)]
mod tests;

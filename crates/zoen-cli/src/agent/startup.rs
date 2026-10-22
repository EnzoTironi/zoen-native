//! Startup evidence for this one locked local host, never a liveness/authority record.
use crate::agent_host_lock::HostLock;
use client_core::Client;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

pub(crate) struct Readiness<'host> {
    path: PathBuf,
    _host: &'host HostLock,
}
impl<'host> Readiness<'host> {
    /// Clear only our known marker, after the OS home lock and before account/provider work.
    pub(crate) fn reset(home: &Path, host: &'host HostLock) -> Result<Self, String> {
        let path = home.join("agent-ready");
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("cannot clear stale agent readiness: {error}")),
        }
        Ok(Self { path, _host: host })
    }

    fn publish(&self) -> Result<(), String> {
        let mut options = File::options();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        // An empty marker has no payload or durable authority. Refuse replacement.
        options
            .open(&self.path)
            .map_err(|error| format!("cannot publish fresh agent readiness: {error}"))?;
        Ok(())
    }
}
impl Drop for Readiness<'_> {
    fn drop(&mut self) {
        // Abrupt process death can leave the filename; the next locked reset clears it.
        let _ = fs::remove_file(&self.path);
    }
}

fn current_head(client: &Client, space: String) -> Result<u64, String> {
    Ok(client
        .timeline(space)
        .map_err(|error| format!("cannot read initial agent timeline: {error}"))?
        .iter()
        .map(|entry| entry.seq)
        .max()
        .unwrap_or(0))
}

pub(super) async fn prepare(
    client: &Client,
    readiness: &Readiness<'_>,
    timeout_ms: u64,
) -> Result<BTreeMap<String, u64>, String> {
    client
        .wait_until_settled(timeout_ms)
        .await
        .map_err(|error| format!("agent sync did not settle: {error}"))?;
    let state = client
        .agent_state()
        .await
        .map_err(|error| error.to_string())?;
    if state.status == "revoked" {
        return Err("this agent was revoked by its owner".into());
    }
    let mut cursors: BTreeMap<String, u64> = state.cursors.into_iter().collect();
    let mut fresh = Vec::new();
    for space in client.spaces() {
        if cursors.contains_key(&space.id) {
            continue;
        }
        let head = current_head(client, space.id.clone())?;
        fresh.push((space.id, head));
    }
    for (space, head) in fresh {
        client
            .agent_advance(space.clone(), head)
            .await
            .map_err(|error| format!("initial agent cursor was not acknowledged: {error}"))?;
        cursors.insert(space, head);
    }
    readiness.publish()?;
    Ok(cursors)
}

#[cfg(test)]
mod tests;

//! Cached, short-circuit completion observations; diagnostics never grant readiness.

use super::{Client, ConnectionDto};
use serde::Serialize;
use std::{io::Write, time::Duration};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Check {
    Unknown,
    Uploads,
    Profiles,
    Mls,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ConnectionState {
    Offline,
    Connecting,
    Online,
    Unparsed,
}

#[derive(Clone, Serialize)]
pub(super) struct SyncCompletionObservation {
    state: ConnectionState,
    synced: bool,
    pending: u64,
    unknown_ready: Option<bool>,
    uploads_ready: Option<bool>,
    profiles_ready: Option<bool>,
    mls_ready: Option<bool>,
    #[serde(skip)]
    profile_settlement: Option<crate::profile::ProfileSettlement>,
}

impl SyncCompletionObservation {
    fn evaluate(connection: &ConnectionDto, mut check: impl FnMut(Check) -> bool) -> Self {
        let mut observed = Self {
            state: match connection.state.as_str() {
                "offline" => ConnectionState::Offline,
                "connecting" => ConnectionState::Connecting,
                "online" => ConnectionState::Online,
                _ => ConnectionState::Unparsed,
            },
            synced: connection.synced,
            pending: connection.pending,
            unknown_ready: None,
            uploads_ready: None,
            profiles_ready: None,
            mls_ready: None,
            profile_settlement: None,
        };
        if !matches!(observed.state, ConnectionState::Online)
            || !observed.synced
            || observed.pending != 0
        {
            return observed;
        }
        // Preserve the original && order and its unevaluated suffix. In particular,
        // do not re-read a predicate later just to describe a deadline decision.
        for (kind, result) in [
            (Check::Unknown, &mut observed.unknown_ready),
            (Check::Uploads, &mut observed.uploads_ready),
            (Check::Profiles, &mut observed.profiles_ready),
            (Check::Mls, &mut observed.mls_ready),
        ] {
            let ready = check(kind);
            *result = Some(ready);
            if !ready {
                break;
            }
        }
        observed
    }

    pub(super) fn satisfied(&self) -> bool {
        matches!(self.state, ConnectionState::Online)
            && self.synced
            && self.pending == 0
            && self.unknown_ready == Some(true)
            && self.uploads_ready == Some(true)
            && self.profiles_ready == Some(true)
            && self.mls_ready == Some(true)
    }

    fn write_deadline(&self, timeout_ms: u64, elapsed: Duration, writer: &mut impl Write) {
        if self.satisfied() {
            return;
        }
        #[derive(Serialize)]
        struct Deadline<'a> {
            event: &'static str,
            version: u8,
            timeout_ms: u64,
            elapsed_ms: u64,
            #[serde(flatten)]
            observed: &'a SyncCompletionObservation,
        }
        let record = Deadline {
            event: "zoen_sync_wait_deadline",
            version: 1,
            timeout_ms,
            elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
            observed: self,
        };
        if let Ok(bytes) = serde_json::to_vec(&record) {
            if bytes.len() < 2_048 && writer.write_all(&bytes).is_ok() {
                let _ = writer.write_all(b"\n");
            }
        }
    }

    fn write_profile_deadline(&self, timeout_ms: u64, elapsed: Duration, writer: &mut impl Write) {
        let Some(blocked_by) = self.profile_settlement.filter(|value| !value.satisfied()) else {
            return;
        };
        if self.profiles_ready != Some(false) {
            return;
        }
        #[derive(Serialize)]
        struct ProfileDeadline {
            event: &'static str,
            version: u8,
            timeout_ms: u64,
            elapsed_ms: u64,
            blocked_by: crate::profile::ProfileSettlement,
        }
        let record = ProfileDeadline {
            event: "zoen_profile_wait_deadline",
            version: 1,
            timeout_ms,
            elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
            blocked_by,
        };
        if let Ok(bytes) = serde_json::to_vec(&record) {
            if bytes.len() < 512 && writer.write_all(&bytes).is_ok() {
                let _ = writer.write_all(b"\n");
            }
        }
    }

    pub(super) fn trace_deadline(&self, timeout_ms: u64, elapsed: Duration) {
        if std::env::var_os("ZOEN_SYNC_WAIT_DIAGNOSTICS").is_some() {
            let mut stderr = std::io::stderr().lock();
            self.write_deadline(timeout_ms, elapsed, &mut stderr);
            self.write_profile_deadline(timeout_ms, elapsed, &mut stderr);
        }
    }
}

impl Client {
    /// Return the actual observation that ended this wait. A strict caller must
    /// not turn a later maintenance transition into a zero-budget second wait.
    pub(super) async fn wait_sync_completion(
        &self,
        timeout_ms: u64,
    ) -> (ConnectionDto, SyncCompletionObservation) {
        let started = tokio::time::Instant::now();
        let deadline = started + Duration::from_millis(timeout_ms);
        loop {
            let c = self.connection();
            let observed = self.sync_completion_observation(&c);
            if observed.satisfied() || tokio::time::Instant::now() >= deadline {
                if !observed.satisfied() {
                    observed.trace_deadline(timeout_ms, started.elapsed());
                }
                return (c, observed);
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    }

    pub(super) fn sync_completion_observation(
        &self,
        connection: &ConnectionDto,
    ) -> SyncCompletionObservation {
        // The same Engine is locked once, only after the connection preconditions
        // pass, exactly as in the original completion predicate.
        let mut engine = None;
        let mut profile_settlement = None;
        let mut observed = SyncCompletionObservation::evaluate(connection, |check| {
            let engine = engine.get_or_insert_with(|| self.lock());
            match check {
                Check::Unknown => engine.net.unknown.is_empty(),
                Check::Uploads => engine.uploads_pending() == 0,
                Check::Profiles => {
                    let profile = engine.profile_settlement();
                    profile_settlement = Some(profile);
                    profile.satisfied()
                }
                Check::Mls => engine.mls_settled(),
            }
        });
        observed.profile_settlement = profile_settlement;
        observed
    }

    #[cfg(test)]
    pub(super) fn sync_idle(&self, connection: &ConnectionDto) -> bool {
        self.sync_completion_observation(connection).satisfied()
    }
}

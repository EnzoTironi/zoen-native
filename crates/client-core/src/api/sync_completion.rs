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

    pub(super) fn trace_deadline(&self, timeout_ms: u64, elapsed: Duration) {
        if std::env::var_os("ZOEN_SYNC_WAIT_DIAGNOSTICS").is_some() {
            self.write_deadline(timeout_ms, elapsed, &mut std::io::stderr().lock());
        }
    }
}

impl Client {
    pub(super) fn sync_completion_observation(
        &self,
        connection: &ConnectionDto,
    ) -> SyncCompletionObservation {
        // The same Engine is locked once, only after the connection preconditions
        // pass, exactly as in the original completion predicate.
        let mut engine = None;
        SyncCompletionObservation::evaluate(connection, |check| {
            let engine = engine.get_or_insert_with(|| self.lock());
            match check {
                Check::Unknown => engine.net.unknown.is_empty(),
                Check::Uploads => engine.uploads_pending() == 0,
                Check::Profiles => engine.profiles_settled(),
                Check::Mls => engine.mls_settled(),
            }
        })
    }

    pub(super) fn sync_idle(&self, connection: &ConnectionDto) -> bool {
        self.sync_completion_observation(connection).satisfied()
    }
}

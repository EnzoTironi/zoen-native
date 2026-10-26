//! Existing quiet/backoff lifecycle, separate from provider and source recovery.
use super::{Arc, Client, CoreListener, Duration, Instant, Waker};
use client_core::CoreError;
use std::future::Future;

pub(super) struct HostConnection {
    backoff: Duration,
    offline_since: Option<Instant>,
    last_activity: Instant,
}
impl HostConnection {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            backoff: Duration::from_secs(1),
            offline_since: None,
            last_activity: now,
        }
    }
    pub(super) fn activity(&mut self, at: Instant) {
        self.last_activity = self.last_activity.max(at);
    }
    pub(super) fn maintain(&mut self, e: &Client, waker: &Arc<Waker>, now: Instant) {
        let online = *waker.online.lock().unwrap_or_else(|p| p.into_inner());
        if online == Some(false) {
            let since = *self.offline_since.get_or_insert(now);
            if now.saturating_duration_since(since) >= self.backoff {
                e.stop_sync();
                let _ = e.start_sync(Some(waker.clone() as Arc<dyn CoreListener>));
                self.offline_since = Some(now);
                self.backoff = (self.backoff * 2).min(Duration::from_secs(60));
            }
        } else {
            self.offline_since = None;
            self.backoff = Duration::from_secs(1);
            if now.saturating_duration_since(self.last_activity) > Duration::from_secs(10) {
                e.stop_sync();
                let _ = e.start_sync(Some(waker.clone() as Arc<dyn CoreListener>));
                self.last_activity = now;
            }
        }
    }
}

pub(super) async fn source_state<T>(
    client: &Client,
    deadline: Instant,
    read: impl Future<Output = Result<T, CoreError>>,
) -> Result<T, String> {
    // A freshly started Net is not authenticated yet. The protected RPC must
    // not be polled until normal connection/MLS/profile completion succeeds.
    // Both phases retain the caller's one absolute source deadline.
    let started = Instant::now();
    let Some(remaining) = deadline.checked_duration_since(started) else {
        return failure("settle", None, started);
    };
    let budget_ms =
        u64::try_from(remaining.as_millis()).map_err(|_| "agent_host_source_state_unavailable")?;
    let until = tokio::time::Instant::from_std(deadline);
    match tokio::time::timeout_at(until, client.wait_until_settled(budget_ms)).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => return failure("settle", Some(&error), started),
        Err(_) => return failure("settle", None, started),
    }
    // timeout_at may poll an immediately ready future at an expired deadline.
    // Refuse explicitly before any protected RPC can be submitted.
    if Instant::now() >= deadline {
        return failure("state_read", None, started);
    }
    match tokio::time::timeout_at(until, read).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => failure("state_read", Some(&error), started),
        Err(_) => failure("state_read", None, started),
    }
}

fn failure<T>(
    phase: &'static str,
    error: Option<&CoreError>,
    started: Instant,
) -> Result<T, String> {
    if std::env::var_os("ZOEN_SYNC_WAIT_DIAGNOSTICS").is_some() {
        let kind = match error {
            None => "deadline",
            Some(CoreError::Invalid { .. }) => "core_invalid",
            Some(CoreError::Stale { .. }) => "core_stale",
            Some(CoreError::Forbidden { .. }) => "core_forbidden",
            Some(CoreError::Storage { .. }) => "core_storage",
            Some(CoreError::NotFound { .. }) => "core_not_found",
            Some(CoreError::DraftRejected { .. }) => "core_draft_rejected",
        };
        let reported_tag = match error {
            Some(CoreError::Invalid { reason }) => match reason.as_str() {
                "offline" => Some("offline"),
                "complete_read_busy" => Some("complete_read_busy"),
                "complete_read_stale" => Some("complete_read_stale"),
                "complete_read_expired" => Some("complete_read_expired"),
                "invalid_complete_read" => Some("invalid_complete_read"),
                "complete_read_profile_exceeded" => Some("complete_read_profile_exceeded"),
                _ => None,
            },
            _ => None,
        };
        // Closed facts only: no source/request IDs, server text or credentials.
        super::say(serde_json::json!({
            "event":"zoen_agent_source_state_failure", "version":1,
            "phase":phase, "kind":kind, "reported_tag":reported_tag,
            "elapsed_ms":started.elapsed().as_millis().to_string()
        }));
    }
    Err("agent_host_source_state_unavailable".into())
}

//! Current profile request correlation and retry deadlines.

use super::*;

/// Only the current retry may consume a profile response or settle its deadline.
/// A timed-out request can still reply after the replacement was sent.
pub(super) fn remember_waiting(waiting: &mut HashMap<u64, Waiting>, id: u64, next: Waiting) {
    match &next {
        Waiting::SealedProfiles(_) => {
            waiting.retain(|_, entry| !matches!(entry, Waiting::SealedProfiles(_)))
        }
        Waiting::ProfileUploaded(_) => {
            waiting.retain(|_, entry| !matches!(entry, Waiting::ProfileUploaded(_)))
        }
        _ => {}
    }
    waiting.insert(id, next);
}

/// Free to send when nothing is in flight and any backoff has passed.
pub(super) fn free(slot: &mut Option<tokio::time::Instant>) -> bool {
    match slot {
        Some(t) if *t > tokio::time::Instant::now() => false,
        _ => {
            *slot = None;
            true
        }
    }
}

pub(super) fn settle(slot: &mut Option<tokio::time::Instant>, ok: bool) {
    *slot = (!ok).then(|| tokio::time::Instant::now() + Duration::from_secs(5));
}

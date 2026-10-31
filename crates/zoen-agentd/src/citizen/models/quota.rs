//! One process-local operational quota. Durable finance still belongs to Runtime.
#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::perf,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::CHEAP_CHAT_PROFILE;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use zoen_models::{Admission, AdmissionError, DispatchAuthority, DispatchRequest, Operation};

#[derive(Default)]
struct QuotaState {
    // Metadata digest only: no prompt, model response or provider credential.
    seen: BTreeMap<String, [u8; 32]>,
    used: BTreeMap<(String, u64), u32>,
}

/// Admits a frozen chat attempt once, within its owner's process-local UTC cap.
/// Restart continuity, price verification and financial admission are NOT provided here.
pub struct CheapTierAuthority {
    daily_cap: u32,
    state: Mutex<QuotaState>,
}

impl CheapTierAuthority {
    #[must_use]
    pub fn new(daily_cap: u32) -> Self {
        Self {
            daily_cap,
            state: Mutex::default(),
        }
    }

    fn admit_at(&self, r: &DispatchRequest, now: SystemTime) -> Result<Admission, AdmissionError> {
        if r.profile != CHEAP_CHAT_PROFILE
            || r.descriptor.operation != Operation::ChatCompletion
            || r.descriptor.image.is_some()
        {
            return Err(AdmissionError::Denied);
        }
        let encoded = serde_json::to_vec(&(
            "zoen.staging-chat-admission.v1",
            &r.context,
            &r.request_digest,
            r.profile,
            &r.descriptor,
        ))
        .map_err(|_| AdmissionError::Unavailable)?;
        let binding: [u8; 32] = Sha256::digest(encoded).into();
        let mut state = self.state.lock().map_err(|_| AdmissionError::Unavailable)?;
        if let Some(original) = state.seen.get(&r.context.attempt_id) {
            return if *original == binding {
                Ok(Admission::AlreadyKnown)
            } else {
                Err(AdmissionError::Denied)
            };
        }
        // An invalid clock must never manufacture a valid epoch-zero quota.
        let day = now
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AdmissionError::Unavailable)?
            .as_secs()
            / 86_400;
        let key = (r.context.owner.clone(), day);
        let used = state.used.get(&key).copied().unwrap_or_default();
        if used >= self.daily_cap {
            return Err(AdmissionError::Denied);
        }
        let next = used.checked_add(1).ok_or(AdmissionError::Unavailable)?;
        state.used.insert(key, next);
        // Only a fresh admitted attempt becomes known. Refusal is not dispatch evidence.
        state.seen.insert(r.context.attempt_id.clone(), binding);
        Ok(Admission::Fresh)
    }
}

#[async_trait]
impl DispatchAuthority for CheapTierAuthority {
    async fn admit(&self, r: &DispatchRequest) -> Result<Admission, AdmissionError> {
        self.admit_at(r, SystemTime::now())
    }
}

#[cfg(test)]
mod tests;

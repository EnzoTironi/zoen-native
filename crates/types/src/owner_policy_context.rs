//! Read-only policy discovery, bound to an owner's exact pending image review.
use crate::owner_budget::{digest_id, SignedOwnerPolicy};
use serde::{Deserialize, Serialize};
pub const CONTEXT_TTL_MS: i64 = 5 * 60 * 1000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerPolicyInquiry {
    pub id: String,
    pub owner: String,
    pub agent: String,
    pub space: String,
    pub request: String,
    pub content_hash: String,
    pub price_digest: String,
    pub at_ms: i64,
}
fn locator(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
impl OwnerPolicyInquiry {
    pub fn valid_shape(&self) -> bool {
        locator(&self.id)
            && digest_id(&self.owner)
            && digest_id(&self.agent)
            && self.owner != self.agent
            && locator(&self.space)
            && locator(&self.request)
            && digest_id(&self.content_hash)
            && digest_id(&self.price_digest)
            && self.at_ms > 0
    }
    pub fn fresh_at(&self, at_ms: i64) -> bool {
        self.valid_shape()
            && at_ms > 0
            && self.at_ms <= at_ms.saturating_add(30_000)
            && at_ms.saturating_sub(self.at_ms) <= CONTEXT_TTL_MS
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedOwnerPolicyInquiry {
    pub inquiry: OwnerPolicyInquiry,
    pub device: String,
    pub cert: String,
    pub signature: String,
}

/// Internal Rust wire. UI consumers must project i64 values to decimal strings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerPolicyOptions {
    pub inquiry_digest: String,
    pub currency: String,
    pub scale: u8,
    pub reservation_units: i64,
    pub image_profile_digest: String,
    pub observed_at_ms: i64,
    pub year: i32,
    pub month: u8,
    pub current_policy: Option<SignedOwnerPolicy>,
    pub current_policy_digest: Option<String>,
}
impl OwnerPolicyOptions {
    pub fn valid_shape(&self) -> bool {
        digest_id(&self.inquiry_digest)
            && self.currency == "USD"
            && self.scale == 6
            && self.reservation_units >= 0
            && digest_id(&self.image_profile_digest)
            && self.observed_at_ms > 0
            && (2000..=2200).contains(&self.year)
            && (1..=12).contains(&self.month)
            && match (&self.current_policy, &self.current_policy_digest) {
                (None, None) => true,
                (Some(signed), Some(digest)) => signed.policy.valid_shape() && digest_id(digest),
                _ => false,
            }
    }
}

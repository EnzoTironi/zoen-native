//! Read-only discovery for an exact image review or explicit private account management.
use crate::owner_budget::{digest_id, SignedOwnerPolicy};
use serde::{Deserialize, Serialize};
pub const CONTEXT_TTL_MS: i64 = 5 * 60 * 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerPolicyInquiryPurpose {
    AccountBudget,
}

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
    // None is omitted to preserve the existing image inquiry's signed bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<OwnerPolicyInquiryPurpose>,
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
    #[must_use]
    pub fn valid_shape(&self) -> bool {
        locator(&self.id)
            && digest_id(&self.owner)
            && digest_id(&self.agent)
            && self.owner != self.agent
            && locator(&self.space)
            && match self.purpose {
                None => {
                    locator(&self.request)
                        && digest_id(&self.content_hash)
                        && digest_id(&self.price_digest)
                }
                Some(OwnerPolicyInquiryPurpose::AccountBudget) => {
                    self.request.is_empty()
                        && self.content_hash.is_empty()
                        && self.price_digest.is_empty()
                }
            }
            && self.at_ms > 0
    }
    #[must_use]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerPolicyImageSpec {
    pub size: String,
    pub quality: String,
}
impl OwnerPolicyImageSpec {
    #[must_use]
    pub fn valid_shape(&self) -> bool {
        ["1024x1024", "1536x1024", "1024x1536"].contains(&self.size.as_str())
            && ["low", "medium", "high"].contains(&self.quality.as_str())
    }
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
    // Management has no Request definition; only the actual host supplies this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_spec: Option<OwnerPolicyImageSpec>,
}
impl OwnerPolicyOptions {
    pub fn valid_shape(&self) -> bool {
        digest_id(&self.inquiry_digest)
            && self.currency == "USD"
            && self.scale == 6
            && self.reservation_units >= 0
            && digest_id(&self.image_profile_digest)
            && self.observed_at_ms > 0
            && self
                .image_spec
                .as_ref()
                .is_none_or(OwnerPolicyImageSpec::valid_shape)
            && (2000..=2200).contains(&self.year)
            && (1..=12).contains(&self.month)
            && match (&self.current_policy, &self.current_policy_digest) {
                (None, None) => true,
                (Some(signed), Some(digest)) => signed.policy.valid_shape() && digest_id(digest),
                _ => false,
            }
    }
}

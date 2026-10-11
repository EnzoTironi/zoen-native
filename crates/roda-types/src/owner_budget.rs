//! Explicit owner policy; the legacy MonthlyBudget projection cannot issue it.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerPeriodPolicy {
    pub owner: String,
    pub year: i32,
    pub month: u8,
    pub version: u32,
    pub previous_digest: Option<String>,
    pub currency: String,
    pub scale: u8,
    pub limit_units: i64,
    pub max_attempt_units: i64,
    pub expires_at_ms: i64,
    pub enabled: bool,
    pub allowed_profiles: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedOwnerPolicy {
    pub policy: OwnerPeriodPolicy,
    pub device: String,
    pub cert: String,
    pub signature: String,
}

pub fn digest_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

impl OwnerPeriodPolicy {
    pub fn valid_shape(&self) -> bool {
        digest_id(&self.owner)
            && (2000..=2200).contains(&self.year)
            && (1..=12).contains(&self.month)
            && self.version > 0
            && match (&self.previous_digest, self.version) {
                (None, 1) => true,
                (Some(previous), 2..) => digest_id(previous),
                _ => false,
            }
            && self.currency == "USD"
            && self.scale == 6
            && self.limit_units >= 0
            && self.max_attempt_units >= 0
            && self.max_attempt_units <= self.limit_units
            && self.expires_at_ms > 0
            && !self.allowed_profiles.is_empty()
            && self.allowed_profiles.len() <= 16
            && self.allowed_profiles.iter().all(|p| digest_id(p))
            && self.allowed_profiles.windows(2).all(|w| w[0] < w[1])
    }
}

//! Currency, period and version come from the actual authenticated Runtime reply.
use super::*;
use chrono::{TimeZone, Utc};
fn units(value: &str) -> R<i64> {
    let parsed: i64 = value.parse().map_err(|_| invalid())?;
    if parsed < 0 || parsed.to_string() != value {
        return Err(invalid());
    }
    Ok(parsed)
}
impl Engine {
    pub(crate) fn prepare_owner_policy(
        &self,
        scope: &OwnerPolicyScope,
        source: &str,
        expected_options: &str,
        limit: &str,
        max_attempt: &str,
        enabled: bool,
    ) -> R<OwnerPolicyReview> {
        let (_, options) = self.owner_policy_options_record(scope, source)?;
        let options = options.ok_or_else(denied)?;
        if options_digest(&options)? != expected_options {
            return Err(denied());
        }
        let limit_units = units(limit)?;
        let max_attempt_units = units(max_attempt)?;
        if enabled
            && (max_attempt_units < options.reservation_units
                || limit_units < options.reservation_units)
        {
            return Err(invalid());
        }
        let current = options.current_policy.as_ref().map(|signed| &signed.policy);
        let mut allowed_profiles =
            current.map_or_else(Vec::new, |policy| policy.allowed_profiles.clone());
        allowed_profiles.push(options.image_profile_digest.clone());
        allowed_profiles.sort();
        allowed_profiles.dedup();
        let (year, month) = if options.month == 12 {
            (options.year.checked_add(1).ok_or_else(invalid)?, 1)
        } else {
            (options.year, u32::from(options.month) + 1)
        };
        let expires = Utc
            .with_ymd_and_hms(year, month, 1, 0, 0, 0)
            .single()
            .ok_or_else(invalid)?
            .timestamp_millis();
        let policy = OwnerPeriodPolicy {
            owner: scope.expected_identity_id.clone(),
            year: options.year,
            month: options.month,
            version: match current {
                Some(policy) => policy.version.checked_add(1).ok_or_else(invalid)?,
                None => 1,
            },
            previous_digest: options.current_policy_digest.clone(),
            currency: options.currency,
            scale: options.scale,
            limit_units,
            max_attempt_units,
            expires_at_ms: expires,
            enabled,
            allowed_profiles,
        };
        self.review_owner_policy(
            scope,
            &serde_json::to_string(&policy).map_err(|_| invalid())?,
        )
    }
}
impl Client {
    pub fn prepare_owner_policy(
        &self,
        scope: OwnerPolicyScope,
        source: String,
        options_digest: String,
        limit_units: String,
        max_attempt_units: String,
        enabled: bool,
    ) -> R<OwnerPolicyReview> {
        self.lock().prepare_owner_policy(
            &scope,
            &source,
            &options_digest,
            &limit_units,
            &max_attempt_units,
            enabled,
        )
    }
}

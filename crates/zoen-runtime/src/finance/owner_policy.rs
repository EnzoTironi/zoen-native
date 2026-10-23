//! Exact owner controls under the existing native directory transaction.
use super::*;
use chrono::Datelike;
use types::owner_policy_context::{OwnerPolicyOptions, SignedOwnerPolicyInquiry};

impl NativeDirectoryGuard {
    pub(crate) async fn install_owner_policy(
        mut self,
        signed: SignedOwnerPolicy,
    ) -> Result<crate::OwnerPolicyInstallation, RuntimeError> {
        if signed.policy.owner != self.owner {
            return Err(RuntimeError::Denied);
        }
        if !event_log::owner_budget::verify(&signed) {
            return Err(RuntimeError::InvalidPolicy);
        }
        let digest =
            event_log::owner_budget::digest(&signed.policy).ok_or(RuntimeError::InvalidPolicy)?;
        if let Some(exact) = self.observed_policy(&signed, &digest).await? {
            self.tx.commit().await?;
            return Ok(if exact {
                crate::OwnerPolicyInstallation::Installed(digest)
            } else {
                crate::OwnerPolicyInstallation::Refused(digest)
            });
        }
        match Finance::install_policy_in(&mut self.tx, signed.clone()).await {
            Ok(actual) => {
                self.tx.commit().await?;
                Ok(crate::OwnerPolicyInstallation::Installed(actual))
            }
            Err(
                RuntimeError::InvalidPolicy
                | RuntimeError::InvalidBinding
                | RuntimeError::Denied
                | RuntimeError::InvalidProfile,
            ) => {
                // A concurrent known commit may have become visible while live
                // validation refused (e.g. expiry). Reobserve before reporting.
                let exact = self.observed_policy(&signed, &digest).await? == Some(true);
                // Never commit speculative period writes from refused setup.
                self.tx.rollback().await?;
                Ok(if exact {
                    crate::OwnerPolicyInstallation::Installed(digest)
                } else {
                    crate::OwnerPolicyInstallation::Refused(digest)
                })
            }
            Err(error) => Err(error),
        }
    }

    async fn observed_policy(
        &mut self,
        signed: &SignedOwnerPolicy,
        digest: &str,
    ) -> Result<Option<bool>, RuntimeError> {
        let policy = &signed.policy;
        let (start, _) = period(policy.year, policy.month)?;
        let row: Option<(String, Json<SignedOwnerPolicy>)> = sqlx::query_as(
            "SELECT digest,signed FROM runtime_policies WHERE owner=$1 AND period_start=$2 AND version=$3 FOR SHARE",
        ).bind(&self.owner).bind(start).bind(i64::from(policy.version))
            .fetch_optional(&mut *self.tx).await?;
        let Some((stored_digest, Json(stored))) = row else {
            return Ok(None);
        };
        if !event_log::owner_budget::verify(&stored)
            || stored.policy.owner != self.owner
            || stored.policy.year != policy.year
            || stored.policy.month != policy.month
            || stored.policy.version != policy.version
            || event_log::owner_budget::digest(&stored.policy).as_deref() != Some(&stored_digest)
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        if stored_digest == digest && stored.policy != *policy {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        // Historical acknowledgement only, under CURRENT native directory locks.
        // Expiry/disabled latest never becomes usable through this read.
        Ok(Some(stored_digest == digest))
    }

    /// A read does not install a policy, reserve money, or admit dispatch.
    pub(crate) async fn owner_policy_options(
        mut self,
        signed: &SignedOwnerPolicyInquiry,
        price: &crate::ImagePriceProfile,
    ) -> Result<OwnerPolicyOptions, RuntimeError> {
        let inquiry = &signed.inquiry;
        let profile_digest = price.digest()?;
        if inquiry.owner != self.owner
            || inquiry.agent != self.agent
            || (inquiry.purpose.is_none() && inquiry.price_digest != profile_digest)
            || !event_log::owner_policy_inquiry::verify(signed)
        {
            return Err(RuntimeError::Denied);
        }
        Finance::device(&mut self.tx, &inquiry.owner, &signed.device, &signed.cert).await?;
        Finance::person(&mut self.tx, &inquiry.owner).await?;
        let now = Finance::clock(&mut self.tx).await?;
        if !inquiry.fresh_at(now) {
            return Err(RuntimeError::Denied);
        }
        let date = chrono::DateTime::from_timestamp_millis(now).ok_or(RuntimeError::Unavailable)?;
        let year = date.year();
        let month = u8::try_from(date.month()).map_err(|_| RuntimeError::Unavailable)?;
        let (start, end) = period(year, month)?;
        let row: Option<(i64, String, Json<SignedOwnerPolicy>)> = sqlx::query_as(
            "SELECT version,digest,signed FROM runtime_policies WHERE owner=$1 AND period_start=$2 ORDER BY version DESC LIMIT 1 FOR SHARE",
        )
        .bind(&inquiry.owner)
        .bind(start)
        .fetch_optional(&mut *self.tx)
        .await?;
        let (current_policy, current_policy_digest) =
            if let Some((version, digest, Json(policy))) = row {
                let p = &policy.policy;
                if !event_log::owner_budget::verify(&policy)
                    || p.owner != inquiry.owner
                    || p.year != year
                    || p.month != month
                    || i64::from(p.version) != version
                    || p.expires_at_ms <= start
                    || p.expires_at_ms > end
                    || event_log::owner_budget::digest(p).as_deref() != Some(&digest)
                {
                    return Err(RuntimeError::InvalidPolicy);
                }
                // Retain disabled/expired latest versions. Absence is not a reset
                // of the version chain, and historical Installed is not active.
                (Some(policy), Some(digest))
            } else {
                (None, None)
            };
        fn image_setting(value: impl serde::Serialize) -> Result<String, RuntimeError> {
            serde_json::to_value(value)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .ok_or(RuntimeError::InvalidProfile)
        }
        let image_spec = if inquiry.purpose.is_some() {
            Some(types::owner_policy_context::OwnerPolicyImageSpec {
                size: image_setting(price.image.size)?,
                quality: image_setting(price.image.quality)?,
            })
        } else {
            None
        };
        let options = OwnerPolicyOptions {
            inquiry_digest: event_log::owner_policy_inquiry::digest(inquiry)
                .ok_or(RuntimeError::InvalidBinding)?,
            currency: price.currency.clone(),
            scale: price.scale,
            reservation_units: price.reservation_units,
            image_profile_digest: profile_digest,
            observed_at_ms: now,
            year,
            month,
            current_policy,
            current_policy_digest,
            image_spec,
        };
        if !options.valid_shape() {
            return Err(RuntimeError::InvalidBinding);
        }
        self.tx.commit().await?;
        Ok(options)
    }
}

//! Exact owner controls under the existing native directory transaction.
use super::*;
use chrono::Datelike;
use types::owner_policy_context::{OwnerPolicyOptions, SignedOwnerPolicyInquiry};

impl NativeDirectoryGuard {
    pub(crate) async fn install_owner_policy(
        mut self,
        signed: SignedOwnerPolicy,
    ) -> Result<String, RuntimeError> {
        if signed.policy.owner != self.owner {
            return Err(RuntimeError::Denied);
        }
        let digest = Finance::install_policy_in(&mut self.tx, signed).await?;
        self.tx.commit().await?;
        Ok(digest)
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
            || inquiry.price_digest != profile_digest
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
        };
        if !options.valid_shape() {
            return Err(RuntimeError::InvalidBinding);
        }
        self.tx.commit().await?;
        Ok(options)
    }
}

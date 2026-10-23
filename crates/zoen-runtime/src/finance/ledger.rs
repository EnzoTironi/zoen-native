//! Existing financial authority, extracted without changing transaction bodies.
use super::*;

impl Finance {
    pub(super) async fn lock_period(
        tx: &mut Transaction<'_, Postgres>,
        b: &Binding,
        shared: bool,
    ) -> Result<PeriodBalance, RuntimeError> {
        let query = if shared {
            "SELECT held_units,spent_units FROM runtime_periods WHERE owner=$1 AND period_start=$2 AND currency=$3 AND scale=$4 FOR SHARE"
        } else {
            "SELECT held_units,spent_units FROM runtime_periods WHERE owner=$1 AND period_start=$2 AND currency=$3 AND scale=$4 FOR UPDATE"
        };
        let row: Option<(i64, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(query.to_owned()))
            .bind(&b.context.owner)
            .bind(b.period_start)
            .bind(b.price.currency())
            .bind(i16::from(b.price.scale()))
            .fetch_optional(&mut **tx)
            .await?;
        let Some((held_units, spent_units)) = row else {
            return Err(RuntimeError::Denied);
        };
        Ok(PeriodBalance {
            held_units,
            spent_units,
        })
    }
    pub(super) async fn policy(
        tx: &mut Transaction<'_, Postgres>,
        b: &Binding,
    ) -> Result<crate::OwnerPeriodPolicy, RuntimeError> {
        let row: Option<(Json<SignedOwnerPolicy>,String)> = sqlx::query_as("SELECT signed,digest FROM runtime_policies WHERE owner=$1 AND period_start=$2 ORDER BY version DESC LIMIT 1")
            .bind(&b.context.owner).bind(b.period_start).fetch_optional(&mut **tx).await?;
        let Some((Json(signed), digest)) = row else {
            return Err(RuntimeError::Denied);
        };
        let now = Self::clock(tx).await?;
        let p = &signed.policy;
        let (start, end) = period(p.year, p.month)?;
        if !event_log::owner_budget::verify(&signed)
            || event_log::owner_budget::digest(p).as_deref() != Some(&digest)
            || digest != b.policy_digest
            || p.owner != b.context.owner
            || start != b.period_start
            || now < start
            || now >= end
            || now >= p.expires_at_ms
            || !p.enabled
            || p.currency != b.price.currency()
            || p.scale != b.price.scale()
            || !p.allowed_profiles.contains(&b.price.digest()?)
            || b.hold_units > p.max_attempt_units
        {
            return Err(RuntimeError::Denied);
        }
        Ok(signed.policy)
    }

    pub(super) async fn not_closed(
        tx: &mut Transaction<'_, Postgres>,
        b: &Binding,
    ) -> Result<(), RuntimeError> {
        let closed: Option<String> = sqlx::query_scalar(
            "SELECT binding_digest FROM runtime_predispatch_closures WHERE attempt=$1",
        )
        .bind(&b.context.attempt_id)
        .fetch_optional(&mut **tx)
        .await?;
        match closed {
            None => Ok(()),
            Some(digest) if digest == b.digest()? => Err(RuntimeError::Denied),
            Some(_) => Err(RuntimeError::InvalidBinding),
        }
    }

    pub(super) async fn retain_closure(
        tx: &mut Transaction<'_, Postgres>,
        b: &Binding,
        digest: &str,
    ) -> Result<(), RuntimeError> {
        let prior: Option<(String, i64, String)> = sqlx::query_as(
            "SELECT owner,period_start,binding_digest FROM runtime_predispatch_closures WHERE attempt=$1",
        )
        .bind(&b.context.attempt_id)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some((owner, period, original)) = prior {
            if owner != b.context.owner || period != b.period_start || original != digest {
                return Err(RuntimeError::InvalidBinding);
            }
        } else {
            sqlx::query("INSERT INTO runtime_predispatch_closures(attempt,owner,period_start,binding_digest) VALUES($1,$2,$3,$4)")
                .bind(&b.context.attempt_id).bind(&b.context.owner).bind(b.period_start)
                .bind(digest).execute(&mut **tx).await?;
        }
        Ok(())
    }
    pub async fn install_policy(&self, signed: SignedOwnerPolicy) -> Result<String, RuntimeError> {
        if !event_log::owner_budget::verify(&signed) {
            return Err(RuntimeError::InvalidPolicy);
        }
        let p = &signed.policy;
        let digest = event_log::owner_budget::digest(p).ok_or(RuntimeError::InvalidPolicy)?;
        let (start, end) = period(p.year, p.month)?;
        if p.expires_at_ms <= start || p.expires_at_ms > end {
            return Err(RuntimeError::InvalidPolicy);
        }
        let mut tx = self.begin().await?;
        Self::device(&mut tx, &p.owner, &signed.device, &signed.cert).await?;
        Self::person(&mut tx, &p.owner).await?;
        let now = Self::clock(&mut tx).await?;
        if now >= p.expires_at_ms {
            return Err(RuntimeError::InvalidPolicy);
        }
        sqlx::query("INSERT INTO runtime_periods(owner,period_start,period_end,currency,scale) VALUES ($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING")
            .bind(&p.owner).bind(start).bind(end).bind(&p.currency).bind(i16::from(p.scale)).execute(&mut *tx).await?;
        let matching: Option<i32> = sqlx::query_scalar("SELECT 1 FROM runtime_periods WHERE owner=$1 AND period_start=$2 AND period_end=$3 AND currency=$4 AND scale=$5 FOR UPDATE")
            .bind(&p.owner).bind(start).bind(end).bind(&p.currency).bind(i16::from(p.scale)).fetch_optional(&mut *tx).await?;
        if matching.is_none() {
            return Err(RuntimeError::InvalidPolicy);
        }
        let existing: Option<(i64,String)> = sqlx::query_as("SELECT version,digest FROM runtime_policies WHERE owner=$1 AND period_start=$2 ORDER BY version DESC LIMIT 1")
            .bind(&p.owner).bind(start).fetch_optional(&mut *tx).await?;
        let exact: Option<String> = sqlx::query_scalar(
            "SELECT digest FROM runtime_policies WHERE owner=$1 AND period_start=$2 AND version=$3",
        )
        .bind(&p.owner)
        .bind(start)
        .bind(i64::from(p.version))
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(old) = exact {
            if old != digest {
                return Err(RuntimeError::InvalidBinding);
            }
            tx.commit().await?;
            return Ok(digest);
        }
        if !match existing {
            None => p.version == 1 && p.previous_digest.is_none(),
            Some((version, ref previous)) => {
                Some(i64::from(p.version)) == version.checked_add(1)
                    && p.previous_digest.as_ref() == Some(previous)
            }
        } {
            return Err(RuntimeError::InvalidPolicy);
        }
        sqlx::query("INSERT INTO runtime_policies(owner,period_start,version,digest,previous_digest,signed) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(&p.owner).bind(start).bind(i64::from(p.version)).bind(&digest).bind(&p.previous_digest)
            .bind(Json(&signed)).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(digest)
    }
    pub(super) async fn journal(
        tx: &mut Transaction<'_, Postgres>,
        b: &Binding,
        kind: &str,
        charge: i64,
    ) -> Result<(), RuntimeError> {
        sqlx::query("INSERT INTO runtime_journals(attempt,kind) VALUES ($1,$2)")
            .bind(&b.context.attempt_id)
            .bind(kind)
            .execute(&mut **tx)
            .await?;
        let hold = if kind == "reserve" {
            b.hold_units
        } else {
            -b.hold_units
        };
        let mut postings = vec![("held", hold), ("clearing", -hold)];
        if kind == "settle" {
            postings.extend([("expense", charge), ("payable", -charge)]);
        }
        for (account, units) in postings {
            sqlx::query(
                "INSERT INTO runtime_postings(attempt,kind,account,units) VALUES ($1,$2,$3,$4)",
            )
            .bind(&b.context.attempt_id)
            .bind(kind)
            .bind(account)
            .bind(units)
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }
    pub async fn reserve(&self, b: &Binding) -> Result<(), RuntimeError> {
        if b.hold_units != b.price.reserve(b.requested_output_tokens)? {
            return Err(RuntimeError::InvalidBinding);
        }
        // Reservation is replayable by its immutable binding, including an
        // uncertain SQL commit. Keep each transaction's original short timeouts
        // and recheck directory/policy on each bounded retry. This loop never
        // claims an attempt, admits a dispatch or constructs a send capability.
        for retry in 0..3 {
            match self.reserve_once(b).await {
                Err(RuntimeError::Unavailable) if retry < 2 && !self.pool.is_closed() => {
                    tokio::time::sleep(std::time::Duration::from_millis(125 << retry)).await;
                }
                outcome => return outcome,
            }
        }
        Err(RuntimeError::Unavailable)
    }
    pub(super) async fn reserve_once(&self, b: &Binding) -> Result<(), RuntimeError> {
        let mut tx = self.begin().await?;
        Self::directory(&mut tx, b).await?;
        let balance = Self::lock_period(&mut tx, b, false).await?;
        Self::not_closed(&mut tx, b).await?;
        let policy = Self::policy(&mut tx, b).await?;
        let existing: Option<String> =
            sqlx::query_scalar("SELECT binding_digest FROM runtime_attempts WHERE attempt=$1")
                .bind(&b.context.attempt_id)
                .fetch_optional(&mut *tx)
                .await?;
        let digest = b.digest()?;
        if let Some(old) = existing {
            if old != digest {
                return Err(RuntimeError::InvalidBinding);
            }
            tx.commit().await?;
            return Ok(());
        }
        let held = balance
            .held_units
            .checked_add(b.hold_units)
            .ok_or(RuntimeError::OverBudget)?;
        if b.hold_units < 0
            || held
                .checked_add(balance.spent_units)
                .is_none_or(|v| v > policy.limit_units)
        {
            return Err(RuntimeError::OverBudget);
        }
        sqlx::query("INSERT INTO runtime_attempts(attempt,owner,period_start,binding_digest,binding,hold_units) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(&b.context.attempt_id).bind(&b.context.owner).bind(b.period_start).bind(&digest).bind(Json(b)).bind(b.hold_units).execute(&mut *tx).await?;
        sqlx::query("UPDATE runtime_periods SET held_units=$3 WHERE owner=$1 AND period_start=$2")
            .bind(&b.context.owner)
            .bind(b.period_start)
            .bind(held)
            .execute(&mut *tx)
            .await?;
        Self::journal(&mut tx, b, "reserve", 0).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn claim(&self, b: &Binding) -> Result<Option<FreshClaim>, RuntimeError> {
        let mut tx = self.begin().await?;
        Self::directory(&mut tx, b).await?;
        let balance = Self::lock_period(&mut tx, b, false).await?;
        Self::not_closed(&mut tx, b).await?;
        let policy = Self::policy(&mut tx, b).await?;
        if balance
            .held_units
            .checked_add(balance.spent_units)
            .is_none_or(|v| v > policy.limit_units)
        {
            return Err(RuntimeError::OverBudget);
        }
        let exact: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM runtime_attempts WHERE attempt=$1 AND binding_digest=$2",
        )
        .bind(&b.context.attempt_id)
        .bind(b.digest()?)
        .fetch_optional(&mut *tx)
        .await?;
        if exact.is_none() {
            return Err(RuntimeError::InvalidBinding);
        }
        let terminal: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM runtime_dispositions WHERE attempt=$1)",
        )
        .bind(&b.context.attempt_id)
        .fetch_one(&mut *tx)
        .await?;
        if terminal {
            tx.commit().await?;
            return Ok(None);
        }
        let mut nonce = [0; 32];
        getrandom::getrandom(&mut nonce).map_err(|_| RuntimeError::Unavailable)?;
        let nonce = hex::encode(nonce);
        let new = sqlx::query(
            "INSERT INTO runtime_claims(attempt,nonce) VALUES ($1,$2) ON CONFLICT DO NOTHING",
        )
        .bind(&b.context.attempt_id)
        .bind(&nonce)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        tx.commit().await?;
        Ok(new.then(|| FreshClaim {
            attempt: b.context.attempt_id.clone(),
            nonce,
            binding: b.digest().expect("validated binding"),
        }))
    }
    pub async fn guard(
        &self,
        b: &Binding,
        step: &crate::VerifiedStep,
    ) -> Result<DirectoryGuard, RuntimeError> {
        let mut tx = self.begin().await?;
        Self::directory(&mut tx, b).await?;
        let balance = Self::lock_period(&mut tx, b, true).await?;
        Self::not_closed(&mut tx, b).await?;
        let policy = Self::policy(&mut tx, b).await?;
        if balance
            .held_units
            .checked_add(balance.spent_units)
            .is_none_or(|v| v > policy.limit_units)
        {
            return Err(RuntimeError::OverBudget);
        }
        let mut valid_until_ms = policy
            .expires_at_ms
            .min(period(policy.year, policy.month)?.1);
        if let Some(native_deadline) = step.grant_deadline() {
            valid_until_ms = valid_until_ms.min(native_deadline);
        }
        if Self::clock(&mut tx).await? >= valid_until_ms {
            return Err(RuntimeError::Denied);
        }
        Ok(DirectoryGuard { tx, valid_until_ms })
    }
}

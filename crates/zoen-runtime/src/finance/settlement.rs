//! Existing financial authority, extracted without changing transaction bodies.
use super::*;

impl Finance {
    pub async fn capture(
        &self,
        capture: &Capture,
        digest: &str,
        sealed: &[u8],
    ) -> Result<FinancialState, RuntimeError> {
        let b = &capture.binding;
        if capture.result.context != b.context || capture.result.request_digest != b.request_digest
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let mut tx = self.begin().await?;
        let balance = Self::lock_period(&mut tx, b, false).await?;
        let exact: Option<i32> = sqlx::query_scalar("SELECT 1 FROM runtime_attempts a JOIN runtime_claims c USING (attempt) WHERE a.attempt=$1 AND a.binding_digest=$2")
            .bind(&b.context.attempt_id).bind(b.digest()?).fetch_optional(&mut *tx).await?;
        if exact.is_none() {
            return Err(RuntimeError::InvalidBinding);
        }
        sqlx::query("INSERT INTO runtime_evidence(attempt,digest,sealed) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(&b.context.attempt_id).bind(digest).bind(sealed).execute(&mut *tx).await?;
        Self::outbox(&mut tx, b, "evidence", digest).await?;
        let existing: Option<(String, i64, String)> = sqlx::query_as(
            "SELECT kind,amount_units,source_digest FROM runtime_dispositions WHERE attempt=$1",
        )
        .bind(&b.context.attempt_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((kind, units, source)) = existing {
            if kind != "settle" || source != digest {
                return Err(RuntimeError::InvalidBinding);
            }
            tx.commit().await?;
            return Ok(FinancialState::Settled { units });
        }
        let Some(charge) = b.price.charge(&capture.result)? else {
            tx.commit().await?;
            return Ok(FinancialState::Claimed);
        };
        let held = balance
            .held_units
            .checked_sub(b.hold_units)
            .filter(|v| *v >= 0)
            .ok_or(RuntimeError::InvalidBinding)?;
        let spent = balance
            .spent_units
            .checked_add(charge)
            .ok_or(RuntimeError::InvalidProfile)?;
        sqlx::query("INSERT INTO runtime_dispositions(attempt,kind,amount_units,source_digest) VALUES ($1,'settle',$2,$3)")
            .bind(&b.context.attempt_id).bind(charge).bind(digest).execute(&mut *tx).await?;
        sqlx::query("UPDATE runtime_periods SET held_units=$3,spent_units=$4 WHERE owner=$1 AND period_start=$2")
            .bind(&b.context.owner).bind(b.period_start).bind(held).bind(spent).execute(&mut *tx).await?;
        Self::journal(&mut tx, b, "settle", charge).await?;
        Self::outbox(&mut tx, b, "settle", digest).await?;
        tx.commit().await?;
        Ok(FinancialState::Settled { units: charge })
    }
    pub(super) async fn outbox(
        tx: &mut Transaction<'_, Postgres>,
        b: &Binding,
        kind: &str,
        digest: &str,
    ) -> Result<(), RuntimeError> {
        sqlx::query("INSERT INTO runtime_financial_outbox(attempt,kind,digest) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(&b.context.attempt_id).bind(kind).bind(digest).execute(&mut **tx).await?;
        Ok(())
    }
    pub async fn release(
        &self,
        proof: crate::execution::PreDispatchClosure,
        continuity: &crate::RuntimeAuthority,
    ) -> Result<FinancialState, RuntimeError> {
        continuity.continuity()?;
        let (b, digest) = proof.consume();
        let mut tx = self.begin().await?;
        let balance = Self::lock_period(&mut tx, &b, false).await?;
        let exact: Option<String> =
            sqlx::query_scalar("SELECT binding_digest FROM runtime_attempts WHERE attempt=$1")
                .bind(&b.context.attempt_id)
                .fetch_optional(&mut *tx)
                .await?;
        if digest != b.digest()? || exact.as_deref().is_some_and(|old| old != digest) {
            return Err(RuntimeError::InvalidBinding);
        }
        Self::retain_closure(&mut tx, &b, &digest).await?;
        if exact.is_none() {
            return Self::finish_release(tx, continuity).await;
        }
        let terminal: Option<String> =
            sqlx::query_scalar("SELECT kind FROM runtime_dispositions WHERE attempt=$1")
                .bind(&b.context.attempt_id)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(kind) = terminal {
            if kind != "release" {
                return Err(RuntimeError::InvalidBinding);
            }
            return Self::finish_release(tx, continuity).await;
        }
        let held = balance
            .held_units
            .checked_sub(b.hold_units)
            .filter(|v| *v >= 0)
            .ok_or(RuntimeError::InvalidBinding)?;
        sqlx::query("INSERT INTO runtime_dispositions(attempt,kind,amount_units,source_digest) VALUES ($1,'release',0,$2)")
            .bind(&b.context.attempt_id).bind(&digest).execute(&mut *tx).await?;
        sqlx::query("UPDATE runtime_periods SET held_units=$3 WHERE owner=$1 AND period_start=$2")
            .bind(&b.context.owner)
            .bind(b.period_start)
            .bind(held)
            .execute(&mut *tx)
            .await?;
        Self::journal(&mut tx, &b, "release", 0).await?;
        Self::outbox(&mut tx, &b, "release", &digest).await?;
        Self::finish_release(tx, continuity).await
    }

    pub(super) async fn finish_release(
        tx: Transaction<'static, Postgres>,
        continuity: &crate::RuntimeAuthority,
    ) -> Result<FinancialState, RuntimeError> {
        continuity.continuity()?;
        tx.commit().await?;
        #[cfg(test)]
        if continuity.fault.load(std::sync::atomic::Ordering::SeqCst) == 28 {
            return Err(RuntimeError::Unavailable);
        }
        continuity.continuity()?;
        Ok(FinancialState::Released)
    }
    pub async fn balance(
        &self,
        owner: &str,
        year: i32,
        month: u8,
    ) -> Result<PeriodBalance, RuntimeError> {
        let start = period(year, month)?.0;
        let (held_units, spent_units) = sqlx::query_as(
            "SELECT held_units,spent_units FROM runtime_periods WHERE owner=$1 AND period_start=$2",
        )
        .bind(owner)
        .bind(start)
        .fetch_one(&self.pool)
        .await?;
        Ok(PeriodBalance {
            held_units,
            spent_units,
        })
    }
    pub async fn inspect(&self, attempt: &str) -> Result<Option<FinancialState>, RuntimeError> {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runtime_attempts WHERE attempt=$1)")
                .bind(attempt)
                .fetch_one(&self.pool)
                .await?;
        if !exists {
            return Ok(None);
        }
        let terminal: Option<(String, i64)> =
            sqlx::query_as("SELECT kind,amount_units FROM runtime_dispositions WHERE attempt=$1")
                .bind(attempt)
                .fetch_optional(&self.pool)
                .await?;
        if let Some((kind, units)) = terminal {
            return Ok(Some(if kind == "settle" {
                FinancialState::Settled { units }
            } else {
                FinancialState::Released
            }));
        }
        let claimed: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runtime_claims WHERE attempt=$1)")
                .bind(attempt)
                .fetch_one(&self.pool)
                .await?;
        Ok(Some(if claimed {
            FinancialState::Claimed
        } else {
            FinancialState::Reserved
        }))
    }
    pub async fn pending_outbox(
        &self,
        limit: u16,
    ) -> Result<Vec<(String, String, String, Option<Vec<u8>>)>, RuntimeError> {
        Ok(sqlx::query_as("SELECT o.attempt,o.kind,o.digest,e.sealed FROM runtime_financial_outbox o LEFT JOIN runtime_evidence e USING(attempt,digest) WHERE NOT EXISTS (SELECT 1 FROM runtime_financial_acks a WHERE (a.attempt,a.kind,a.digest)=(o.attempt,o.kind,o.digest)) ORDER BY o.attempt,o.kind,o.digest LIMIT $1")
            .bind(i64::from(limit)).fetch_all(&self.pool).await?)
    }
    pub async fn ack_outbox(
        &self,
        attempt: &str,
        kind: &str,
        digest: &str,
    ) -> Result<(), RuntimeError> {
        sqlx::query("INSERT INTO runtime_financial_acks(attempt,kind,digest) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(attempt).bind(kind).bind(digest).execute(&self.pool).await?;
        Ok(())
    }
    #[cfg(test)]
    pub async fn current_period(&self) -> (i32, u8, i64, i64) {
        let now: i64 =
            sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::BIGINT")
                .fetch_one(&self.pool)
                .await
                .unwrap();
        let time = chrono::DateTime::from_timestamp_millis(now).unwrap();
        let (start, end) = period(time.year(), time.month() as u8).unwrap();
        (time.year(), time.month() as u8, start, end)
    }
}

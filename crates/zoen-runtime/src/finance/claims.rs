//! Existing one-shot SQL claim. Retained claim metadata is never a fresh permit.
use super::*;
impl Finance {
    pub async fn claim(&self, b: &Binding) -> Result<Option<FreshClaim>, RuntimeError> {
        let mut tx = self.begin().await?;
        Self::require_unmanaged_policy(&mut tx).await?;
        let claim = Self::claim_in(&mut tx, b).await?;
        tx.commit().await?;
        Ok(claim)
    }
    pub(crate) async fn claim_in(
        tx: &mut Transaction<'_, Postgres>,
        b: &Binding,
    ) -> Result<Option<FreshClaim>, RuntimeError> {
        if !Self::claim_allowed(tx, b).await? {
            return Ok(None);
        }
        let mut nonce = [0u8; 32];
        getrandom::getrandom(&mut nonce).map_err(|_| RuntimeError::Unavailable)?;
        let nonce = hex::encode(nonce);
        let binding = b.digest()?;
        let new = Self::insert_claim(tx, b, &nonce).await?;
        Ok(new.then(|| FreshClaim {
            attempt: b.context.attempt_id.clone(),
            nonce,
            binding,
            checkpoint: None,
        }))
    }
    pub(crate) async fn restore_claim_in(
        tx: &mut Transaction<'_, Postgres>,
        b: &Binding,
        nonce: &str,
    ) -> Result<bool, RuntimeError> {
        if !Self::claim_allowed(tx, b).await? {
            return Ok(false);
        }
        Self::insert_claim(tx, b, nonce).await
    }
    async fn claim_allowed(
        tx: &mut Transaction<'_, Postgres>,
        b: &Binding,
    ) -> Result<bool, RuntimeError> {
        let digest = b.digest()?;
        Self::directory(tx, b).await?;
        let balance = Self::lock_period(tx, b, false).await?;
        Self::not_closed(tx, b).await?;
        let policy = Self::policy(tx, b).await?;
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
        .bind(&digest)
        .fetch_optional(&mut **tx)
        .await?;
        if exact.is_none() {
            return Err(RuntimeError::InvalidBinding);
        }
        let terminal: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM runtime_dispositions WHERE attempt=$1)",
        )
        .bind(&b.context.attempt_id)
        .fetch_one(&mut **tx)
        .await?;
        if terminal {
            return Ok(false);
        }
        Ok(true)
    }
    async fn insert_claim(
        tx: &mut Transaction<'_, Postgres>,
        b: &Binding,
        nonce: &str,
    ) -> Result<bool, RuntimeError> {
        if nonce.len() != 64
            || !nonce
                .bytes()
                .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let new = sqlx::query(
            "INSERT INTO runtime_claims(attempt,nonce) VALUES ($1,$2) ON CONFLICT DO NOTHING",
        )
        .bind(&b.context.attempt_id)
        .bind(nonce)
        .execute(&mut **tx)
        .await?
        .rows_affected()
            == 1;
        Ok(new)
    }
}

use crate::{Binding, Capture, FinancialState, PeriodBalance, RuntimeError};
#[cfg(test)]
use chrono::Datelike;
use chrono::{TimeZone, Utc};
use sqlx::{postgres::PgPoolOptions, types::Json, PgPool, Postgres, Transaction};
use types::{owner_budget::SignedOwnerPolicy, Identity, IdentityKind};

pub(crate) struct Finance {
    pub(super) pool: PgPool,
}
pub(crate) struct FreshClaim {
    attempt: String,
    nonce: String,
    binding: String,
    checkpoint: Option<crate::managed_images::DispatchCheckpoint>,
}
impl FreshClaim {
    pub(crate) fn effect_digest(&self) -> Result<String, RuntimeError> {
        self.checkpoint_reference().digest()
    }
    pub(crate) fn checkpoint_reference(&self) -> crate::managed_images::DispatchReference {
        crate::managed_images::DispatchReference {
            attempt: self.attempt.clone(),
            nonce: self.nonce.clone(),
            binding_digest: self.binding.clone(),
        }
    }
    pub(crate) fn with_checkpoint(
        mut self,
        checkpoint: Option<crate::managed_images::DispatchCheckpoint>,
    ) -> Result<Self, RuntimeError> {
        if checkpoint.as_ref().is_some_and(|checkpoint| {
            Some(&checkpoint.prepared.effect_digest) != self.effect_digest().ok().as_ref()
        }) {
            return Err(RuntimeError::InvalidBinding);
        }
        self.checkpoint = checkpoint;
        Ok(self)
    }
    pub fn consume(
        self,
    ) -> (
        String,
        String,
        String,
        Option<crate::managed_images::DispatchCheckpoint>,
    ) {
        (self.attempt, self.nonce, self.binding, self.checkpoint)
    }
}
pub(crate) struct DirectoryGuard {
    tx: Transaction<'static, Postgres>,
    valid_until_ms: i64,
}

pub(crate) struct NativeDirectoryGuard {
    tx: Transaction<'static, Postgres>,
    owner: String,
    agent: String,
    device: String,
    certificate_digest: String,
    #[cfg(test)]
    ack_cut: NativeAckCut,
}

#[cfg(test)]
enum NativeAckCut {
    Normal,
    Suppressed,
    Delayed,
    OutputExpiry(std::sync::Arc<std::sync::atomic::AtomicU64>),
}

/// One known SQL maintenance admission. Only NativeDirectoryGuard constructs
/// it in production; reads, replay and uncertain commits cannot recover it.
/// It cannot authorize a provider send and is neither Clone nor Deserialize.
pub(crate) struct NativeActivation {
    scope: crate::native::ActivationScope,
    expires: std::time::Instant,
}

pub(crate) struct ReplyOutputAdmission {
    scope: crate::native::ReplyOutputScope,
    expires: std::time::Instant,
}

impl ReplyOutputAdmission {
    pub(crate) fn check(
        &self,
        expected: &crate::native::ReplyOutputScope,
    ) -> Result<(), RuntimeError> {
        if &self.scope != expected {
            return Err(RuntimeError::InvalidBinding);
        }
        if std::time::Instant::now() >= self.expires {
            return Err(RuntimeError::Denied);
        }
        Ok(())
    }
    pub(crate) fn consume(
        self,
        expected: &crate::native::ReplyOutputScope,
    ) -> Result<(), RuntimeError> {
        self.check(expected)
    }
}

impl NativeActivation {
    pub(crate) fn check(
        &self,
        expected: &crate::native::ActivationScope,
    ) -> Result<(), RuntimeError> {
        if &self.scope != expected {
            return Err(RuntimeError::InvalidBinding);
        }
        if std::time::Instant::now() >= self.expires {
            return Err(RuntimeError::Denied);
        }
        Ok(())
    }

    pub(crate) fn consume(
        self,
        expected: &crate::native::ActivationScope,
    ) -> Result<(), RuntimeError> {
        self.check(expected)
    }

    #[cfg(test)]
    pub(crate) fn fixture(scope: crate::native::ActivationScope) -> Self {
        Self {
            scope,
            expires: std::time::Instant::now() + std::time::Duration::from_secs(2),
        }
    }
}

impl NativeDirectoryGuard {
    pub(crate) async fn authorize_output(
        mut self,
        scope: crate::native::ReplyOutputScope,
    ) -> Result<ReplyOutputAdmission, RuntimeError> {
        if !scope.matches_directory(
            &self.owner,
            &self.agent,
            &self.device,
            &self.certificate_digest,
        ) {
            return Err(RuntimeError::InvalidBinding);
        }
        let started = std::time::Instant::now();
        let clock = Finance::clock(&mut self.tx).await?;
        let remaining = match scope.expires_at_ms() {
            Some(expiry) => expiry
                .checked_sub(clock)
                .filter(|remaining| *remaining > 0)
                .ok_or(RuntimeError::Denied)?
                .min(2000) as u64,
            None => 2000,
        };
        self.tx.commit().await?;
        #[cfg(test)]
        match self.ack_cut {
            NativeAckCut::Normal => {}
            NativeAckCut::Suppressed => return Err(RuntimeError::Unavailable),
            NativeAckCut::Delayed => {
                tokio::time::sleep(std::time::Duration::from_millis(2200)).await
            }
            NativeAckCut::OutputExpiry(observed_remaining) => {
                if scope.expires_at_ms().is_none() || remaining >= 1800 {
                    return Err(RuntimeError::InvalidBinding);
                }
                observed_remaining.store(remaining, std::sync::atomic::Ordering::SeqCst);
                println!("native output deadline cut: actual SQL/native remainder {remaining} ms; ACK delay consumes it below the 2 s maintenance cap");
                tokio::time::sleep(std::time::Duration::from_millis(remaining + 50)).await;
            }
        }
        let admission = ReplyOutputAdmission {
            scope,
            expires: started + std::time::Duration::from_millis(remaining),
        };
        admission.check(&admission.scope)?;
        Ok(admission)
    }

    #[cfg(test)]
    pub(crate) fn suppress_known_ack(mut self) -> Self {
        self.ack_cut = NativeAckCut::Suppressed;
        self
    }

    #[cfg(test)]
    pub(crate) fn delay_known_ack(mut self) -> Self {
        self.ack_cut = NativeAckCut::Delayed;
        self
    }

    #[cfg(test)]
    pub(crate) fn delay_output_expiry(
        mut self,
        observed_remaining: std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        self.ack_cut = NativeAckCut::OutputExpiry(observed_remaining);
        self
    }

    pub(crate) async fn commit(self) -> Result<(), RuntimeError> {
        self.tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn authorize(
        self,
        scope: crate::native::ActivationScope,
    ) -> Result<NativeActivation, RuntimeError> {
        self.authorize_before(scope, None).await
    }
    pub(crate) async fn authorize_before(
        mut self,
        scope: crate::native::ActivationScope,
        expires_at_ms: Option<i64>,
    ) -> Result<NativeActivation, RuntimeError> {
        if !scope.matches_directory(
            &self.owner,
            &self.agent,
            &self.device,
            &self.certificate_digest,
        ) {
            return Err(RuntimeError::InvalidBinding);
        }
        // Query and ACK latency consume the window. A dead/idle-terminated
        // connection fails before any FDB activation can start.
        let observed = std::time::Instant::now();
        let now = Finance::clock(&mut self.tx).await?;
        let remaining = match expires_at_ms {
            Some(expiry) => expiry
                .checked_sub(now)
                .filter(|remaining| *remaining > 0)
                .ok_or(RuntimeError::Denied)?
                .min(2000) as u64,
            None => 2000,
        };
        self.tx.commit().await?;
        // Explicit test cuts after a known real SQL commit. Neither cut claims
        // database wire-level commit-unknown behavior.
        #[cfg(test)]
        match self.ack_cut {
            NativeAckCut::Normal => {}
            NativeAckCut::Suppressed => return Err(RuntimeError::Unavailable),
            NativeAckCut::Delayed => {
                tokio::time::sleep(std::time::Duration::from_millis(2200)).await
            }
            NativeAckCut::OutputExpiry(_) => return Err(RuntimeError::InvalidBinding),
        }
        let admission = NativeActivation {
            scope,
            expires: observed + std::time::Duration::from_millis(remaining),
        };
        admission.check(&admission.scope)?;
        Ok(admission)
    }
}
pub(crate) struct DispatchPermit {
    expires: std::time::Instant,
}
impl DispatchPermit {
    pub fn consume(self) -> Result<zoen_models::Admission, RuntimeError> {
        if std::time::Instant::now() >= self.expires {
            return Err(RuntimeError::Denied);
        }
        Ok(zoen_models::Admission::Fresh)
    }
}

impl DirectoryGuard {
    pub async fn finish(
        mut self,
        admission: crate::execution::FreshAdmission,
    ) -> Result<DispatchPermit, RuntimeError> {
        let (attempt, nonce, checkpoint) = admission.consume();
        sqlx::query("INSERT INTO runtime_admission_witnesses (attempt,nonce) VALUES ($1,$2)")
            .bind(attempt)
            .bind(nonce)
            .execute(&mut *self.tx)
            .await?;
        if let Some(checkpoint) = checkpoint {
            checkpoint.write_sql(&mut self.tx).await?;
        }
        // Anchor the monotonic deadline before the SQL clock request, so query
        // and commit latency consume the window rather than extend permission.
        let observed = std::time::Instant::now();
        let now = Finance::clock(&mut self.tx).await?;
        let remaining = self
            .valid_until_ms
            .checked_sub(now)
            .filter(|remaining| *remaining > 0)
            .ok_or(RuntimeError::Denied)?;
        let window = std::time::Duration::from_millis(remaining as u64)
            .min(std::time::Duration::from_secs(2));
        self.tx.commit().await?;
        Ok(DispatchPermit {
            expires: observed + window,
        })
    }
    #[cfg(test)]
    pub async fn rollback(self) -> Result<(), RuntimeError> {
        self.tx.rollback().await?;
        Ok(())
    }
}

pub(crate) fn period(year: i32, month: u8) -> Result<(i64, i64), RuntimeError> {
    if !(2000..=2200).contains(&year) || !(1..=12).contains(&month) {
        return Err(RuntimeError::InvalidPolicy);
    }
    let start = Utc
        .with_ymd_and_hms(year, u32::from(month), 1, 0, 0, 0)
        .single()
        .ok_or(RuntimeError::InvalidPolicy)?;
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, u32::from(month) + 1)
    };
    let end = Utc
        .with_ymd_and_hms(next_year, next_month, 1, 0, 0, 0)
        .single()
        .ok_or(RuntimeError::InvalidPolicy)?;
    Ok((start.timestamp_millis(), end.timestamp_millis()))
}
mod directory;
mod image_checkpoint;
mod image_checkpoint_recovery;
mod ledger;
mod settlement;

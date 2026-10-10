use crate::{Binding, Capture, FinancialState, PeriodBalance, RuntimeError};
#[cfg(test)]
use chrono::Datelike;
use chrono::{TimeZone, Utc};
use roda_types::{owner_budget::SignedOwnerPolicy, Identity, IdentityKind};
use sqlx::{postgres::PgPoolOptions, types::Json, PgPool, Postgres, Transaction};

pub(crate) struct Finance {
    pub(super) pool: PgPool,
}
pub(crate) struct FreshClaim {
    attempt: String,
    nonce: String,
    binding: String,
}
impl FreshClaim {
    pub fn consume(self) -> (String, String, String) {
        (self.attempt, self.nonce, self.binding)
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
}

/// One known SQL maintenance admission. Only NativeDirectoryGuard constructs
/// it in production; reads, replay and uncertain commits cannot recover it.
/// It cannot authorize a provider send and is neither Clone nor Deserialize.
pub(crate) struct NativeActivation {
    scope: crate::native::ActivationScope,
    expires: std::time::Instant,
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

    pub(crate) async fn commit(self) -> Result<(), RuntimeError> {
        self.tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn authorize(
        mut self,
        scope: crate::native::ActivationScope,
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
        Finance::clock(&mut self.tx).await?;
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
        }
        let admission = NativeActivation {
            scope,
            expires: observed + std::time::Duration::from_secs(2),
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
        let (attempt, nonce) = admission.consume();
        sqlx::query("INSERT INTO runtime_admission_witnesses (attempt,nonce) VALUES ($1,$2)")
            .bind(attempt)
            .bind(nonce)
            .execute(&mut *self.tx)
            .await?;
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
impl Finance {
    pub async fn connect(url: &str) -> Result<Self, RuntimeError> {
        Ok(Self {
            pool: PgPoolOptions::new()
                .max_connections(12)
                .acquire_timeout(std::time::Duration::from_secs(3))
                .connect(url)
                .await?,
        })
    }
    pub async fn bind_deployment(
        &self,
        namespace: &str,
        key_digest: &str,
        db: &foundationdb::Database,
        root: &foundationdb::tuple::Subspace,
    ) -> Result<String, RuntimeError> {
        let mut tx = self.begin().await?;
        // Serialize only startup pairing. Ordinary finance and HTTP calls do
        // not take this table lock. Both store commits must be known successes.
        sqlx::query("LOCK TABLE runtime_deployment_binding IN EXCLUSIVE MODE")
            .execute(&mut *tx)
            .await?;
        let old: Option<(String, String, String)> = sqlx::query_as(
            "SELECT namespace,key_digest,witness FROM runtime_deployment_binding WHERE singleton",
        )
        .fetch_optional(&mut *tx)
        .await?;
        let witness = if let Some((old_namespace, old_key, witness)) = old {
            if old_namespace != namespace || old_key != key_digest {
                return Err(RuntimeError::DeploymentMismatch);
            }
            crate::execution::Execution::verify_deployment(db, root, &witness).await?;
            witness
        } else {
            sqlx::query("LOCK TABLE runtime_attempts IN SHARE MODE")
                .execute(&mut *tx)
                .await?;
            let retained: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runtime_attempts)")
                    .fetch_one(&mut *tx)
                    .await?;
            if retained {
                return Err(RuntimeError::DeploymentMismatch);
            }
            let mut bytes = [0; 32];
            getrandom::getrandom(&mut bytes).map_err(|_| RuntimeError::Unavailable)?;
            let witness = hex::encode(bytes);
            crate::execution::Execution::create_deployment(db, root, &witness).await?;
            sqlx::query("INSERT INTO runtime_deployment_binding(namespace,key_digest,witness) VALUES ($1,$2,$3)")
                .bind(namespace).bind(key_digest).bind(&witness).execute(&mut *tx).await?;
            witness
        };
        // A failed/unknown SQL commit leaves a possibly orphaned FDB marker.
        // Open never adopts an orphan, creates a second pairing or grants work.
        tx.commit().await?;
        Ok(witness)
    }
    async fn begin(&self) -> Result<Transaction<'static, Postgres>, RuntimeError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT set_config('lock_timeout','2s',true),set_config('statement_timeout','2s',true),set_config('idle_in_transaction_session_timeout','5s',true)")
            .execute(&mut *tx).await?;
        Ok(tx)
    }
    async fn clock(tx: &mut Transaction<'_, Postgres>) -> Result<i64, RuntimeError> {
        Ok(
            sqlx::query_scalar(
                "SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::BIGINT",
            )
            .fetch_one(&mut **tx)
            .await?,
        )
    }
    async fn device(
        tx: &mut Transaction<'_, Postgres>,
        identity: &str,
        device: &str,
        cert: &str,
    ) -> Result<(), RuntimeError> {
        let active: Option<i32> = sqlx::query_scalar("SELECT 1 FROM devices WHERE device=$1 AND identity=$2 AND cert=$3 AND revoked_at IS NULL FOR SHARE")
            .bind(device).bind(identity).bind(cert).fetch_optional(&mut **tx).await?;
        if active.is_none() {
            return Err(RuntimeError::Denied);
        }
        Ok(())
    }
    async fn person(tx: &mut Transaction<'_, Postgres>, owner: &str) -> Result<(), RuntimeError> {
        let row: Option<(String, Option<String>, Json<Identity>)> =
            sqlx::query_as("SELECT kind,owner,profile FROM identities WHERE id=$1 FOR SHARE")
                .bind(owner)
                .fetch_optional(&mut **tx)
                .await?;
        if !row.is_some_and(|(kind, parent, Json(p))| {
            kind == "Person"
                && parent.is_none()
                && p.id == owner
                && p.kind == IdentityKind::Person
                && roda_log::agent_owner::profile_authorized(&p)
        }) {
            return Err(RuntimeError::Denied);
        }
        Ok(())
    }
    async fn directory(
        tx: &mut Transaction<'_, Postgres>,
        b: &Binding,
    ) -> Result<(), RuntimeError> {
        Self::device(tx, &b.context.agent, &b.context.device, &b.device_cert).await?;
        let row: Option<(String, Option<String>, Json<Identity>)> =
            sqlx::query_as("SELECT kind,owner,profile FROM identities WHERE id=$1 FOR SHARE")
                .bind(&b.context.agent)
                .fetch_optional(&mut **tx)
                .await?;
        if !row.is_some_and(|(kind, owner, Json(p))| {
            kind == "Agent"
                && owner.as_deref() == Some(&b.context.owner)
                && p.id == b.context.agent
                && p.kind == IdentityKind::Agent
                && p.owner == owner
                && roda_log::agent_owner::profile_authorized(&p)
        }) {
            return Err(RuntimeError::Denied);
        }
        Self::person(tx, &b.context.owner).await
    }

    pub(super) async fn native_directory(
        &self,
        agent: &Identity,
        owner: &Identity,
        device: &str,
        certificate: &str,
    ) -> Result<NativeDirectoryGuard, RuntimeError> {
        let mut tx = self.begin().await?;
        Self::device(&mut tx, &agent.id, device, certificate).await?;
        for expected in [agent, owner] {
            let row: Option<(String, Option<String>, Json<Identity>)> =
                sqlx::query_as("SELECT kind,owner,profile FROM identities WHERE id=$1 FOR SHARE")
                    .bind(&expected.id)
                    .fetch_optional(&mut *tx)
                    .await?;
            let Some((kind, parent, Json(actual))) = row else {
                return Err(RuntimeError::Denied);
            };
            if &actual != expected
                || parent != expected.owner
                || kind
                    != match expected.kind {
                        IdentityKind::Person => "Person",
                        IdentityKind::Agent => "Agent",
                    }
                || !roda_log::agent_owner::profile_authorized(&actual)
            {
                return Err(RuntimeError::Denied);
            }
        }
        if agent.kind != IdentityKind::Agent
            || owner.kind != IdentityKind::Person
            || owner.owner.is_some()
            || agent.owner.as_deref() != Some(owner.id.as_str())
        {
            return Err(RuntimeError::Denied);
        }
        Ok(NativeDirectoryGuard {
            tx,
            owner: owner.id.clone(),
            agent: agent.id.clone(),
            device: device.into(),
            certificate_digest: crate::hash(certificate.as_bytes()),
            #[cfg(test)]
            ack_cut: NativeAckCut::Normal,
        })
    }
    async fn lock_period(
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
            .bind(&b.price.currency)
            .bind(i16::from(b.price.scale))
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
    async fn policy(
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
        if !roda_log::owner_budget::verify(&signed)
            || roda_log::owner_budget::digest(p).as_deref() != Some(&digest)
            || digest != b.policy_digest
            || p.owner != b.context.owner
            || start != b.period_start
            || now < start
            || now >= end
            || now >= p.expires_at_ms
            || !p.enabled
            || p.currency != b.price.currency
            || p.scale != b.price.scale
            || !p.allowed_profiles.contains(&b.price.digest()?)
            || b.hold_units > p.max_attempt_units
        {
            return Err(RuntimeError::Denied);
        }
        Ok(signed.policy)
    }
    pub async fn install_policy(&self, signed: SignedOwnerPolicy) -> Result<String, RuntimeError> {
        if !roda_log::owner_budget::verify(&signed) {
            return Err(RuntimeError::InvalidPolicy);
        }
        let p = &signed.policy;
        let digest = roda_log::owner_budget::digest(p).ok_or(RuntimeError::InvalidPolicy)?;
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
    async fn journal(
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
    async fn reserve_once(&self, b: &Binding) -> Result<(), RuntimeError> {
        let mut tx = self.begin().await?;
        Self::directory(&mut tx, b).await?;
        let balance = Self::lock_period(&mut tx, b, false).await?;
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
    pub async fn guard(&self, b: &Binding) -> Result<DirectoryGuard, RuntimeError> {
        let mut tx = self.begin().await?;
        Self::directory(&mut tx, b).await?;
        let balance = Self::lock_period(&mut tx, b, true).await?;
        let policy = Self::policy(&mut tx, b).await?;
        if balance
            .held_units
            .checked_add(balance.spent_units)
            .is_none_or(|v| v > policy.limit_units)
        {
            return Err(RuntimeError::OverBudget);
        }
        Ok(DirectoryGuard {
            tx,
            valid_until_ms: policy
                .expires_at_ms
                .min(period(policy.year, policy.month)?.1),
        })
    }
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
    async fn outbox(
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
        let terminal: Option<String> =
            sqlx::query_scalar("SELECT kind FROM runtime_dispositions WHERE attempt=$1")
                .bind(&b.context.attempt_id)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(kind) = terminal {
            if kind != "release" {
                return Err(RuntimeError::InvalidBinding);
            }
            tx.commit().await?;
            return Ok(FinancialState::Released);
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
        continuity.continuity()?;
        tx.commit().await?;
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

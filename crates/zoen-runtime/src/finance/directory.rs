//! Existing financial authority, extracted without changing transaction bodies.
use super::*;

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
    pub(super) async fn begin(&self) -> Result<Transaction<'static, Postgres>, RuntimeError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT set_config('lock_timeout','2s',true),set_config('statement_timeout','2s',true),set_config('idle_in_transaction_session_timeout','5s',true)")
            .execute(&mut *tx).await?;
        Ok(tx)
    }
    pub(super) async fn clock(tx: &mut Transaction<'_, Postgres>) -> Result<i64, RuntimeError> {
        Ok(
            sqlx::query_scalar(
                "SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::BIGINT",
            )
            .fetch_one(&mut **tx)
            .await?,
        )
    }

    pub(crate) async fn reply_clock(&self) -> Result<i64, RuntimeError> {
        let mut tx = self.begin().await?;
        let now = Self::clock(&mut tx).await?;
        tx.commit().await?;
        Ok(now)
    }

    /// An original discovery snapshot, not a spend reservation or live permit.
    /// Dispatch must independently revalidate the frozen policy and budget.
    pub(crate) async fn reply_budget(
        &self,
        owner: &str,
        price: &crate::PriceProfile,
    ) -> Result<(i64, String), RuntimeError> {
        let mut tx = self.begin().await?;
        Self::person(&mut tx, owner).await?;
        let now = Self::clock(&mut tx).await?;
        let row: Option<(Json<SignedOwnerPolicy>, String)> = sqlx::query_as(
            "SELECT signed,digest FROM runtime_policies WHERE owner=$1 AND period_start <= $2 ORDER BY period_start DESC,version DESC LIMIT 1"
        ).bind(owner).bind(now).fetch_optional(&mut *tx).await?;
        let Some((Json(signed), digest)) = row else {
            return Err(RuntimeError::Denied);
        };
        let policy = &signed.policy;
        let (start, end) = period(policy.year, policy.month)?;
        if !event_log::owner_budget::verify(&signed)
            || event_log::owner_budget::digest(policy).as_deref() != Some(&digest)
            || policy.owner != owner
            || now < start
            || now >= end
            || now >= policy.expires_at_ms
            || !policy.enabled
            || policy.currency != price.currency
            || policy.scale != price.scale
            || !policy.allowed_profiles.contains(&price.digest()?)
            || price.reserve(price.max_output_tokens)? > policy.max_attempt_units
        {
            return Err(RuntimeError::Denied);
        }
        tx.commit().await?;
        Ok((start, digest))
    }
    pub(super) async fn device(
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
    pub(super) async fn person(
        tx: &mut Transaction<'_, Postgres>,
        owner: &str,
    ) -> Result<(), RuntimeError> {
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
                && event_log::agent_owner::profile_authorized(&p)
        }) {
            return Err(RuntimeError::Denied);
        }
        Ok(())
    }
    pub(super) async fn directory(
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
                && event_log::agent_owner::profile_authorized(&p)
        }) {
            return Err(RuntimeError::Denied);
        }
        Self::person(tx, &b.context.owner).await
    }

    pub(crate) async fn native_directory(
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
                || !event_log::agent_owner::profile_authorized(&actual)
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
}

impl NativeDirectoryGuard {
    /// Publication uses the same current profile/certificate/device locks as
    /// maintenance, and the existing relay receipts in this very transaction.
    pub(crate) async fn publish_key_packages(
        mut self,
        packages: &[Vec<u8>],
    ) -> Result<(), RuntimeError> {
        let validated = relay_admission::directory::key_packages::validate_publications(
            &self.agent,
            &self.device,
            packages,
            None,
        )
        .map_err(|_| RuntimeError::Denied)?;
        if !zoen_relay::db::put_key_packages_in(
            &mut self.tx,
            &self.agent,
            &self.device,
            &validated,
            None,
        )
        .await?
        {
            return Err(RuntimeError::Denied);
        }
        self.tx.commit().await?;
        Ok(())
    }
}

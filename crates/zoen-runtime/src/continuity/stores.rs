use super::*;
use foundationdb::Transaction as FdbTransaction;

impl ManagedRuntime {
    pub(super) async fn sql_checkpoint(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Checkpoint, RuntimeError> {
        let row: Option<(i64,i64,String)> = sqlx::query_as("SELECT generation,sequence,digest FROM runtime_image_checkpoint WHERE singleton AND profile=3 AND namespace=$1 AND witness=$2 AND agent IS NULL AND device IS NULL FOR UPDATE")
            .bind(&self.deployment.namespace).bind(&self.deployment.witness).fetch_optional(&mut **tx).await?;
        let (generation, sequence, digest) = row.ok_or(RuntimeError::RestoreUnreconciled)?;
        Ok(Checkpoint {
            generation: generation
                .try_into()
                .map_err(|_| RuntimeError::InvalidBinding)?,
            sequence: sequence
                .try_into()
                .map_err(|_| RuntimeError::InvalidBinding)?,
            digest,
        })
    }

    pub(super) async fn write_sql(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        prepared: &Prepared,
    ) -> Result<(), RuntimeError> {
        if self.sql_checkpoint(tx).await? != prepared.previous {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let changed = sqlx::query("UPDATE runtime_image_checkpoint SET generation=$1,sequence=$2,digest=$3 WHERE singleton AND profile=3 AND namespace=$4 AND witness=$5")
            .bind(i64::try_from(prepared.next.generation).map_err(|_| RuntimeError::InvalidBinding)?)
            .bind(i64::try_from(prepared.next.sequence).map_err(|_| RuntimeError::InvalidBinding)?)
            .bind(&prepared.next.digest).bind(&self.deployment.namespace).bind(&self.deployment.witness).execute(&mut **tx).await?.rows_affected();
        if changed != 1 {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(())
    }

    pub(super) async fn fdb_checkpoint(
        &self,
        runtime: &RuntimeAuthority,
        tx: &FdbTransaction,
    ) -> Result<Checkpoint, RuntimeError> {
        if runtime.execution.deployment != self.deployment.witness
            || runtime.execution.root.bytes()
                != foundationdb::tuple::pack(&(
                    "zoen-runtime-v1",
                    self.deployment.namespace.as_str(),
                ))
            || runtime.execution.source.as_ref().map(|s| s.cell())
                != Some(self.deployment.source.as_str())
        {
            return Err(RuntimeError::DeploymentMismatch);
        }
        let source = tx
            .get(&runtime.execution.root.pack(&("relay-source/1",)), false)
            .await?;
        if source.as_deref() != Some(self.deployment.source.as_bytes()) {
            return Err(RuntimeError::DeploymentMismatch);
        }
        let bytes = tx
            .get(&key(&runtime.execution), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if bytes.len() > 4096 {
            return Err(RuntimeError::InvalidBinding);
        }
        serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)
    }

    pub(super) async fn write_fdb(
        &self,
        runtime: &RuntimeAuthority,
        tx: &FdbTransaction,
        prepared: &Prepared,
    ) -> Result<(), RuntimeError> {
        if self.fdb_checkpoint(runtime, tx).await? != prepared.previous {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        self.anchor
            .reader()
            .pending_subject_commitments(prepared)
            .map_err(error)?;
        let history = history(&runtime.execution, prepared.next.sequence);
        if tx.get(&history, false).await?.is_some() {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        tx.set(
            &history,
            &serde_json::to_vec(prepared).map_err(|_| RuntimeError::InvalidBinding)?,
        );
        tx.set(
            &key(&runtime.execution),
            &serde_json::to_vec(&prepared.next).map_err(|_| RuntimeError::InvalidBinding)?,
        );
        Ok(())
    }
}

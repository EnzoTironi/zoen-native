use super::*;
use foundationdb::Transaction as FdbTransaction;
use sqlx::{Postgres, Transaction};

pub(super) fn key(execution: &crate::execution::Execution) -> Vec<u8> {
    execution.root.pack(&("managed-images/checkpoint/1",))
}

impl RuntimeAuthority {
    pub(crate) async fn image_checkpoints(
        &self,
        deployment: &Deployment,
    ) -> Result<(Checkpoint, Checkpoint), RuntimeError> {
        let (generation, sequence, digest): (i64, i64, String) = sqlx::query_as(
            "SELECT generation,sequence,digest FROM runtime_image_checkpoint WHERE namespace=$1 AND witness=$2 AND agent=$3 AND device=$4",
        ).bind(&deployment.namespace).bind(&deployment.witness).bind(&deployment.agent)
            .bind(&deployment.device).fetch_one(&self.finance.pool).await?;
        let sql = Checkpoint {
            generation: u64::try_from(generation).map_err(|_| RuntimeError::InvalidBinding)?,
            sequence: u64::try_from(sequence).map_err(|_| RuntimeError::InvalidBinding)?,
            digest,
        };
        let transaction = self.execution.transaction().await?;
        let bytes = transaction
            .get(&key(&self.execution), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let fdb = serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
        Ok((sql, fdb))
    }
}

impl DispatchCheckpoint {
    pub(crate) async fn write_fdb(
        &self,
        execution: &crate::execution::Execution,
        transaction: &FdbTransaction,
    ) -> Result<(), RuntimeError> {
        if execution.deployment != self.deployment.witness
            || execution.source.as_ref().map(|source| source.cell())
                != Some(self.deployment.source.as_str())
            || execution.root.bytes()
                != foundationdb::tuple::pack(&(
                    "zoen-runtime-v1",
                    self.deployment.namespace.as_str(),
                ))
        {
            return Err(RuntimeError::DeploymentMismatch);
        }
        let key = key(execution);
        let bytes = transaction
            .get(&key, false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let old: Checkpoint =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
        if old != self.prepared.previous {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        self.write_dispatch_reference(execution, transaction)
            .await?;
        transaction.set(
            &key,
            &serde_json::to_vec(&self.prepared.next).map_err(|_| RuntimeError::InvalidBinding)?,
        );
        Ok(())
    }

    pub(crate) async fn write_sql(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
    ) -> Result<(), RuntimeError> {
        let old = &self.prepared.previous;
        let next = &self.prepared.next;
        let changed = sqlx::query("UPDATE runtime_image_checkpoint SET generation=$1,sequence=$2,digest=$3 WHERE singleton AND namespace=$4 AND witness=$5 AND agent=$6 AND device=$7 AND generation=$8 AND sequence=$9 AND digest=$10")
            .bind(i64::try_from(next.generation).map_err(|_| RuntimeError::InvalidBinding)?)
            .bind(i64::try_from(next.sequence).map_err(|_| RuntimeError::InvalidBinding)?)
            .bind(&next.digest).bind(&self.deployment.namespace).bind(&self.deployment.witness)
            .bind(&self.deployment.agent).bind(&self.deployment.device)
            .bind(i64::try_from(old.generation).map_err(|_| RuntimeError::InvalidBinding)?)
            .bind(i64::try_from(old.sequence).map_err(|_| RuntimeError::InvalidBinding)?)
            .bind(&old.digest).execute(&mut **transaction).await?.rows_affected();
        if changed != 1 {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(())
    }
}

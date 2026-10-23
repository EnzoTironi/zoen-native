use super::*;
use foundationdb::RangeOption;
use std::path::Path;
use std::sync::Arc;

impl RuntimeAuthority {
    /// Bootstraps only an unused paired execution namespace. The independent
    /// local directory must be outside the SQL/FDB backup and restore domain.
    /// This configures a detector; continuity/output production guards stay shut.
    pub async fn initialize_managed_image_continuity(
        mut self,
        directory: &Path,
        agent: &str,
        device: &str,
    ) -> Result<Self, RuntimeError> {
        let deployment = self.configured_image_deployment(agent, device).await?;
        let mut sql = self.finance.begin().await?;
        sqlx::query("LOCK TABLE runtime_attempts IN SHARE MODE")
            .execute(&mut *sql)
            .await?;
        sqlx::query("LOCK TABLE runtime_image_checkpoint IN EXCLUSIVE MODE")
            .execute(&mut *sql)
            .await?;
        let retained: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runtime_attempts)")
            .fetch_one(&mut *sql)
            .await?;
        if retained {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let fdb = self.execution.transaction().await?;
        let mut range = RangeOption::from(self.execution.root.range());
        range.limit = Some(4);
        let rows = fdb.get_range(&range, 1, false).await?;
        let allowed = [
            self.execution.root.pack(&("deployment",)),
            self.execution.root.pack(&("relay-source/1",)),
            stores::key(&self.execution),
        ];
        if rows.more()
            || rows
                .iter()
                .any(|row| !allowed.iter().any(|key| row.key() == key.as_slice()))
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let anchor = match std::fs::symlink_metadata(directory) {
            Ok(_) => AnchorWriter::open(directory, deployment.clone()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                AnchorWriter::create(directory, deployment.clone())
            }
            Err(_) => return Err(RuntimeError::Unavailable),
        }
        .map_err(error)?;
        let genesis = anchor.reader().checkpoint().map_err(error)?;
        if genesis.generation != 1 || genesis.sequence != 0 {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let key = stores::key(&self.execution);
        if let Some(bytes) = fdb.get(&key, false).await? {
            let old: Checkpoint =
                serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
            if old != genesis {
                return Err(RuntimeError::RestoreUnreconciled);
            }
        } else {
            fdb.set(
                &key,
                &serde_json::to_vec(&genesis).map_err(|_| RuntimeError::InvalidBinding)?,
            );
        }
        sqlx::query("INSERT INTO runtime_image_checkpoint(namespace,witness,agent,device,generation,sequence,digest) VALUES($1,$2,$3,$4,1,0,$5) ON CONFLICT DO NOTHING")
            .bind(&deployment.namespace).bind(&deployment.witness).bind(&deployment.agent)
            .bind(&deployment.device).bind(&genesis.digest).execute(&mut *sql).await?;
        let existing: Option<String> = sqlx::query_scalar("SELECT digest FROM runtime_image_checkpoint WHERE namespace=$1 AND witness=$2 AND agent=$3 AND device=$4 AND generation=1 AND sequence=0")
            .bind(&deployment.namespace).bind(&deployment.witness).bind(&deployment.agent)
            .bind(&deployment.device).fetch_optional(&mut *sql).await?;
        if existing.as_deref() != Some(&genesis.digest) {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        fdb.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        sql.commit().await?;
        if self.image_checkpoints(&deployment).await? != (genesis.clone(), genesis) {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        self.managed_images = Some(Arc::new(tokio::sync::Mutex::new(ManagedImages {
            anchor,
            deployment,
        })));
        Ok(self)
    }

    /// Cold-open never creates a replacement anchor. A matching old SQL/FDB
    /// pair cannot reset the independently retained checkpoint. Recovery of a
    /// known interrupted transition never creates a fresh dispatch permit.
    pub async fn open_managed_image_continuity(
        mut self,
        directory: &Path,
        agent: &str,
        device: &str,
    ) -> Result<Self, RuntimeError> {
        let deployment = self.configured_image_deployment(agent, device).await?;
        let mut anchor = AnchorWriter::open(directory, deployment.clone()).map_err(error)?;
        let (sql, fdb) = self.image_checkpoints(&deployment).await?;
        match anchor.reconcile(&sql, &fdb).map_err(error)? {
            Recovery::Clean | Recovery::BothBefore | Recovery::BothAfter => {}
            Recovery::SqlBefore | Recovery::FdbBefore => {
                return Err(RuntimeError::RestoreUnreconciled)
            }
        }
        self.managed_images = Some(Arc::new(tokio::sync::Mutex::new(ManagedImages {
            anchor,
            deployment,
        })));
        Ok(self)
    }
}

use super::*;
use foundationdb::RangeOption;
use std::{path::Path, sync::Arc};

impl RuntimeAuthority {
    pub(super) async fn shared_deployment(&self) -> Result<RuntimeDeployment, RuntimeError> {
        let (namespace, witness): (String, String) = sqlx::query_as(
            "SELECT namespace,witness FROM runtime_deployment_binding WHERE singleton",
        )
        .fetch_one(&self.finance.pool)
        .await?;
        if witness != self.execution.deployment
            || self.execution.root.bytes()
                != foundationdb::tuple::pack(&("zoen-runtime-v1", namespace.as_str()))
        {
            return Err(RuntimeError::DeploymentMismatch);
        }
        let source = self
            .execution
            .source
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .cell()
            .to_owned();
        self.execution.transaction().await?;
        Ok(RuntimeDeployment {
            namespace,
            witness,
            source,
        })
    }

    /// Bootstrap one unused paired runtime. The operator must retain this
    /// private local directory independently of SQL/FDB/vault backups.
    /// This configures policy continuity; paid dispatch/output stay closed.
    pub async fn initialize_runtime_continuity(
        mut self,
        directory: &Path,
    ) -> Result<Self, RuntimeError> {
        if !directory.is_absolute()
            || directory
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(RuntimeError::InvalidBinding);
        }
        if self.runtime_continuity.is_some() || self.managed_images.is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
        let deployment = self.shared_deployment().await?;
        let mut sql = self.finance.continuity_transaction().await?;
        sqlx::query("SELECT singleton FROM runtime_deployment_binding WHERE singleton FOR UPDATE")
            .fetch_one(&mut *sql)
            .await?;
        for statement in [
            "LOCK TABLE runtime_attempts IN SHARE MODE",
            "LOCK TABLE runtime_policies IN SHARE MODE",
            "LOCK TABLE runtime_periods IN SHARE MODE",
            "LOCK TABLE runtime_image_checkpoint IN EXCLUSIVE MODE",
        ] {
            sqlx::query(statement).execute(&mut *sql).await?;
        }
        let used: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runtime_attempts) OR EXISTS(SELECT 1 FROM runtime_policies) OR EXISTS(SELECT 1 FROM runtime_periods)").fetch_one(&mut *sql).await?;
        if used {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let fdb = self.execution.transaction().await?;
        let mut range = RangeOption::from(self.execution.root.range());
        range.limit = Some(4);
        let rows = fdb.get_range(&range, 1, false).await?;
        let allowed = [
            self.execution.root.pack(&("deployment",)),
            self.execution.root.pack(&("relay-source/1",)),
            key(&self.execution),
        ];
        if rows.more()
            || rows
                .iter()
                .any(|row| !allowed.iter().any(|key| row.key() == key.as_slice()))
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let old_fdb = fdb.get(&key(&self.execution), false).await?;
        let old_sql: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runtime_image_checkpoint)")
                .fetch_one(&mut *sql)
                .await?;
        let anchor = match std::fs::symlink_metadata(directory) {
            Ok(_) => AnchorWriter::open_runtime(directory, deployment.clone()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if old_sql || old_fdb.is_some() {
                    return Err(RuntimeError::RestoreUnreconciled);
                }
                AnchorWriter::create_runtime(directory, deployment.clone())
            }
            Err(_) => return Err(RuntimeError::Unavailable),
        }
        .map_err(error)?;
        let genesis = anchor.reader().checkpoint().map_err(error)?;
        if genesis.generation != 1
            || genesis.sequence != 0
            || !anchor
                .reader()
                .subject_commitments()
                .map_err(error)?
                .is_empty()
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        if let Some(bytes) = old_fdb {
            let old: Checkpoint =
                serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
            if old != genesis {
                return Err(RuntimeError::RestoreUnreconciled);
            }
        } else {
            fdb.set(
                &key(&self.execution),
                &serde_json::to_vec(&genesis).map_err(|_| RuntimeError::InvalidBinding)?,
            );
        }
        sqlx::query("INSERT INTO runtime_image_checkpoint(namespace,witness,agent,device,generation,sequence,digest,profile) VALUES($1,$2,NULL,NULL,1,0,$3,3) ON CONFLICT DO NOTHING")
            .bind(&deployment.namespace).bind(&deployment.witness).bind(&genesis.digest).execute(&mut *sql).await?;
        let mut managed = ManagedRuntime { anchor, deployment };
        if managed.sql_checkpoint(&mut sql).await? != genesis {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        fdb.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        sql.commit().await?;
        managed.reconcile(&self).await?;
        self.runtime_continuity = Some(Arc::new(tokio::sync::Mutex::new(managed)));
        Ok(self)
    }

    /// Open retained history; missing/replaced anchors are never initialized.
    /// Pending original policy intents recover under fresh owner authorization.
    pub async fn open_runtime_continuity(mut self, directory: &Path) -> Result<Self, RuntimeError> {
        if !directory.is_absolute()
            || directory
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(RuntimeError::InvalidBinding);
        }
        if self.runtime_continuity.is_some() || self.managed_images.is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
        let deployment = self.shared_deployment().await?;
        let anchor = AnchorWriter::open_runtime(directory, deployment.clone()).map_err(error)?;
        let mut managed = ManagedRuntime { anchor, deployment };
        managed.reconcile(&self).await?;
        self.runtime_continuity = Some(Arc::new(tokio::sync::Mutex::new(managed)));
        Ok(self)
    }
}

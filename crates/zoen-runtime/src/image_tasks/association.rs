//! A retained original result, never a new send or output capability.
use super::*;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Association {
    task: String,
    evidence: String,
    nonce: String,
}
impl crate::execution::Execution {
    pub(crate) async fn associate_image_capture(
        &self,
        task: &ImageTask,
        nonce: &str,
        digest: &str,
        custody: &custody::Custody,
    ) -> Result<(), RuntimeError> {
        task.validate()?;
        let run = &task.binding.context.run_id;
        let attempt = &task.binding.context.attempt_id;
        let trx = self.transaction().await?;
        let admitted = trx.get(&self.attempt(attempt, "admitted"), false).await?;
        if admitted.as_deref() != Some(nonce.as_bytes()) {
            return Err(RuntimeError::InvalidBinding);
        }
        let prepared = trx
            .get(&self.attempt(attempt, "prepared"), false)
            .await?
            .ok_or(RuntimeError::InvalidBinding)?;
        let frozen: Binding = custody.open(&format!("prepared/{attempt}"), None, &prepared)?;
        if frozen != task.binding {
            return Err(RuntimeError::InvalidBinding);
        }
        // A generic fixture or another producer cannot claim a native task run.
        if trx
            .get(&self.run(run, "image-record/1"), false)
            .await?
            .is_none()
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let value = Association {
            task: task.task_digest.clone(),
            evidence: digest.into(),
            nonce: nonce.into(),
        };
        let subject = format!("image-result/1/{run}");
        let key = self.run(run, "image-result/1");
        if let Some(old) = trx.get(&key, false).await? {
            let old: Association = custody.open(&subject, None, &old)?;
            if old.task != value.task || old.evidence != value.evidence || old.nonce != value.nonce
            {
                return Err(RuntimeError::InvalidBinding);
            }
            return Ok(());
        }
        let (_, sealed) = custody.seal(&subject, &value)?;
        trx.set(&key, &sealed);
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }
    pub(crate) async fn image_capture_digest(
        &self,
        task: &ImageTask,
        custody: &custody::Custody,
    ) -> Result<String, RuntimeError> {
        let trx = self.transaction().await?;
        let run = &task.binding.context.run_id;
        let bytes = trx
            .get(&self.run(run, "image-result/1"), false)
            .await?
            .ok_or(RuntimeError::Unavailable)?;
        let value: Association = custody.open(&format!("image-result/1/{run}"), None, &bytes)?;
        let admitted = trx
            .get(
                &self.attempt(&task.binding.context.attempt_id, "admitted"),
                false,
            )
            .await?;
        if value.task != task.task_digest
            || !valid_digest(&value.evidence)
            || admitted.as_deref() != Some(value.nonce.as_bytes())
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(value.evidence)
    }
    pub(crate) async fn image_evidence_bytes(
        &self,
        attempt: &str,
        digest: &str,
    ) -> Result<Option<Vec<u8>>, RuntimeError> {
        let trx = self.transaction().await?;
        let Some(root) = trx
            .get(&self.root.pack(&("evidence", attempt, digest)), false)
            .await?
        else {
            return Ok(None);
        };
        self.read_evidence_payload(attempt, digest, &root)
            .await
            .map(Some)
    }
}

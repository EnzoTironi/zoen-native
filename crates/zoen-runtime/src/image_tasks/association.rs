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
        self.image_task_original_in(&trx, task, custody).await?;
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

impl RuntimeAuthority {
    /// Repairs only the association of a previously retained original result.
    /// It cannot mint a claim, admission witness, provider send or native task.
    pub(crate) async fn retained_image_capture(
        &self,
        task: &ImageTask,
    ) -> Result<(String, capture::ImageCapture), RuntimeError> {
        task.validate()?;
        let attempt = &task.binding.context.attempt_id;
        let (digest, repair) = match self
            .execution
            .image_capture_digest(task, &self.custody)
            .await
        {
            Ok(digest) => (digest, false),
            Err(RuntimeError::Unavailable) => {
                let mut candidates = std::collections::BTreeSet::new();
                let sql: Vec<String> = sqlx::query_scalar(
                    "SELECT digest FROM runtime_evidence WHERE attempt=$1 ORDER BY digest LIMIT 3",
                )
                .bind(attempt)
                .fetch_all(&self.finance.pool)
                .await?;
                candidates.extend(sql);
                let trx = self.execution.transaction().await?;
                let sub = self
                    .execution
                    .root
                    .subspace(&("evidence", attempt.as_str()));
                let mut range = foundationdb::RangeOption::from(sub.range());
                range.limit = Some(3);
                let rows = trx.get_range(&range, 1, false).await?;
                if rows.more() {
                    return Err(RuntimeError::InvalidBinding);
                }
                for row in rows.iter() {
                    let (digest,): (String,) = sub
                        .unpack(row.key())
                        .map_err(|_| RuntimeError::InvalidBinding)?;
                    candidates.insert(digest);
                }
                if candidates.is_empty() {
                    return Err(RuntimeError::Unavailable);
                }
                if candidates.len() != 1 {
                    return Err(RuntimeError::InvalidBinding);
                }
                (
                    candidates
                        .into_iter()
                        .next()
                        .ok_or(RuntimeError::Unavailable)?,
                    true,
                )
            }
            Err(error) => return Err(error),
        };
        if !valid_digest(&digest) {
            return Err(RuntimeError::InvalidBinding);
        }
        let sealed=match self.execution.image_evidence_bytes(attempt,&digest).await? {
            Some(bytes)=>bytes,
            None=>sqlx::query_scalar("SELECT e.sealed FROM runtime_evidence e JOIN runtime_attempts a USING(attempt) WHERE e.attempt=$1 AND e.digest=$2 AND a.binding_digest=$3")
                .bind(attempt).bind(&digest).bind(task.binding.digest()?).fetch_optional(&self.finance.pool).await?.ok_or(RuntimeError::Unavailable)?,
        };
        let plain = self.custody.open_bounded_bytes(
            &format!("evidence/{attempt}"),
            Some(&digest),
            &sealed,
            capture::MAXIMUM,
        )?;
        let capture = capture::ImageCapture::decode(&plain)?;
        if capture.task != *task {
            return Err(RuntimeError::InvalidBinding);
        }
        // Existing association validation checks the same immutable native record,
        // prepared binding and ORIGINAL nonce retained inside authenticated capture.
        self.execution
            .associate_image_capture(task, &capture.nonce, &digest, &self.custody)
            .await?;
        if repair
            && self
                .execution
                .image_capture_digest(task, &self.custody)
                .await?
                != digest
        {
            return Err(RuntimeError::Unavailable);
        }
        Ok((digest, capture))
    }
}

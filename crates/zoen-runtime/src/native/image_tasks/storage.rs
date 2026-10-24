use super::*;
use crate::{
    execution::{Execution, Fence, Lease},
    finance::NativeActivation,
};
use foundationdb::Transaction;

impl Execution {
    pub(crate) async fn image_task_original_in(
        &self,
        trx: &Transaction,
        task: &ImageTask,
        custody: &Custody,
    ) -> Result<(), RuntimeError> {
        let run = &task.binding.context.run_id;
        let bytes = trx
            .get(&self.run(run, "image-record/1"), false)
            .await?
            .ok_or(RuntimeError::InvalidBinding)?;
        let record: Record = custody.open(&format!("native-image-run/1/{run}"), None, &bytes)?;
        record.validate(
            run,
            self.source
                .as_ref()
                .ok_or(RuntimeError::CoreAuthorityUnavailable)?
                .cell(),
        )?;
        if record.task != *task {
            return Err(RuntimeError::InvalidBinding);
        }
        let pin = trx
            .get(
                &self
                    .native_references(&record.original)
                    .pack(&(run.as_str(),)),
                false,
            )
            .await?;
        if pin.as_deref() != Some(record.original.manifest_digest.as_bytes()) {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
    pub(super) async fn read_image_task(
        &self,
        run: &str,
        custody: &Custody,
    ) -> Result<(Record, String), RuntimeError> {
        let trx = self.transaction().await?;
        let bytes = trx
            .get(&self.run(run, "image-record/1"), false)
            .await?
            .ok_or(RuntimeError::Denied)?;
        let record: Record = custody.open(&format!("native-image-run/1/{run}"), None, &bytes)?;
        record.validate(
            run,
            self.source
                .as_ref()
                .ok_or(RuntimeError::CoreAuthorityUnavailable)?
                .cell(),
        )?;
        Ok((record, hash(&bytes)))
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn create_image_task(
        &self,
        runtime: &RuntimeAuthority,
        record: &Record,
        facts: &ImageTaskFacts,
        fence: &Fence,
        agent: &Identity,
        owner: &Identity,
        certificate: &str,
    ) -> Result<(), RuntimeError> {
        record.task.validate()?;
        let run = &record.task.request.context.run_id;
        let key = self.run(run, "image-record/1");
        let (record_digest, sealed) = runtime
            .custody
            .seal(&format!("native-image-run/1/{run}"), record)?;
        let (_, prepared) = runtime.custody.seal(
            &format!("prepared/{}", record.task.binding.context.attempt_id),
            &record.task.binding,
        )?;
        let scope = ActivationScope {
            namespace: self.root.bytes().to_vec(),
            deployment: self.deployment.clone(),
            previous: Some(record.original.clone()),
            target: record.original.clone(),
            fence: fence.clone(),
            effects: record_digest,
        };
        let admission: NativeActivation = runtime
            .finance
            .native_directory(
                agent,
                owner,
                &record.task.request.context.device,
                certificate,
            )
            .await?
            .authorize(scope.clone())
            .await?;
        admission.check(&scope)?;
        let trx = self.transaction().await?;
        runtime.image_output_scope()?.check_in(self, &trx).await?;
        Self::lease(
            &trx,
            &self.native_lease(&record.original.context.principal),
            fence,
            trx.get_read_version().await?,
        )
        .await?;
        if self
            .native_root(&trx, &record.original.context.principal)
            .await?
            .as_ref()
            != Some(&record.original)
        {
            return Err(RuntimeError::Denied);
        }
        let head = self
            .source
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .head_in(&trx, facts.space())
            .await
            .map_err(|_| RuntimeError::Denied)?
            .ok_or(RuntimeError::Denied)?;
        if head.seq() != facts.frontier().seq() || head.hash() != facts.frontier().hash() {
            return Err(RuntimeError::Denied);
        }
        if let Some(old) = trx.get(&key, false).await? {
            let old: Record =
                runtime
                    .custody
                    .open(&format!("native-image-run/1/{run}"), None, &old)?;
            old.task.validate()?;
            let a = &old.task;
            let b = &record.task;
            if a.approval_hash != b.approval_hash
                || a.request_id != b.request_id
                || a.source_digest != b.source_digest
                || a.request.prompt != b.request.prompt
                || a.binding.price != b.binding.price
                || old.source != record.source
                || old.original.context.principal != record.original.context.principal
            {
                return Err(RuntimeError::InvalidBinding);
            }
            admission.consume(&scope)?;
            return Ok(());
        }
        let attempt = &record.task.binding.context.attempt_id;
        for part in ["prepared", "closed", "admitted"] {
            if trx
                .get(&self.attempt(attempt, part), false)
                .await?
                .is_some()
            {
                return Err(RuntimeError::InvalidBinding);
            }
        }
        trx.set(&key, &sealed);
        trx.set(&self.attempt(attempt, "prepared"), &prepared);
        trx.set(
            &self
                .native_references(&record.original)
                .pack(&(run.as_str(),)),
            record.original.manifest_digest.as_bytes(),
        );
        admission.consume(&scope)?;
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }
    pub(super) async fn acquire_image_task(
        &self,
        record: &Record,
        expected: &str,
    ) -> Result<(Fence, Fence), RuntimeError> {
        let trx = self.transaction().await?;
        let prior = trx
            .get(
                &self.run(&record.task.request.context.run_id, "image-record/1"),
                false,
            )
            .await?
            .ok_or(RuntimeError::Denied)?;
        if hash(&prior) != expected {
            return Err(RuntimeError::InvalidBinding);
        }
        let version = trx.get_read_version().await?;
        let mut fences = Vec::new();
        for key in [
            self.run(&record.task.request.context.run_id, "lease"),
            self.native_lease(&record.original.context.principal),
        ] {
            let token = if let Some(value) = trx.get(&key, false).await? {
                let prior: Lease =
                    serde_json::from_slice(&value).map_err(|_| RuntimeError::InvalidBinding)?;
                if prior.expires > version {
                    return Err(RuntimeError::Denied);
                }
                prior
                    .token
                    .checked_add(1)
                    .ok_or(RuntimeError::InvalidBinding)?
            } else {
                1
            };
            let fence = Fence {
                holder: types::new_id("image-worker"),
                token,
            };
            trx.set(
                &key,
                &serde_json::to_vec(&Lease {
                    holder: fence.holder.clone(),
                    token,
                    expires: version
                        .checked_add(super::super::storage::LEASE_VERSIONS)
                        .ok_or(RuntimeError::Unavailable)?,
                })
                .map_err(|_| RuntimeError::InvalidBinding)?,
            );
            fences.push(fence);
        }
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok((fences.remove(0), fences.remove(0)))
    }
    pub(super) async fn verify_image_task(
        &self,
        trx: &Transaction,
        step: &ImageStep,
        custody: &Custody,
    ) -> Result<(), RuntimeError> {
        let r = &step.record;
        let b = &r.task.binding;
        let facts = &step.facts;
        r.task.validate()?;
        let record = trx
            .get(&self.run(&b.context.run_id, "image-record/1"), false)
            .await?
            .ok_or(RuntimeError::Denied)?;
        let prepared = trx
            .get(&self.attempt(&b.context.attempt_id, "prepared"), false)
            .await?
            .ok_or(RuntimeError::Denied)?;
        let frozen: Binding = custody.open(
            &format!("prepared/{}", b.context.attempt_id),
            None,
            &prepared,
        )?;
        let pin = trx
            .get(
                &self
                    .native_references(&r.original)
                    .pack(&(b.context.run_id.as_str(),)),
                false,
            )
            .await?;
        let current = self.native_root(trx, &r.original.context.principal).await?;
        let head = self
            .source
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .head_in(trx, &b.context.space)
            .await
            .map_err(|_| RuntimeError::Denied)?
            .ok_or(RuntimeError::Denied)?;
        if hash(&record) != step.record_digest
            || frozen != *b
            || pin.as_deref() != Some(r.original.manifest_digest.as_bytes())
            || current.as_ref() != Some(&step.current)
            || head.seq() != facts.frontier().seq()
            || head.hash() != facts.frontier().hash()
            || facts.request() != r.task.request_id
            || facts.content_hash() != r.task.approval_hash
            || facts.proposal().source_sha256 != r.task.source_digest
            || facts.input().prompt != r.task.request.prompt
            || facts.input().definition.price_digest != b.price.digest()?
            || facts.input().definition.version != b.context.definition_version
            || facts.agent() != b.context.agent
            || facts.space() != b.context.space
            || facts.proposal().device != b.context.device
            || facts.proposal().owner != b.context.owner
            || trx
                .get(&self.run(&b.context.run_id, "cancel"), false)
                .await?
                .is_some()
        {
            return Err(RuntimeError::Denied);
        }
        let version = trx.get_read_version().await?;
        Self::lease(
            trx,
            &self.run(&b.context.run_id, "lease"),
            &step.run_fence,
            version,
        )
        .await?;
        Self::lease(
            trx,
            &self.native_lease(&r.original.context.principal),
            &step.device_fence,
            version,
        )
        .await
    }
    pub(super) async fn release_image_task(&self, step: &ImageStep) -> Result<(), RuntimeError> {
        self.release_image_fences(&step.record, &step.run_fence, &step.device_fence)
            .await
    }
    pub(super) async fn release_image_fences(
        &self,
        record: &Record,
        run: &Fence,
        device: &Fence,
    ) -> Result<(), RuntimeError> {
        let trx = self.transaction().await?;
        let version = trx.get_read_version().await?;
        for (key, fence) in [
            (self.run(&record.task.binding.context.run_id, "lease"), run),
            (
                self.native_lease(&record.original.context.principal),
                device,
            ),
        ] {
            let bytes = trx.get(&key, false).await?.ok_or(RuntimeError::Denied)?;
            let mut prior: Lease =
                serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
            if prior.holder != fence.holder || prior.token != fence.token {
                return Err(RuntimeError::Denied);
            }
            prior.expires = version;
            trx.set(
                &key,
                &serde_json::to_vec(&prior).map_err(|_| RuntimeError::InvalidBinding)?,
            );
        }
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }
}

use foundationdb::{RangeOption, Transaction};
use zoen_relay::log::fdb::RuntimeSource;

use super::*;
use crate::{custody::Custody, execution::Lease};

impl Execution {
    pub(in crate::native) fn reply_source(&self) -> Result<&RuntimeSource, RuntimeError> {
        self.source
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)
    }

    pub(super) fn reply_trigger(&self, agent: &str, space: &str, trigger: &str) -> Vec<u8> {
        // Stable across workers, devices, image generations and wake delivery.
        self.root.pack(&("native-trigger/1", agent, space, trigger))
    }

    fn reply_wake(&self, wake: &str) -> Vec<u8> {
        self.root.pack(&("native-wake/1", wake))
    }

    pub(super) async fn reply_in(
        &self,
        trx: &Transaction,
        run: &str,
        custody: &Custody,
    ) -> Result<(RunRecord, String), RuntimeError> {
        let sealed = trx
            .get(&self.run(run, "record/1"), false)
            .await?
            .ok_or(RuntimeError::Denied)?;
        let record: RunRecord = custody.open(&format!("native-run/1/{run}"), None, &sealed)?;
        record.validate(run, self.reply_source()?.cell())?;
        Ok((record, hash(&sealed)))
    }

    pub(super) async fn read_reply(
        &self,
        run: &str,
        custody: &Custody,
    ) -> Result<(RunRecord, String), RuntimeError> {
        self.reply_in(&self.transaction().await?, run, custody)
            .await
    }

    pub(super) async fn activate_replies(
        &self,
        staged: super::super::storage::Staged,
        device_fence: &Fence,
        admission: crate::finance::NativeActivation,
        batch: Batch,
        custody: &Custody,
    ) -> Result<(Root, Vec<ReplyRun>), RuntimeError> {
        let scope = batch.scope(self, &staged, device_fence)?;
        admission.check(&scope)?;
        let trx = self.transaction().await?;
        let source = self.reply_source()?;
        if source.cell() != batch.source {
            return Err(RuntimeError::InvalidBinding);
        }
        let head = source
            .head_in(&trx, &batch.space)
            .await
            .map_err(|_| RuntimeError::Denied)?
            .ok_or(RuntimeError::Denied)?;
        if head.seq() != batch.head.seq || head.hash() != batch.head.hash {
            return Err(RuntimeError::Denied);
        }
        self.native_activation(&trx, &staged, device_fence).await?;
        let principal = &staged.root.context.principal;
        let mut runs = Vec::new();
        let mut triggers = std::collections::BTreeSet::new();
        for candidate in &batch.runs {
            if !digest(&candidate.trigger) || !triggers.insert(&candidate.trigger) {
                return Err(RuntimeError::InvalidBinding);
            }
            let key = self.reply_trigger(&principal.agent, &batch.space, &candidate.trigger);
            if let Some(prior) = trx.get(&key, false).await? {
                let run = std::str::from_utf8(&prior).map_err(|_| RuntimeError::InvalidBinding)?;
                if !id(run) {
                    return Err(RuntimeError::InvalidBinding);
                }
                let (record, _) = self.reply_in(&trx, run, custody).await?;
                if record.request.context.agent != principal.agent
                    || record.request.context.owner != principal.owner
                    || record.intent.space() != batch.space
                    || record.intent.trigger() != candidate.trigger
                {
                    return Err(RuntimeError::InvalidBinding);
                }
                runs.push(record.locator);
                continue;
            }
            let record: RunRecord = custody.open(
                &format!("native-run/1/{}", candidate.locator.run),
                None,
                &candidate.record,
            )?;
            record.validate(&candidate.locator.run, source.cell())?;
            let frozen: Binding = custody.open(
                &format!("prepared/{}", candidate.locator.attempt),
                None,
                &candidate.binding,
            )?;
            let wake: Wake = custody.open(
                &format!("native-wake/1/{}", candidate.locator.wake),
                None,
                &candidate.wake,
            )?;
            if record.locator != candidate.locator
                || record.original != staged.root
                || record.head != batch.head
                || record.intent.space() != batch.space
                || record.intent.trigger() != candidate.trigger
                || frozen != record.binding
                || wake.locator != candidate.locator
                || wake.record_digest != hash(&candidate.record)
            {
                return Err(RuntimeError::InvalidBinding);
            }
            let record_key = self.run(&candidate.locator.run, "record/1");
            let prepared_key = self.attempt(&candidate.locator.attempt, "prepared");
            let wake_key = self.reply_wake(&candidate.locator.wake);
            let reference_key = self
                .native_references(&staged.root)
                .pack(&(candidate.locator.run.as_str(),));
            for key in [&record_key, &prepared_key, &wake_key, &reference_key] {
                if trx.get(key, false).await?.is_some() {
                    return Err(RuntimeError::InvalidBinding);
                }
            }
            for part in ["closed", "admitted"] {
                if trx
                    .get(&self.attempt(&candidate.locator.attempt, part), false)
                    .await?
                    .is_some()
                {
                    return Err(RuntimeError::InvalidBinding);
                }
            }
            trx.set(&record_key, &candidate.record);
            trx.set(&prepared_key, &candidate.binding);
            trx.set(&wake_key, &candidate.wake);
            trx.set(&reference_key, staged.root.manifest_digest.as_bytes());
            trx.set(&key, candidate.locator.run.as_bytes());
            runs.push(candidate.locator.clone());
        }
        #[cfg(test)]
        if batch.cut == 16 {
            return Err(RuntimeError::Unavailable);
        }
        admission.consume(&scope)?;
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if batch.cut == 17 {
            return Err(RuntimeError::Unavailable);
        }
        Ok((staged.root, runs))
    }

    pub(super) async fn acquire_reply(
        &self,
        record: &RunRecord,
        expected: &str,
    ) -> Result<ReplyFences, RuntimeError> {
        let trx = self.transaction().await?;
        self.reply_record_scope(&trx, record, expected).await?;
        let fences = self
            .acquire_reply_in(&trx, record)
            .await?
            .ok_or(RuntimeError::Denied)?;
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(fences)
    }

    pub(super) async fn acquire_reply_in(
        &self,
        trx: &Transaction,
        record: &RunRecord,
    ) -> Result<Option<ReplyFences>, RuntimeError> {
        let version = trx.get_read_version().await?;
        let principal = &record.original.context.principal;
        let run_key = self.run(&record.locator.run, "lease");
        let device_key = self.native_lease(principal);
        let holder = roda_types::new_id("reply-worker");
        let mut fences = Vec::new();
        for key in [&run_key, &device_key] {
            let token = if let Some(prior) = trx.get(key, false).await? {
                if prior.len() > 8192 {
                    return Err(RuntimeError::InvalidBinding);
                }
                let prior: Lease =
                    serde_json::from_slice(&prior).map_err(|_| RuntimeError::InvalidBinding)?;
                if !id(&prior.holder) || prior.token <= 0 || prior.expires <= 0 {
                    return Err(RuntimeError::InvalidBinding);
                }
                if prior.expires > version {
                    return Ok(None);
                }
                prior
                    .token
                    .checked_add(1)
                    .ok_or(RuntimeError::InvalidBinding)?
            } else {
                1
            };
            let lease = Lease {
                holder: holder.clone(),
                token,
                expires: version
                    .checked_add(super::super::storage::LEASE_VERSIONS)
                    .ok_or(RuntimeError::Unavailable)?,
            };
            trx.set(
                key,
                &serde_json::to_vec(&lease).map_err(|_| RuntimeError::InvalidBinding)?,
            );
            fences.push(Fence {
                holder: holder.clone(),
                token,
            });
        }
        Ok(Some(ReplyFences {
            run: fences.remove(0),
            device: fences.remove(0),
        }))
    }

    pub(super) async fn reply_original_scope(
        &self,
        trx: &Transaction,
        record: &RunRecord,
        expected: &str,
    ) -> Result<(), RuntimeError> {
        let sealed = trx
            .get(&self.run(&record.locator.run, "record/1"), false)
            .await?
            .ok_or(RuntimeError::Denied)?;
        if hash(&sealed) != expected {
            return Err(RuntimeError::InvalidBinding);
        }
        record.validate(&record.locator.run, self.reply_source()?.cell())?;
        let trigger = trx
            .get(
                &self.reply_trigger(
                    &record.request.context.agent,
                    record.intent.space(),
                    record.intent.trigger(),
                ),
                false,
            )
            .await?;
        if trigger.as_deref() != Some(record.locator.run.as_bytes()) {
            return Err(RuntimeError::InvalidBinding);
        }
        let reference = trx
            .get(
                &self
                    .native_references(&record.original)
                    .pack(&(record.locator.run.as_str(),)),
                false,
            )
            .await?;
        if reference.as_deref() != Some(record.original.manifest_digest.as_bytes()) {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }

    async fn reply_record_scope(
        &self,
        trx: &Transaction,
        record: &RunRecord,
        expected: &str,
    ) -> Result<(), RuntimeError> {
        self.reply_original_scope(trx, record, expected).await?;
        if trx
            .get(&self.run(&record.locator.run, "cancel"), false)
            .await?
            .is_some()
            || trx
                .get(&self.attempt(&record.locator.attempt, "closed"), false)
                .await?
                .is_some()
        {
            return Err(RuntimeError::Denied);
        }
        Ok(())
    }

    pub(super) async fn verify_reply(
        &self,
        record: &RunRecord,
        expected: &str,
        current: &Root,
        facts: &ReplyFacts,
        fences: &ReplyFences,
        custody: &Custody,
    ) -> Result<(), RuntimeError> {
        let trx = self.transaction().await?;
        self.verify_reply_in(&trx, record, expected, current, facts, fences, custody)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn verify_reply_in(
        &self,
        trx: &Transaction,
        record: &RunRecord,
        expected: &str,
        current: &Root,
        facts: &ReplyFacts,
        fences: &ReplyFences,
        custody: &Custody,
    ) -> Result<(), RuntimeError> {
        self.reply_record_scope(trx, record, expected).await?;
        let principal = &record.original.context.principal;
        let version = trx.get_read_version().await?;
        Self::lease(
            trx,
            &self.run(&record.locator.run, "lease"),
            &fences.run,
            version,
        )
        .await?;
        Self::lease(trx, &self.native_lease(principal), &fences.device, version).await?;
        let head = self
            .reply_source()?
            .head_in(trx, record.intent.space())
            .await
            .map_err(|_| RuntimeError::Denied)?
            .ok_or(RuntimeError::Denied)?;
        if head.seq() != facts.frontier().seq()
            || head.hash() != facts.frontier().hash()
            || facts.intent() != &record.intent
            || self.native_root(trx, principal).await?.as_ref() != Some(current)
        {
            return Err(RuntimeError::Denied);
        }
        self.reply_binding_in(trx, record, custody).await?;
        // The caller owns this transaction, including the final admission write.
        Ok(())
    }

    pub(super) async fn reply_binding_in(
        &self,
        trx: &Transaction,
        record: &RunRecord,
        custody: &Custody,
    ) -> Result<(), RuntimeError> {
        let frozen = trx
            .get(&self.attempt(&record.locator.attempt, "prepared"), false)
            .await?
            .ok_or(RuntimeError::InvalidBinding)?;
        let binding: Binding = custody.open(
            &format!("prepared/{}", record.locator.attempt),
            None,
            &frozen,
        )?;
        if binding != record.binding {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }

    pub(super) async fn release_reply(
        &self,
        record: &RunRecord,
        fences: &ReplyFences,
    ) -> Result<(), RuntimeError> {
        let trx = self.transaction().await?;
        let version = trx.get_read_version().await?;
        for (key, fence) in [
            (self.run(&record.locator.run, "lease"), &fences.run),
            (
                self.native_lease(&record.original.context.principal),
                &fences.device,
            ),
        ] {
            let prior = trx.get(&key, false).await?.ok_or(RuntimeError::Denied)?;
            if prior.len() > 8192 {
                return Err(RuntimeError::InvalidBinding);
            }
            let mut lease: Lease =
                serde_json::from_slice(&prior).map_err(|_| RuntimeError::InvalidBinding)?;
            if lease.holder != fence.holder || lease.token != fence.token {
                return Err(RuntimeError::Denied);
            }
            lease.expires = version;
            trx.set(
                &key,
                &serde_json::to_vec(&lease).map_err(|_| RuntimeError::InvalidBinding)?,
            );
        }
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }

    pub(crate) async fn pending_reply_runs(
        &self,
        limit: u16,
        custody: &Custody,
    ) -> Result<Vec<ReplyRun>, RuntimeError> {
        if !(1..=64).contains(&limit) {
            return Err(RuntimeError::InvalidBinding);
        }
        self.reply_source()?;
        let trx = self.transaction().await?;
        let prefix = self.root.subspace(&("native-wake/1",));
        let rows = super::super::storage::bounded_rows(
            &trx,
            RangeOption::from(prefix.range()),
            limit as usize,
            crate::custody::MAX_SEALED,
        )
        .await?;
        let mut runs = Vec::new();
        for (key, sealed) in rows {
            let (id,): (String,) = prefix
                .unpack(&key)
                .map_err(|_| RuntimeError::InvalidBinding)?;
            if !crate::id(&id) {
                return Err(RuntimeError::InvalidBinding);
            }
            let wake: Wake = custody.open(&format!("native-wake/1/{id}"), None, &sealed)?;
            if wake.locator.wake != id {
                return Err(RuntimeError::InvalidBinding);
            }
            let (record, digest) = self.reply_in(&trx, &wake.locator.run, custody).await?;
            if record.locator != wake.locator || digest != wake.record_digest {
                return Err(RuntimeError::InvalidBinding);
            }
            runs.push(wake.locator);
        }
        Ok(runs)
    }
}

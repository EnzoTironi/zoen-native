//! Composite effects share the caller-owned activation transaction.
use super::*;
impl Execution {
    pub(in crate::native) async fn reply_effects_in(
        &self,
        trx: &Transaction,
        root: &Root,
        batch: &Batch,
        custody: &Custody,
        mode: DiscoveryMode,
    ) -> Result<ReplyEffects, RuntimeError> {
        let source = self.reply_source()?;
        if source.cell() != batch.source {
            return Err(RuntimeError::InvalidBinding);
        }
        let head = source
            .head_in(trx, &batch.space)
            .await
            .map_err(|_| RuntimeError::Denied)?
            .ok_or(RuntimeError::Denied)?;
        if mode != DiscoveryMode::Recovery
            && (head.seq() != batch.head.seq || head.hash() != batch.head.hash)
        {
            return Err(RuntimeError::Denied);
        }
        let principal = &root.context.principal;
        let mut runs = Vec::new();
        let mut checks = std::collections::BTreeMap::new();
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
                let (record, _) = self.reply_in(trx, run, custody).await?;
                if record.request.context.agent != principal.agent
                    || record.request.context.owner != principal.owner
                    || record.intent.space() != batch.space
                    || record.intent.trigger() != candidate.trigger
                {
                    return Err(RuntimeError::InvalidBinding);
                }
                if mode != DiscoveryMode::Untracked {
                    self.capture_reply_rows(trx, &record, custody, &mut checks)
                        .await?;
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
                || record.original != *root
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
                .native_references(root)
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
            trx.set(&reference_key, root.manifest_digest.as_bytes());
            trx.set(&key, candidate.locator.run.as_bytes());
            if mode != DiscoveryMode::Untracked {
                self.capture_reply_rows(trx, &record, custody, &mut checks)
                    .await?;
            }
            runs.push(candidate.locator.clone());
        }
        Ok(ReplyEffects {
            runs,
            checks: checks.into_iter().collect(),
        })
    }
    async fn capture_reply_rows(
        &self,
        trx: &Transaction,
        record: &RunRecord,
        custody: &Custody,
        checks: &mut std::collections::BTreeMap<Vec<u8>, String>,
    ) -> Result<(), RuntimeError> {
        self.reply_binding_in(trx, record, custody).await?;
        let sealed = trx
            .get(&self.run(&record.locator.run, "record/1"), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let wake_bytes = trx
            .get(&self.reply_wake(&record.locator.wake), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let wake: Wake = custody.open(
            &format!("native-wake/1/{}", record.locator.wake),
            None,
            &wake_bytes,
        )?;
        if wake.locator != record.locator || wake.record_digest != hash(&sealed) {
            return Err(RuntimeError::InvalidBinding);
        }
        self.reply_original_scope(trx, record, &hash(&sealed))
            .await?;
        for key in [
            self.run(&record.locator.run, "record/1"),
            self.attempt(&record.locator.attempt, "prepared"),
            self.reply_wake(&record.locator.wake),
            self.native_references(&record.original)
                .pack(&(record.locator.run.as_str(),)),
            self.reply_trigger(
                &record.request.context.agent,
                record.intent.space(),
                record.intent.trigger(),
            ),
        ] {
            let bytes = trx
                .get(&key, false)
                .await?
                .ok_or(RuntimeError::RestoreUnreconciled)?;
            let digest = hash(&bytes);
            if checks
                .insert(key, digest.clone())
                .is_some_and(|prior| prior != digest)
            {
                return Err(RuntimeError::InvalidBinding);
            }
        }
        Ok(())
    }
}

use super::*;

impl Execution {
    pub(in crate::native) async fn collect_native_stage(
        &self,
        principal: &Principal,
    ) -> Result<(), RuntimeError> {
        let trx = self.transaction().await?;
        let key = self.native_key(principal, "stage");
        let Some(value) = trx.get(&key, false).await? else {
            return Ok(());
        };
        let stage: Stage = decode(&value)?;
        // An interrupted Images activation retains the exact original capsule
        // until recovery has reauthorized and activated (or explicitly refused).
        if trx
            .get(&self.image_activation_pin(principal), false)
            .await?
            .is_some()
        {
            return Ok(());
        }
        stage
            .root
            .context
            .validate(principal, stage.root.context.key_version)?;
        let version = trx.get_read_version().await?;
        let collect_after = stage
            .expires
            .checked_add(ORPHAN_RETENTION_VERSIONS)
            .ok_or(RuntimeError::InvalidBinding)?;
        if version < collect_after {
            return Ok(());
        }
        if self
            .native_root(&trx, principal)
            .await?
            .is_some_and(|r| r.context.capsule == stage.root.context.capsule)
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let (begin, end) = self.native_generation(&stage.root).range();
        trx.clear_range(&begin, &end);
        trx.clear(&key);
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }

    pub(in crate::native) async fn collect_native_retired(
        &self,
        principal: &Principal,
    ) -> Result<(), RuntimeError> {
        let trx = self.transaction().await?;
        let version = trx.get_read_version().await?;
        let active = self.native_root(&trx, principal).await?;
        let stage: Option<Stage> = trx
            .get(&self.native_key(principal, "stage"), false)
            .await?
            .map(|value| decode(&value))
            .transpose()?;
        let retired_rows = bounded_rows(
            &trx,
            foundationdb::RangeOption::from(self.native_retired(principal).range()),
            MAX_RETIRED,
            8192,
        )
        .await?;
        let mut changed = false;
        for (key, bytes) in retired_rows {
            let retired: Retired = decode(&bytes)?;
            retired
                .root
                .context
                .validate(principal, retired.root.context.key_version)?;
            let (capsule,): (String,) = self
                .native_retired(principal)
                .unpack(&key)
                .map_err(|_| RuntimeError::InvalidBinding)?;
            if capsule != retired.root.context.capsule {
                return Err(RuntimeError::InvalidBinding);
            }
            if retired.collect_after > version
                || active
                    .as_ref()
                    .is_some_and(|r| r.context.capsule == capsule)
                || stage.as_ref().is_some_and(|s| {
                    s.root.context.capsule == capsule
                        || s.previous
                            .as_ref()
                            .is_some_and(|r| r.context.capsule == capsule)
                })
            {
                continue;
            }
            let mut references =
                foundationdb::RangeOption::from(self.native_references(&retired.root).range());
            references.limit = Some(1);
            if !trx.get_range(&references, 1, false).await?.is_empty() {
                continue;
            }
            let (begin, end) = self.native_generation(&retired.root).range();
            trx.clear_range(&begin, &end);
            trx.clear(&key);
            changed = true;
        }
        if changed {
            // Root, stage and reference range reads conflict with any concurrent
            // activation or retained-reference insertion. Pinned evidence stays.
            trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        }
        Ok(())
    }
}

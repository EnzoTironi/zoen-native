use super::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};

fn intent_subject(deployment: &RuntimeDeployment) -> String {
    format!(
        "runtime-policy-continuity/1/{}/{}",
        deployment.namespace, deployment.witness
    )
}

impl ManagedRuntime {
    pub(super) fn seal_policy(
        &self,
        runtime: &RuntimeAuthority,
        signed: &SignedOwnerPolicy,
    ) -> Result<String, RuntimeError> {
        let (_, bytes) = runtime
            .custody
            .seal(&intent_subject(&self.deployment), signed)?;
        let encoded = STANDARD.encode(bytes);
        if encoded.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        Ok(encoded)
    }

    pub(super) fn retained_policy(
        &self,
        runtime: &RuntimeAuthority,
        prepared: &Prepared,
    ) -> Result<SignedOwnerPolicy, RuntimeError> {
        let encoded = prepared
            .intent
            .as_ref()
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if encoded.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let signed: SignedOwnerPolicy =
            runtime
                .custody
                .open(&intent_subject(&self.deployment), None, &bytes)?;
        let digest = policy_digest(&signed)?;
        let [update] = prepared.subject_updates.as_slice() else {
            return Err(RuntimeError::RestoreUnreconciled);
        };
        if prepared.retirement
            || effect(&signed)? != prepared.effect_digest
            || update.subject != subject(&signed)?
            || update.next != digest
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(signed)
    }
}

impl RuntimeAuthority {
    pub(crate) async fn install_continuous_owner_policy(
        &self,
        signed: SignedOwnerPolicy,
        guard: NativeDirectoryGuard,
    ) -> Result<crate::OwnerPolicyInstallation, RuntimeError> {
        let digest = policy_digest(&signed)?;
        match self.install_continuous_policy(signed, Some(guard)).await {
            Ok(actual) => Ok(crate::OwnerPolicyInstallation::Installed(actual)),
            Err(
                RuntimeError::InvalidPolicy
                | RuntimeError::InvalidBinding
                | RuntimeError::Denied
                | RuntimeError::InvalidProfile,
            ) => Ok(crate::OwnerPolicyInstallation::Refused(digest)),
            Err(error) => Err(error),
        }
    }

    pub(crate) async fn install_continuous_policy(
        &self,
        signed: SignedOwnerPolicy,
        native: Option<NativeDirectoryGuard>,
    ) -> Result<String, RuntimeError> {
        let digest = policy_digest(&signed)?;
        let native_observation = native.is_some();
        let managed = self
            .runtime_continuity
            .as_ref()
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let mut managed = managed.lock().await;
        managed.reconcile(self).await?;
        let mut tx = match native {
            Some(guard) => guard.policy_transaction(&signed.policy.owner)?,
            None => self.finance.continuity_transaction().await?,
        };
        if Finance::policy_scope(&mut tx).await? != Some(3) {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let sql = managed.sql_checkpoint(&mut tx).await?;
        let fdb = self.execution.transaction().await?;
        if managed.fdb_checkpoint(self, &fdb).await? != sql
            || managed.anchor.reader().checkpoint().map_err(error)? != sql
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let before = inventory(self, &mut tx, &fdb).await?;
        if before
            != managed
                .anchor
                .reader()
                .subject_commitments()
                .map_err(error)?
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        if native_observation {
            // Native historical acknowledgements are observations, including an
            // expired original. Current directory locks are still held here.
            if let Some(exact) =
                Finance::observe_owner_policy(&mut tx, &signed.policy.owner, &signed, &digest)
                    .await?
            {
                if !exact {
                    return Err(RuntimeError::InvalidBinding);
                }
                tx.commit().await?;
                return Ok(digest);
            }
        }
        let actual = Finance::install_continuous_policy_in(&mut tx, signed.clone()).await?;
        let after = inventory(self, &mut tx, &fdb).await?;
        if after == before {
            tx.commit().await?;
            return Ok(actual);
        }
        let id = subject(&signed)?;
        let update = SubjectUpdate {
            subject: id.clone(),
            previous: before.get(&id).cloned(),
            next: digest.clone(),
        };
        let sealed = managed.seal_policy(self, &signed)?;
        let _generation = managed
            .anchor
            .reader()
            .generation_guard(&sql)
            .map_err(error)?;
        let prepared = managed
            .anchor
            .prepare_subjects(
                &effect(&signed)?,
                &sql,
                &sql,
                false,
                &[update],
                Some(&sealed),
            )
            .map_err(error)?;
        if managed
            .anchor
            .reader()
            .pending_subject_commitments(&prepared)
            .map_err(error)?
            != after
        {
            return Err(RuntimeError::InvalidBinding);
        }
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 80 {
            return Err(RuntimeError::Unavailable);
        }
        managed.write_fdb(self, &fdb, &prepared).await?;
        fdb.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 81 {
            return Err(RuntimeError::Unavailable);
        }
        managed.write_sql(&mut tx, &prepared).await?;
        tx.commit().await?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst) == 82 {
            return Err(RuntimeError::Unavailable);
        }
        managed.reconcile(self).await?;
        Ok(actual)
    }
}

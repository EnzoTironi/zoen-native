//! Shared independent continuity, retaining the existing SQL financial
//! authority. No constructor here opens paid dispatch, output or refund gates.
#![deny(clippy::all, clippy::perf, clippy::unwrap_used, clippy::expect_used)]
mod policy;
mod recovery;
mod setup;
mod stores;
use crate::{
    finance::{Finance, NativeDirectoryGuard},
    RuntimeAuthority, RuntimeError, SignedOwnerPolicy,
};
use relay_admission::managed_images::{
    AnchorWriter, Checkpoint, Prepared, Recovery, RuntimeDeployment, SubjectUpdate,
};
use sqlx::{types::Json, Postgres, Transaction};
use std::collections::BTreeMap;

pub(crate) struct ManagedRuntime {
    pub(crate) anchor: AnchorWriter,
    pub(crate) deployment: RuntimeDeployment,
}

#[cfg(test)]
impl ManagedRuntime {
    pub(crate) fn test_deployment(&self) -> RuntimeDeployment {
        self.deployment.clone()
    }
    pub(crate) fn test_original_policy(
        &self,
        runtime: &RuntimeAuthority,
    ) -> Result<(Prepared, SignedOwnerPolicy), RuntimeError> {
        let pending = self
            .anchor
            .pending()
            .map_err(error)?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let signed = self.retained_policy(runtime, &pending)?;
        Ok((pending, signed))
    }
}

pub(super) fn subject(signed: &SignedOwnerPolicy) -> Result<String, RuntimeError> {
    let policy = &signed.policy;
    let start = crate::finance::period(policy.year, policy.month)?.0;
    Ok(crate::hash(
        &serde_json::to_vec(&("runtime-policy/1", &policy.owner, start))
            .map_err(|_| RuntimeError::InvalidBinding)?,
    ))
}

fn policy_digest(signed: &SignedOwnerPolicy) -> Result<String, RuntimeError> {
    if !event_log::owner_budget::verify(signed) {
        return Err(RuntimeError::InvalidPolicy);
    }
    event_log::owner_budget::digest(&signed.policy).ok_or(RuntimeError::InvalidPolicy)
}

fn effect(signed: &SignedOwnerPolicy) -> Result<String, RuntimeError> {
    Ok(crate::hash(
        &serde_json::to_vec(&("runtime-policy-install/1", signed))
            .map_err(|_| RuntimeError::InvalidBinding)?,
    ))
}

fn key(execution: &crate::execution::Execution) -> Vec<u8> {
    execution.root.pack(&("managed-images/checkpoint/1",))
}

pub(crate) fn history(execution: &crate::execution::Execution, sequence: u64) -> Vec<u8> {
    execution.root.pack(&("managed-images/effect/1", sequence))
}

fn error(error: relay_admission::managed_images::AnchorError) -> RuntimeError {
    crate::managed_images::error(error)
}

async fn policy_inventory(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let rows: Vec<(String,i64,i64,String,Json<SignedOwnerPolicy>)> = sqlx::query_as("SELECT DISTINCT ON(owner,period_start) owner,period_start,version,digest,signed FROM runtime_policies ORDER BY owner,period_start,version DESC LIMIT 513")
        .fetch_all(&mut **tx).await?;
    if rows.len() > 512 {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    let mut subjects = BTreeMap::new();
    for (owner, start, version, digest, Json(signed)) in rows {
        if signed.policy.owner != owner
            || crate::finance::period(signed.policy.year, signed.policy.month)?.0 != start
            || i64::from(signed.policy.version) != version
            || policy_digest(&signed)? != digest
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        if subjects.insert(subject(&signed)?, digest).is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
    }
    Ok(subjects)
}

pub(crate) async fn inventory(
    runtime: &RuntimeAuthority,
    sql: &mut Transaction<'_, Postgres>,
    fdb: &foundationdb::Transaction,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let mut result = policy_inventory(sql).await?;
    for (subject, value) in crate::native::shared_commitments(runtime, fdb).await? {
        if result.insert(subject, value).is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
    }
    if result.len() > 512 {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    Ok(result)
}

//! Shared independent continuity, retaining the existing SQL financial
//! authority. No constructor here opens paid dispatch, output or refund gates.
#![deny(clippy::all, clippy::perf, clippy::unwrap_used, clippy::expect_used)]
mod ack;
mod association;
mod capture;
pub(crate) mod dispatch;
mod policy;
mod recovery;
mod reservation;
mod scope;
mod setup;
pub(crate) use scope::SharedOutputAnchor;
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

/// One writer plus a read-only view of that same anchor. Inventory checks may
/// inspect immutable/pending history while already holding the writer mutex.
pub(crate) struct RuntimeContinuity {
    managed: tokio::sync::Mutex<ManagedRuntime>,
    reader: relay_admission::managed_images::AnchorReader,
}
impl RuntimeContinuity {
    pub(super) fn new(managed: ManagedRuntime) -> Self {
        let reader = managed.anchor.reader();
        Self {
            managed: tokio::sync::Mutex::new(managed),
            reader,
        }
    }
    pub(crate) async fn lock(&self) -> tokio::sync::MutexGuard<'_, ManagedRuntime> {
        self.managed.lock().await
    }
    pub(crate) fn reader(&self) -> &relay_admission::managed_images::AnchorReader {
        &self.reader
    }
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
    for (subject, value) in Finance::financial_inventory(sql).await? {
        if result.insert(subject, value).is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
    }
    for (subject, value) in crate::native::shared_commitments(runtime, fdb).await? {
        if result.insert(subject, value).is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
    }
    let (dispatch, admitted) = dispatch::commitments(runtime, fdb).await?;
    for (subject, value) in dispatch {
        if result.insert(subject, value).is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
    }
    let captures = capture::commitments(runtime, fdb).await?;
    for (subject, value) in captures.commitments {
        if result.insert(subject, value).is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
    }
    // ACKs refine the existing capture commitment, without consuming another
    // subject or invalidating the byte-exact pre-ACK inventory on upgrades.
    for (subject, value) in ack::commitments(runtime, sql, fdb).await? {
        let original = result
            .get_mut(&subject)
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        *original = value;
    }
    // Output correspondence refines the same subject after financial receipts.
    // Missing correspondence preserves the exact historical commitment bytes.
    for (subject, association) in captures.associations {
        let value = result
            .get_mut(&subject)
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        *value = crate::hash(
            &serde_json::to_vec(&("runtime-capture-association/1", &*value, association))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        );
    }
    for (subject, publication) in captures.publications {
        let value = result
            .get_mut(&subject)
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        *value = crate::native::publication_commitment(value, &publication)?;
    }
    // A live provider must have room to retain its largest allowed result.
    // Unrelated transitions cannot consume another in-flight call's capacity.
    let pending = admitted.difference(&captures.attempts).count();
    let reserved_bytes = pending
        .checked_mul(crate::capture_codec::MAXIMUM)
        .and_then(|value| value.checked_add(captures.bytes))
        .ok_or(RuntimeError::EvidenceTooLarge)?;
    if result
        .len()
        .checked_add(pending)
        .is_none_or(|value| value > 512)
        || reserved_bytes > crate::capture_codec::INVENTORY_MAXIMUM
    {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    Ok(result)
}

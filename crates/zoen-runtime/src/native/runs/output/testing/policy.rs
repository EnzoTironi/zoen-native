//! Durable policy metadata observed from the actual encrypted publication.
use super::*;

pub(crate) async fn policy_audit(
    runtime: &RuntimeAuthority,
    run: &str,
) -> Result<Option<gate::AuditRecord>, RuntimeError> {
    let tx = runtime.execution.transaction().await?;
    let (record, original) = runtime
        .execution
        .reply_in(&tx, run, &runtime.custody)
        .await?;
    let capture = runtime
        .execution
        .output_capture_in(&tx, &record, &original, &runtime.custody)
        .await?;
    let publication = runtime
        .execution
        .publication_in(
            &tx,
            &record,
            &original,
            &capture.association.digest,
            &runtime.custody,
        )
        .await?
        .ok_or(RuntimeError::Denied)?;
    Ok(publication.value.gate)
}

pub(crate) async fn policy_leases_released(
    runtime: &RuntimeAuthority,
    run: &str,
) -> Result<bool, RuntimeError> {
    let tx = runtime.execution.transaction().await?;
    let (record, _) = runtime
        .execution
        .reply_in(&tx, run, &runtime.custody)
        .await?;
    let version = tx.get_read_version().await?;
    for key in [
        runtime.execution.run(run, "lease"),
        runtime
            .execution
            .native_lease(&record.original.context.principal),
    ] {
        let value = tx.get(&key, false).await?.ok_or(RuntimeError::Denied)?;
        let lease: crate::execution::Lease =
            serde_json::from_slice(&value).map_err(|_| RuntimeError::InvalidBinding)?;
        if lease.expires > version {
            return Ok(false);
        }
    }
    Ok(true)
}

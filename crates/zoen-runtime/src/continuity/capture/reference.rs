//! Verify the retained result already committed under the shared checkpoint.
use super::*;
pub(in crate::continuity) async fn reference(
    runtime: &RuntimeAuthority,
    tx: &foundationdb::Transaction,
    attempt: &str,
    digest: &str,
) -> Result<(crate::Binding, String, String), RuntimeError> {
    let key = runtime
        .execution
        .root
        .pack(&("shared-native-capture/1", attempt, digest));
    let bytes = tx
        .get(&key, false)
        .await?
        .ok_or(RuntimeError::RestoreUnreconciled)?;
    if bytes.len() > 8192 {
        return Err(RuntimeError::InvalidBinding);
    }
    let pointer: Pointer =
        serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
    if pointer.key(runtime) != key || pointer.bytes()?.as_slice() != bytes.as_ref() {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    let root = tx
        .get(&pointer.evidence_key(runtime), false)
        .await?
        .ok_or(RuntimeError::RestoreUnreconciled)?;
    let (sealed, binding, nonce) = pointer.verify(runtime, tx, &root).await?;
    Ok((binding, crate::hash(&sealed), nonce))
}

/// Derived pending correspondence, not another queue or execution grant.
pub(in crate::continuity) async fn pending_associations(
    runtime: &RuntimeAuthority,
    limit: u16,
) -> Result<Vec<(String, String)>, RuntimeError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let tx = runtime.execution.transaction().await?;
    let rows = crate::native::bounded_rows(
        &tx,
        foundationdb::RangeOption::from(
            runtime
                .execution
                .root
                .subspace(&("shared-native-capture/1",))
                .range(),
        ),
        513,
        8192,
    )
    .await?;
    if rows.len() > 512 {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    let mut result = Vec::new();
    for (key, bytes) in rows {
        let pointer: Pointer =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
        if pointer.key(runtime) != key || pointer.bytes()? != bytes {
            return Err(RuntimeError::InvalidBinding);
        }
        if tx
            .get(
                &runtime
                    .execution
                    .attempt(&pointer.attempt, "native-capture/1"),
                false,
            )
            .await?
            .is_none()
        {
            result.push((pointer.attempt, pointer.digest));
            if result.len() == usize::from(limit) {
                break;
            }
        }
    }
    Ok(result)
}

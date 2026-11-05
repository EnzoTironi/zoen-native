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

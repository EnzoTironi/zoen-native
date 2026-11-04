use super::*;
pub(in crate::continuity) async fn commitments(
    runtime: &RuntimeAuthority,
    tx: &foundationdb::Transaction,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let prefix = runtime
        .execution
        .root
        .subspace(&("shared-native-dispatch/1",));
    let rows = crate::native::bounded_rows(
        tx,
        foundationdb::RangeOption::from(prefix.range()),
        513,
        8192,
    )
    .await?;
    if rows.len() > 512 {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    let mut pointers = BTreeMap::new();
    for (key, bytes) in rows {
        let pointer: Pointer =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
        if pointer.key(runtime) != key || pointer.bytes()? != bytes {
            return Err(RuntimeError::InvalidBinding);
        }
        pointer.observe(runtime, tx).await?;
        if pointers
            .insert(pointer.subject(), crate::hash(&bytes))
            .is_some()
        {
            return Err(RuntimeError::InvalidBinding);
        }
    }
    Ok(pointers)
}

use super::*;
pub(in crate::continuity) struct CaptureInventory {
    pub(in crate::continuity) commitments: BTreeMap<String, String>,
    pub(in crate::continuity) publications: BTreeMap<String, String>,
    pub(in crate::continuity) receipts: BTreeMap<String, String>,
    pub(in crate::continuity) associations: BTreeMap<String, String>,
    pub(in crate::continuity) attempts: std::collections::BTreeSet<String>,
    pub(in crate::continuity) bytes: usize,
}
pub(in crate::continuity) async fn commitments(
    runtime: &RuntimeAuthority,
    tx: &foundationdb::Transaction,
) -> Result<CaptureInventory, RuntimeError> {
    let prefix = runtime
        .execution
        .root
        .subspace(&("shared-native-capture/1",));
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
    let mut result = BTreeMap::new();
    let mut total = 0usize;
    let mut associations = BTreeMap::new();
    let mut publications = BTreeMap::new();
    let mut receipts = BTreeMap::new();
    let mut attempts = std::collections::BTreeSet::new();
    for (key, bytes) in rows {
        let pointer: Pointer =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidBinding)?;
        if pointer.key(runtime) != key || pointer.bytes()? != bytes {
            return Err(RuntimeError::InvalidBinding);
        }
        let root = tx
            .get(&pointer.evidence_key(runtime), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        total = total
            .checked_add(crate::execution::Execution::evidence_payload_size(&root)?)
            .ok_or(RuntimeError::EvidenceTooLarge)?;
        if total > crate::capture_codec::INVENTORY_MAXIMUM {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let (_, binding, nonce) = pointer.verify(runtime, tx, &root).await?;
        if let Some(bytes) = runtime
            .execution
            .retained_capture_association_in(
                tx,
                &binding,
                (&pointer.digest, &nonce),
                &runtime.custody,
            )
            .await?
        {
            associations.insert(pointer.subject()?, crate::hash(&bytes));
        }
        if let Some((publication, receipt)) = runtime
            .execution
            .shared_output_digests_in(runtime, tx, &binding)
            .await?
        {
            publications.insert(pointer.subject()?, publication);
            if let Some(receipt) = receipt {
                receipts.insert(pointer.subject()?, receipt);
            }
        }
        attempts.insert(pointer.attempt.clone());
        if result
            .insert(pointer.subject()?, crate::hash(&bytes))
            .is_some()
        {
            return Err(RuntimeError::InvalidBinding);
        }
    }
    Ok(CaptureInventory {
        commitments: result,
        associations,
        publications,
        receipts,
        attempts,
        bytes: total,
    })
}

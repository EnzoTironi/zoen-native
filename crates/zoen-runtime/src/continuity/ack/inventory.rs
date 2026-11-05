//! Refine each existing capture's commitment with its existing FDB receipts.
//! With no receipts, the old pointer commitment remains byte-exact.
use super::Record;
use crate::{RuntimeAuthority, RuntimeError};
use sqlx::{Postgres, Transaction};
use std::collections::BTreeMap;
const MAX_REFERENCES: usize = 2 * 512; // evidence + settle per bounded capture
pub(in crate::continuity) async fn commitments(
    runtime: &RuntimeAuthority,
    sql: &mut Transaction<'_, Postgres>,
    tx: &foundationdb::Transaction,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let acknowledged: Vec<(String,String,String)> = sqlx::query_as(
        "SELECT attempt,kind,digest FROM runtime_financial_acks ORDER BY attempt,kind,digest LIMIT 1025",
    ).fetch_all(&mut **sql).await?;
    if acknowledged.len() > MAX_REFERENCES {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    let mut known = BTreeMap::new();
    for (attempt, kind, digest) in acknowledged {
        let record = Record {
            attempt,
            kind,
            digest,
        };
        record.validate(runtime)?;
        known.insert(record.key(runtime), record);
    }
    let prefix = runtime.execution.root.subspace(&("financial",));
    let rows = crate::native::bounded_rows(
        tx,
        foundationdb::RangeOption::from(prefix.range()),
        MAX_REFERENCES + 1,
        1,
    )
    .await?;
    if rows.len() > MAX_REFERENCES {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for (key, bytes) in rows {
        if bytes != b"1" {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let record = known
            .remove(&key)
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        groups
            .entry((record.attempt, record.digest))
            .or_default()
            .push(record.kind);
    }
    let mut result = BTreeMap::new();
    for ((attempt, digest), mut kinds) in groups {
        kinds.sort();
        let key = runtime.execution.root.pack(&(
            "shared-native-capture/1",
            attempt.as_str(),
            digest.as_str(),
        ));
        let pointer = tx
            .get(&key, false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if pointer.len() > 8192 {
            return Err(RuntimeError::InvalidBinding);
        }
        let subject = Record {
            attempt,
            kind: "evidence".into(),
            digest,
        }
        .subject()?;
        let value = crate::hash(
            &serde_json::to_vec(&("runtime-capture-ack/1", pointer.as_ref(), kinds))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        );
        if result.insert(subject, value).is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
    }
    // SQL-next/FDB-before temporarily keeps the previous capture commitment.
    // The original PREPARED operation alone can finish that declared cut.
    Ok(result)
}

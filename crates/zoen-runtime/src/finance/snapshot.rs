//! Hash actual financial authority, including mutable aggregates and immutable
//! history. These commitments are witnesses, never another balance/ledger.
use super::*;
use std::collections::BTreeMap;
const MAX_ROWS: usize = 4096;
const MAX_BYTES: usize = 8 * 1024 * 1024;
type PeriodRow = (String, i64, i64, String, i16, i64, i64);

pub(crate) fn subject(owner: &str, period: i64) -> Result<String, RuntimeError> {
    Ok(crate::hash(
        &serde_json::to_vec(&("runtime-financial-period/1", owner, period))
            .map_err(|_| RuntimeError::InvalidBinding)?,
    ))
}
impl Finance {
    pub(crate) async fn financial_inventory(
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<BTreeMap<String, String>, RuntimeError> {
        // An empty policy-created period has no financial subject yet. Any
        // obligation, closure or nonzero aggregate must participate thereafter.
        let periods: Vec<PeriodRow> = sqlx::query_as("SELECT p.owner,p.period_start,p.period_end,p.currency,p.scale,p.held_units,p.spent_units FROM runtime_periods p WHERE p.held_units<>0 OR p.spent_units<>0 OR EXISTS(SELECT 1 FROM runtime_attempts a WHERE a.owner=p.owner AND a.period_start=p.period_start) OR EXISTS(SELECT 1 FROM runtime_predispatch_closures c WHERE c.owner=p.owner AND c.period_start=p.period_start) ORDER BY p.owner,p.period_start LIMIT 513")
            .fetch_all(&mut **tx).await?;
        if periods.len() > 512 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let mut result = BTreeMap::new();
        let mut total = 0usize;
        for period in periods {
            let mut parts = Vec::new();
            // Sources/columns/order are compile-time SQL, not caller input.
            for (name, source) in SOURCES {
                let query = format!(
                    "SELECT row_to_json(r)::text FROM ({source} LIMIT {}) r",
                    MAX_ROWS + 1
                );
                let rows: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(query))
                    .bind(&period.0)
                    .bind(period.1)
                    .fetch_all(&mut **tx)
                    .await?;
                if rows.len() > MAX_ROWS {
                    return Err(RuntimeError::EvidenceTooLarge);
                }
                for row in &rows {
                    total = total
                        .checked_add(row.len())
                        .ok_or(RuntimeError::EvidenceTooLarge)?;
                    if total > MAX_BYTES {
                        return Err(RuntimeError::EvidenceTooLarge);
                    }
                }
                parts.push((name, rows));
            }
            let digest = crate::hash(
                &serde_json::to_vec(&("runtime-financial-state/1", &period, parts))
                    .map_err(|_| RuntimeError::InvalidBinding)?,
            );
            if result
                .insert(subject(&period.0, period.1)?, digest)
                .is_some()
            {
                return Err(RuntimeError::InvalidBinding);
            }
        }
        Ok(result)
    }
}
// Ciphertext is represented by its SHA-256 computed by PostgreSQL16's built-in
// binary hash, so large evidence cannot inflate the metadata read into memory.
const SOURCES: &[(&str, &str)] = &[
    ("attempts", "SELECT attempt,binding_digest,binding,hold_units,deployment FROM runtime_attempts WHERE owner=$1 AND period_start=$2 ORDER BY attempt"),
    ("claims", "SELECT c.attempt,c.nonce FROM runtime_claims c JOIN runtime_attempts a USING(attempt) WHERE a.owner=$1 AND a.period_start=$2 ORDER BY c.attempt"),
    ("admission", "SELECT c.attempt,c.nonce FROM runtime_admission_witnesses c JOIN runtime_attempts a USING(attempt) WHERE a.owner=$1 AND a.period_start=$2 ORDER BY c.attempt"),
    ("evidence", "SELECT c.attempt,c.digest,encode(sha256(c.sealed),'hex') AS sealed_digest FROM runtime_evidence c JOIN runtime_attempts a USING(attempt) WHERE a.owner=$1 AND a.period_start=$2 ORDER BY c.attempt,c.digest"),
    ("dispositions", "SELECT c.attempt,c.kind,c.amount_units,c.source_digest FROM runtime_dispositions c JOIN runtime_attempts a USING(attempt) WHERE a.owner=$1 AND a.period_start=$2 ORDER BY c.attempt"),
    ("journals", "SELECT c.attempt,c.kind FROM runtime_journals c JOIN runtime_attempts a USING(attempt) WHERE a.owner=$1 AND a.period_start=$2 ORDER BY c.attempt,c.kind"),
    ("postings", "SELECT c.attempt,c.kind,c.account,c.units FROM runtime_postings c JOIN runtime_attempts a USING(attempt) WHERE a.owner=$1 AND a.period_start=$2 ORDER BY c.attempt,c.kind,c.account"),
    ("outbox", "SELECT c.attempt,c.kind,c.digest FROM runtime_financial_outbox c JOIN runtime_attempts a USING(attempt) WHERE a.owner=$1 AND a.period_start=$2 ORDER BY c.attempt,c.kind,c.digest"),
    ("acks", "SELECT c.attempt,c.kind,c.digest FROM runtime_financial_acks c JOIN runtime_attempts a USING(attempt) WHERE a.owner=$1 AND a.period_start=$2 ORDER BY c.attempt,c.kind,c.digest"),
    ("closures", "SELECT attempt,binding_digest FROM runtime_predispatch_closures WHERE owner=$1 AND period_start=$2 ORDER BY attempt"),
];

//! Durable replies to destructive MLS claims. A retry cannot consume another package.

use roda_proto::{
    key_package_claim_time_ms, KeyPackageRecord, Reply, ServerFrame, KEY_PACKAGE_CLAIM_EXPIRED,
    KEY_PACKAGE_CLAIM_TTL_MS,
};
use sqlx::PgPool;
use std::{fmt, sync::Arc, time::Duration};

pub const MAX_RECEIPTS: i64 = 4096;
pub const MAX_RECEIPT_BYTES: i64 = 8 * 1024 * 1024;
pub const GC_BATCH: i64 = 256;

#[derive(Debug)]
pub enum ClaimError {
    Refused(&'static str),
    Database(sqlx::Error),
}

impl fmt::Display for ClaimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(reason) => f.write_str(reason),
            Self::Database(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ClaimError {}

impl From<sqlx::Error> for ClaimError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

/// The same PostgreSQL clock used to enforce receipt freshness and expiry.
pub async fn clock_ms(pool: &PgPool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint")
        .fetch_one(pool)
        .await
}

pub async fn claim(
    pool: &PgPool,
    source_identity: &str,
    source_device: &str,
    operation: &str,
    targets: &[String],
) -> Result<Vec<KeyPackageRecord>, ClaimError> {
    let created = key_package_claim_time_ms(operation)
        .ok_or(ClaimError::Refused("invalid key-package claim operation"))?;
    if targets
        .iter()
        .any(|id| id.len() != 64 || !id.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')))
    {
        return Err(ClaimError::Refused(
            "claim targets must be canonical identity keys",
        ));
    }
    let mut targets = targets.to_vec();
    targets.sort_unstable();
    targets.dedup();
    if targets.is_empty() || targets.len() > 50 {
        return Err(ClaimError::Refused("claim one to fifty identities"));
    }
    let expires = created + KEY_PACKAGE_CLAIM_TTL_MS;
    let mut tx = pool.begin().await?;
    // Serialize quota accounting and same-id retries, across nodes and connections.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("kp-claim:{source_identity}:{source_device}"))
        .execute(&mut *tx)
        .await?;
    // Read the clock after the lock: waiting for another claim cannot extend the TTL.
    let now: i64 =
        sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint")
            .fetch_one(&mut *tx)
            .await?;
    if created > now {
        return Err(ClaimError::Refused(
            "key-package claim operation is in the future",
        ));
    }
    if expires <= now {
        return Err(ClaimError::Refused(KEY_PACKAGE_CLAIM_EXPIRED));
    }
    sqlx::query(
        "DELETE FROM key_package_claim_receipts
         WHERE source_identity = $1 AND source_device = $2 AND expires_at_ms <= $3",
    )
    .bind(source_identity)
    .bind(source_device)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    let previous: Option<(Vec<String>, Vec<u8>)> = sqlx::query_as(
        "SELECT targets, response FROM key_package_claim_receipts
         WHERE source_identity = $1 AND source_device = $2 AND operation_id = $3",
    )
    .bind(source_identity)
    .bind(source_device)
    .bind(operation)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((original, response)) = previous {
        if original != targets {
            return Err(ClaimError::Refused(
                "key-package claim operation reused with different targets",
            ));
        }
        let records = match ServerFrame::decode(&response) {
            Ok(ServerFrame::Res {
                id: 0,
                result: Ok(Reply::KeyPackages(records)),
            }) => records,
            _ => {
                return Err(
                    sqlx::Error::Protocol("corrupt key-package claim receipt".into()).into(),
                )
            }
        };
        tx.commit().await?;
        return Ok(records);
    }
    let (count, bytes): (i64, i64) = sqlx::query_as(
        "SELECT count(*), COALESCE(sum(octet_length(response)), 0)::bigint
         FROM key_package_claim_receipts WHERE source_identity = $1 AND source_device = $2",
    )
    .bind(source_identity)
    .bind(source_device)
    .fetch_one(&mut *tx)
    .await?;
    if count >= MAX_RECEIPTS || bytes >= MAX_RECEIPT_BYTES {
        return Err(ClaimError::Refused(
            "key-package claim receipt capacity reached",
        ));
    }
    let records = crate::db::claim_key_packages_on(&mut tx, &targets).await?;
    let response = ServerFrame::Res {
        id: 0,
        result: Ok(Reply::KeyPackages(records.clone())),
    }
    .encode();
    if bytes + response.len() as i64 > MAX_RECEIPT_BYTES {
        // Dropping this transaction restores every package just taken.
        return Err(ClaimError::Refused(
            "key-package claim receipt capacity reached",
        ));
    }
    sqlx::query(
        "INSERT INTO key_package_claim_receipts
         (source_identity, source_device, operation_id, targets, response, expires_at_ms)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(source_identity)
    .bind(source_device)
    .bind(operation)
    .bind(&targets)
    .bind(response)
    .bind(expires)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(records)
}

pub async fn prune(pool: &PgPool) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "DELETE FROM key_package_claim_receipts WHERE ctid IN (
             SELECT ctid FROM key_package_claim_receipts
             WHERE expires_at_ms <= floor(extract(epoch FROM clock_timestamp()) * 1000)
             ORDER BY expires_at_ms LIMIT $1 FOR UPDATE SKIP LOCKED
         )",
    )
    .bind(GC_BATCH)
    .execute(pool)
    .await?
    .rows_affected())
}

pub(crate) fn spawn(state: &Arc<crate::AppState>) {
    let state = Arc::downgrade(state);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let Some(state) = state.upgrade() else { break };
            if let Err(error) = prune(&state.pool).await {
                tracing::warn!(%error, "key-package claim receipt cleanup failed");
            }
        }
    });
}

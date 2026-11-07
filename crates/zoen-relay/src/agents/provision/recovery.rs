//! Device-authenticated recovery of the owner's retained onboarding originals.
use super::{owner_device, record, Error, Row, LOCKED, SELECT};
use protocol::managed_agents::{digest, Page, Record, Request};
use sqlx::PgPool;

pub async fn discover(
    pool: &PgPool,
    owner: &str,
    device: &str,
    after: &str,
) -> Result<Page, Error> {
    if !after.is_empty() && !digest(after) {
        return Err(Error::Changed);
    }
    let mut tx = pool.begin().await?;
    owner_device(&mut tx, owner, device).await?;
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT owner,operation,service,template_sha256,name,handle,recipient_public_key,
        state,agent,offer_digest,sealed_offer,expires_ms,onboarding_completed FROM managed_agent_requests
        WHERE owner=$1 AND operation>$2 AND NOT onboarding_completed
        AND state IN ('queued','offered','cancel_requested','registered')
        ORDER BY operation LIMIT 33",
    )
    .bind(owner)
    .bind(after)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let more = rows.len() > 32;
    let records: Vec<Record> = rows
        .into_iter()
        .take(32)
        .map(record)
        .collect::<Result<_, _>>()?;
    let next_operation = if more {
        records
            .last()
            .and_then(|r| r.request.as_ref())
            .ok_or(Error::Changed)?
            .operation
            .clone()
    } else {
        String::new()
    };
    Ok(Page {
        records,
        next_operation,
    })
}

pub async fn recover(
    pool: &PgPool,
    owner: &str,
    device: &str,
    operation: &str,
) -> Result<Record, Error> {
    if !digest(operation) {
        return Err(Error::Changed);
    }
    let mut tx = pool.begin().await?;
    owner_device(&mut tx, owner, device).await?;
    let row: Row = sqlx::query_as(
        "SELECT owner,operation,service,template_sha256,name,handle,recipient_public_key,
        state,agent,offer_digest,sealed_offer,expires_ms,onboarding_completed FROM managed_agent_requests
        WHERE owner=$1 AND operation=$2 AND NOT onboarding_completed",
    )
    .bind(owner)
    .bind(operation)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Error::Unavailable)?;
    tx.commit().await?;
    record(row)
}

/// Finishing onboarding never stops the agent or changes grants/budget/runtime.
pub async fn complete(
    pool: &PgPool,
    owner: &str,
    device: &str,
    request: &Request,
    agent: &str,
) -> Result<Record, Error> {
    if !request.valid() || !digest(agent) {
        return Err(Error::Changed);
    }
    let mut tx = pool.begin().await?;
    owner_device(&mut tx, owner, device).await?;
    let row: Row = sqlx::query_as(LOCKED)
        .bind(owner)
        .bind(&request.operation)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::Unavailable)?;
    let held = record(row)?;
    if held.state != "registered" || held.agent != agent || held.request.as_ref() != Some(request) {
        return Err(Error::Changed);
    }
    sqlx::query("UPDATE managed_agent_requests SET onboarding_completed=true WHERE owner=$1 AND operation=$2")
        .bind(owner).bind(&request.operation).execute(&mut *tx).await?;
    let row = sqlx::query_as(SELECT)
        .bind(owner)
        .bind(&request.operation)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    record(row)
}

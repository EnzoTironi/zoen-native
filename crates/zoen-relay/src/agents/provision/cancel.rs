//! An owner requests cancellation; only the exclusively locked host confirms it.
use super::{owner_device, record, Error, Row, LOCKED, SELECT};
use protocol::managed_agents::{Record, Request};
use sqlx::PgPool;

pub async fn cancel(
    pool: &PgPool,
    owner: &str,
    device: &str,
    request: &Request,
) -> Result<Record, Error> {
    if !request.valid() {
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
    if held.request.as_ref() != Some(request) {
        return Err(Error::Changed);
    }
    if matches!(held.state.as_str(), "queued" | "offered") {
        sqlx::query("UPDATE managed_agent_requests SET state='cancel_requested' WHERE owner=$1 AND operation=$2")
            .bind(owner).bind(&request.operation).execute(&mut *tx).await?;
    }
    let row = sqlx::query_as(SELECT)
        .bind(owner)
        .bind(&request.operation)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    record(row)
}

/// The service must first retire/reap its enrollment child and prove the unused
/// host offer was durably cancelled. Accepted consent instead finishes registration.
pub async fn confirmed(pool: &PgPool, owner: &str, request: &Request) -> Result<Record, Error> {
    if !request.valid() {
        return Err(Error::Changed);
    }
    let mut tx = pool.begin().await?;
    let row: Row = sqlx::query_as(LOCKED)
        .bind(owner)
        .bind(&request.operation)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::Unavailable)?;
    let held = record(row)?;
    if held.request.as_ref() != Some(request)
        || !matches!(held.state.as_str(), "cancel_requested" | "cancelled")
    {
        return Err(Error::Changed);
    }
    sqlx::query(
        "UPDATE managed_agent_requests SET state='cancelled' WHERE owner=$1 AND operation=$2",
    )
    .bind(owner)
    .bind(&request.operation)
    .execute(&mut *tx)
    .await?;
    let row = sqlx::query_as(SELECT)
        .bind(owner)
        .bind(&request.operation)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    record(row)
}

//! Trusted service-side queue transitions, with one retained enrollment original.
use super::{record, Error, Row, LOCKED, SELECT};
use protocol::managed_agents::{digest, Record, Request, Service};
use sqlx::PgPool;

pub struct Offer<'a> {
    pub agent: &'a str,
    pub digest: &'a str,
    pub sealed: &'a [u8],
    pub expires_ms: u64,
}

/// A bounded keyset page; restart from None after reaching the end to catch inserts.
pub async fn page(
    pool: &PgPool,
    service: &Service,
    after: Option<(&str, &str)>,
) -> Result<Vec<Record>, Error> {
    if !service.valid() {
        return Err(Error::Changed);
    }
    let (owner, operation) = after.unwrap_or(("", ""));
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT owner,operation,service,template_sha256,name,handle,recipient_public_key,
        state,agent,offer_digest,sealed_offer,expires_ms FROM managed_agent_requests
        WHERE service=$1 AND template_sha256=$2 AND (owner,operation)>($3,$4)
        ORDER BY owner,operation LIMIT 32",
    )
    .bind(&service.id)
    .bind(&service.template_sha256)
    .bind(owner)
    .bind(operation)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(record).collect()
}

pub async fn retain_offer(
    pool: &PgPool,
    owner: &str,
    request: &Request,
    offer: Offer<'_>,
) -> Result<Record, Error> {
    if !request.valid()
        || !digest(offer.agent)
        || !digest(offer.digest)
        || !(40..=16384).contains(&offer.sealed.len())
    {
        return Err(Error::Changed);
    }
    let expiry = i64::try_from(offer.expires_ms).map_err(|_| Error::Changed)?;
    let mut tx = pool.begin().await?;
    let row: Row = sqlx::query_as(LOCKED)
        .bind(owner)
        .bind(&request.operation)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::Unavailable)?;
    let retained = record(row)?;
    if retained.request.as_ref() != Some(request) {
        return Err(Error::Changed);
    }
    if retained.state != "queued" {
        if retained.agent != offer.agent
            || retained.offer_digest != offer.digest
            || retained.expires_ms != offer.expires_ms
        {
            return Err(Error::Changed);
        }
        // Preserve the original randomized ciphertext, including after lost ACK.
        tx.commit().await?;
        return Ok(retained);
    }
    let (now,): (i64,) =
        sqlx::query_as("SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint")
            .fetch_one(&mut *tx)
            .await?;
    if expiry <= now
        || expiry
            .checked_sub(now)
            .is_none_or(|ttl| ttl > 15 * 60 * 1000)
    {
        return Err(Error::Changed);
    }
    sqlx::query(
        "UPDATE managed_agent_requests SET state='offered',agent=$3,offer_digest=$4,
        sealed_offer=$5,expires_ms=$6 WHERE owner=$1 AND operation=$2",
    )
    .bind(owner)
    .bind(&request.operation)
    .bind(offer.agent)
    .bind(offer.digest)
    .bind(offer.sealed)
    .bind(expiry)
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

/// The canonical identity/citizen rows must confirm the exact offered agent.
pub async fn registered(
    pool: &PgPool,
    owner: &str,
    request: &Request,
    agent: &str,
    offer_digest: &str,
) -> Result<Record, Error> {
    if !request.valid() || !digest(agent) || !digest(offer_digest) {
        return Err(Error::Changed);
    }
    let mut tx = pool.begin().await?;
    let row: Row = sqlx::query_as(LOCKED)
        .bind(owner)
        .bind(&request.operation)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::Unavailable)?;
    let retained = record(row)?;
    if retained.request.as_ref() != Some(request)
        || retained.agent != agent
        || retained.offer_digest != offer_digest
        || !matches!(
            retained.state.as_str(),
            "offered" | "expired" | "registered" | "cancel_requested"
        )
    {
        return Err(Error::Changed);
    }
    sqlx::query(
        "UPDATE managed_agent_requests SET state='registered' WHERE owner=$1 AND operation=$2",
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

pub async fn expire(pool: &PgPool, owner: &str, request: &Request) -> Result<(), Error> {
    if !request.valid() {
        return Err(Error::Changed);
    }
    let service = request.service.as_ref().ok_or(Error::Changed)?;
    sqlx::query(
        "UPDATE managed_agent_requests SET state='expired' WHERE owner=$1 AND operation=$2
        AND service=$3 AND template_sha256=$4 AND state='offered'
        AND expires_ms<=(extract(epoch FROM clock_timestamp())*1000)::bigint",
    )
    .bind(owner)
    .bind(&request.operation)
    .bind(&service.id)
    .bind(&service.template_sha256)
    .execute(pool)
    .await?;
    Ok(())
}

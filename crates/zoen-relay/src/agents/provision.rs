//! Owner-authenticated bootstrap queue. Enrollment/citizens remain identity authority.
pub mod cancel;
pub mod host;
pub use cancel::cancel;
use protocol::managed_agents::{digest, Availability, Record, Request, Service};
use sqlx::{PgPool, Postgres, Transaction};

const MAX_OUTSTANDING: i64 = 16;
const SELECT: &str =
    "SELECT owner,operation,service,template_sha256,name,handle,recipient_public_key,
 state,agent,offer_digest,sealed_offer,expires_ms FROM managed_agent_requests
 WHERE owner=$1 AND operation=$2";
const LOCKED: &str =
    "SELECT owner,operation,service,template_sha256,name,handle,recipient_public_key,
 state,agent,offer_digest,sealed_offer,expires_ms FROM managed_agent_requests
 WHERE owner=$1 AND operation=$2 FOR UPDATE";
#[derive(sqlx::FromRow)]
struct Row {
    owner: String,
    operation: String,
    service: String,
    template_sha256: String,
    name: String,
    handle: String,
    recipient_public_key: String,
    state: String,
    agent: String,
    offer_digest: String,
    sealed_offer: Vec<u8>,
    expires_ms: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("managed agent service is unavailable")]
    Unavailable,
    #[error("managed agent request changed")]
    Changed,
    #[error("managed agent request capacity reached")]
    Capacity,
    #[error("managed agent service is unavailable")]
    Db(#[from] sqlx::Error),
}

fn record(row: Row) -> Result<Record, Error> {
    let value = Record {
        request: Some(Request {
            operation: row.operation,
            service: Some(Service {
                id: row.service,
                template_sha256: row.template_sha256,
            }),
            name: row.name,
            handle: row.handle,
            recipient_public_key: row.recipient_public_key,
        }),
        owner: row.owner,
        state: row.state,
        agent: row.agent,
        offer_digest: row.offer_digest,
        sealed_offer: row.sealed_offer,
        expires_ms: u64::try_from(row.expires_ms).map_err(|_| Error::Changed)?,
    };
    if !value.valid() {
        return Err(Error::Changed);
    }
    Ok(value)
}

async fn owner_device(
    tx: &mut Transaction<'_, Postgres>,
    owner: &str,
    device: &str,
) -> Result<(), Error> {
    // Serialize this owner's new requests without trusting identity in a payload.
    let person: Option<(String,)> = sqlx::query_as(
        "SELECT i.id FROM identities i
        JOIN devices d ON d.identity=i.id WHERE i.id=$1 AND i.kind='Person'
        AND d.device=$2 AND d.revoked_at IS NULL FOR UPDATE OF i FOR SHARE OF d",
    )
    .bind(owner)
    .bind(device)
    .fetch_optional(&mut **tx)
    .await?;
    person.ok_or(Error::Unavailable)?;
    Ok(())
}

pub async fn availability(pool: &PgPool) -> Result<Availability, Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT service,template_sha256
        FROM managed_agent_services WHERE heartbeat_at>=clock_timestamp()-interval '30 seconds'
        ORDER BY service LIMIT 2",
    )
    .fetch_all(pool)
    .await?;
    // Choosing a different pool silently could substitute another host/template.
    let service = match rows.as_slice() {
        [(id, template)] => Some(Service {
            id: id.clone(),
            template_sha256: template.clone(),
        }),
        [] => None,
        _ => return Err(Error::Unavailable),
    };
    Ok(Availability { service })
}

pub async fn prepare(
    pool: &PgPool,
    owner: &str,
    device: &str,
    request: &Request,
) -> Result<Record, Error> {
    if !request.valid() {
        return Err(Error::Changed);
    }
    let service = request.service.as_ref().ok_or(Error::Changed)?;
    let mut tx = pool.begin().await?;
    owner_device(&mut tx, owner, device).await?;
    if let Some(row) = sqlx::query_as::<_, Row>(SELECT)
        .bind(owner)
        .bind(&request.operation)
        .fetch_optional(&mut *tx)
        .await?
    {
        let retained = record(row)?;
        if retained.request.as_ref() != Some(request) {
            return Err(Error::Changed);
        }
        tx.commit().await?;
        return Ok(retained);
    }
    let live: Option<(String,)> = sqlx::query_as(
        "SELECT service FROM managed_agent_services
        WHERE service=$1 AND template_sha256=$2
        AND heartbeat_at>=clock_timestamp()-interval '30 seconds' FOR SHARE",
    )
    .bind(&service.id)
    .bind(&service.template_sha256)
    .fetch_optional(&mut *tx)
    .await?;
    live.ok_or(Error::Unavailable)?;
    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM managed_agent_requests
        WHERE owner=$1 AND state IN ('queued','offered','cancel_requested')",
    )
    .bind(owner)
    .fetch_one(&mut *tx)
    .await?;
    if count >= MAX_OUTSTANDING {
        return Err(Error::Capacity);
    }
    sqlx::query(
        "INSERT INTO managed_agent_requests
        (owner,operation,service,template_sha256,name,handle,recipient_public_key)
        VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(owner)
    .bind(&request.operation)
    .bind(&service.id)
    .bind(&service.template_sha256)
    .bind(&request.name)
    .bind(&request.handle)
    .bind(&request.recipient_public_key)
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

pub async fn status(
    pool: &PgPool,
    owner: &str,
    device: &str,
    service: &str,
    operation: &str,
) -> Result<Record, Error> {
    if !digest(service) || !digest(operation) {
        return Err(Error::Changed);
    }
    let mut tx = pool.begin().await?;
    owner_device(&mut tx, owner, device).await?;
    let row = sqlx::query_as(SELECT)
        .bind(owner)
        .bind(operation)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::Unavailable)?;
    let retained = record(row)?;
    if retained
        .request
        .as_ref()
        .and_then(|r| r.service.as_ref())
        .is_none_or(|s| s.id != service)
    {
        return Err(Error::Unavailable);
    }
    tx.commit().await?;
    Ok(retained)
}

/// Operator heartbeat is availability telemetry, never execution or spending consent.
pub async fn heartbeat(pool: &PgPool, service: &Service) -> Result<(), Error> {
    if !service.valid() {
        return Err(Error::Changed);
    }
    let result = sqlx::query(
        "INSERT INTO managed_agent_services(service,template_sha256)
        VALUES ($1,$2) ON CONFLICT(service) DO UPDATE SET heartbeat_at=clock_timestamp()
        WHERE managed_agent_services.template_sha256=EXCLUDED.template_sha256",
    )
    .bind(&service.id)
    .bind(&service.template_sha256)
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        return Err(Error::Changed);
    }
    Ok(())
}

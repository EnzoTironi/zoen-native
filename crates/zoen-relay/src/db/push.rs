//! Purpose-scoped notification registrations; directory ownership stays in db.
use super::*;

/// Refresh/clear only this purpose. Tokens migrate between devices within the same
/// purpose; ordinary APNs and VoIP registrations never replace one another.
pub async fn set_push_token(
    pool: &PgPool,
    identity: &str,
    device: &str,
    platform: &str,
    token: &str,
) -> Result<(), sqlx::Error> {
    let purpose = crate::push::purpose(platform)
        .ok_or_else(|| sqlx::Error::Protocol("invalid push registration purpose".into()))?;
    let apns = platform != crate::push::FCM;
    let canonical;
    let token = if apns && !token.is_empty() {
        canonical = protocol::push::ApnsToken::parse(token)
            .map_err(|_| sqlx::Error::Protocol("invalid APNs registration".into()))?;
        canonical.as_hex()
    } else {
        token
    };
    let mut tx = pool.begin().await?;
    // A token can move between devices, so a device row lock alone cannot
    // serialize concurrent claims for a token with no previous registration.
    if !token.is_empty() {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("push-token:{purpose}:{token}"))
            .execute(&mut *tx)
            .await?;
    }
    // Serialize authorization with revocation before changing token ownership.
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT true FROM devices WHERE identity = $1 AND device = $2
         AND revoked_at IS NULL FOR UPDATE",
    )
    .bind(identity)
    .bind(device)
    .fetch_optional(&mut *tx)
    .await?;
    if active.is_none() {
        return Err(sqlx::Error::Protocol("push device is not active".into()));
    }
    sqlx::query("DELETE FROM push_tokens WHERE purpose = $4 AND ((identity = $1 AND device = $2) OR ($3 <> '' AND (token = $3 OR ($5 AND platform <> 'fcm' AND lower(token) = $3))))")
        .bind(identity)
        .bind(device)
        .bind(token)
        .bind(purpose)
        .bind(apns)
        .execute(&mut *tx)
        .await?;
    if !token.is_empty() {
        sqlx::query(
            "INSERT INTO push_tokens (identity, device, platform, token, purpose) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(identity)
        .bind(device)
        .bind(platform)
        .bind(token)
        .bind(purpose)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// Push tokens of the still-linked devices of `ids`.
pub async fn push_targets(
    pool: &PgPool,
    ids: &[String],
) -> Result<Vec<crate::push::Target>, sqlx::Error> {
    let rows = sqlx::query_as(
        "SELECT p.platform, p.token, p.registration_id FROM push_tokens p
         JOIN devices d ON d.identity = p.identity AND d.device = p.device AND d.revoked_at IS NULL
         WHERE p.identity = ANY($1) AND p.purpose = 'notification'",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(to_targets(rows))
}

/// Selects tokens for a previously authenticated and admitted direct-call pair.
/// Read recipient consent and current device status in the same statement.
pub async fn voip_push_targets(
    pool: &PgPool,
    recipient: &str,
    sender: &str,
    space: &str,
) -> Result<Vec<crate::push::Target>, sqlx::Error> {
    voip_registrations(pool, recipient, sender, space, None).await
}

pub async fn voip_registration_current(
    pool: &PgPool,
    recipient: &str,
    sender: &str,
    space: &str,
    registration: i64,
) -> Result<bool, sqlx::Error> {
    Ok(!voip_registrations(pool, recipient, sender, space, Some(registration))
        .await?
        .is_empty())
}

async fn voip_registrations(
    pool: &PgPool,
    recipient: &str,
    sender: &str,
    space: &str,
    registration: Option<i64>,
) -> Result<Vec<crate::push::Target>, sqlx::Error> {
    let rows = sqlx::query_as(
        "SELECT p.platform, p.token, p.registration_id FROM push_tokens p
         JOIN devices d ON d.identity = p.identity AND d.device = p.device AND d.revoked_at IS NULL
         JOIN voip_consent c ON c.recipient = p.identity
         WHERE p.identity = $1 AND c.sender = $2 AND c.space = $3 AND p.purpose = 'voip'
           AND ($4::bigint IS NULL OR p.registration_id = $4)",
    )
    .bind(recipient)
    .bind(sender)
    .bind(space)
    .bind(registration)
    .fetch_all(pool)
    .await?;
    Ok(to_targets(rows))
}

fn to_targets(rows: Vec<(String, String, i64)>) -> Vec<crate::push::Target> {
    rows.into_iter()
        .map(|(platform, token, registration_id)| crate::push::Target {
            platform,
            token,
            registration_id,
        })
        .collect()
}

pub async fn drop_push_token(
    pool: &PgPool,
    target: &crate::push::Target,
) -> Result<(), sqlx::Error> {
    let purpose = crate::push::purpose(&target.platform)
        .ok_or_else(|| sqlx::Error::Protocol("invalid push registration purpose".into()))?;
    // An invalid provider response only applies to the registration it targeted.
    sqlx::query(
        "DELETE FROM push_tokens WHERE token = $1 AND purpose = $2 AND registration_id = $3",
    )
    .bind(&target.token)
    .bind(purpose)
    .bind(target.registration_id)
    .execute(pool)
    .await
    .map(|_| ())
}

/// Update consent after validating the direct conversation against the event log.
/// Revoked devices cannot change the recipient's policy.
pub async fn set_voip_consent(
    pool: &PgPool,
    recipient: &str,
    device: &str,
    sender: &str,
    space: &str,
    allowed: bool,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT true FROM devices WHERE identity = $1 AND device = $2
         AND revoked_at IS NULL FOR UPDATE",
    )
    .bind(recipient)
    .bind(device)
    .fetch_optional(&mut *tx)
    .await?;
    if active.is_none() {
        return Err(sqlx::Error::Protocol(
            "VoIP consent device is not active".into(),
        ));
    }
    let statement = if allowed {
        "INSERT INTO voip_consent (recipient, sender, space) VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING"
    } else {
        "DELETE FROM voip_consent WHERE recipient = $1 AND sender = $2 AND space = $3"
    };
    sqlx::query(statement)
        .bind(recipient)
        .bind(sender)
        .bind(space)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

/// Reserve a best-effort dispatch after checking the committed event and its fixed
/// expiry. An ambiguous provider outcome must never release this reservation.
pub async fn claim_voip_wake(
    pool: &PgPool,
    space: &str,
    sequence: i64,
    expires_ms: i64,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let now_ms: i64 =
        sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint")
            .fetch_one(&mut *tx)
            .await?;
    if expires_ms <= now_ms || expires_ms > now_ms.saturating_add(60_000) {
        return Ok(false);
    }
    sqlx::query(
        "DELETE FROM voip_wake_claims WHERE (space, sequence) IN (
           SELECT space, sequence FROM voip_wake_claims WHERE expires_ms <= $1
           ORDER BY expires_ms LIMIT 128 FOR UPDATE SKIP LOCKED
         )",
    )
    .bind(now_ms)
    .execute(&mut *tx)
    .await?;
    let inserted = sqlx::query(
        "INSERT INTO voip_wake_claims (space, sequence, expires_ms)
         SELECT $1, $2, $3 WHERE $3 > floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint
         ON CONFLICT DO NOTHING",
    )
    .bind(space)
    .bind(sequence)
    .bind(expires_ms)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(inserted.rows_affected() == 1)
}

pub async fn get_voip_consent(
    pool: &PgPool,
    recipient: &str,
    sender: &str,
    space: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM voip_consent WHERE recipient = $1 AND sender = $2 AND space = $3)",
    )
    .bind(recipient)
    .bind(sender)
    .bind(space)
    .fetch_one(pool)
    .await
}

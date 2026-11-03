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
    let mut tx = pool.begin().await?;
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
    sqlx::query("DELETE FROM push_tokens WHERE purpose = $4 AND ((identity = $1 AND device = $2) OR (token = $3 AND $3 <> ''))")
        .bind(identity)
        .bind(device)
        .bind(token)
        .bind(purpose)
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
    push_targets_for_purpose(pool, ids, "notification").await
}

/// Provider lookup only; no generic message wake calls this path.
pub async fn voip_push_targets(
    pool: &PgPool,
    ids: &[String],
) -> Result<Vec<crate::push::Target>, sqlx::Error> {
    push_targets_for_purpose(pool, ids, "voip").await
}

async fn push_targets_for_purpose(
    pool: &PgPool,
    ids: &[String],
    purpose: &str,
) -> Result<Vec<crate::push::Target>, sqlx::Error> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT p.platform, p.token, p.registration_id FROM push_tokens p
         JOIN devices d ON d.identity = p.identity AND d.device = p.device AND d.revoked_at IS NULL
         WHERE p.identity = ANY($1) AND p.purpose = $2",
    )
    .bind(ids)
    .bind(purpose)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(platform, token, registration_id)| crate::push::Target {
            platform,
            token,
            registration_id,
        })
        .collect())
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

//! Agent citizens on the relay (migration 0028).
//!
//! The relay stores an opaque definition blob and which device claimed the Agent identity.
//! It never runs the model. `zoen-agentd` claims the row, publishes key packages, and wakes
//! on Closed-space traffic for members while the owner is offline.

use sqlx::{PgPool, Postgres, Transaction};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitizenClaim {
    pub agent: String,
    pub device: String,
    pub kind: String,
    pub definition: Vec<u8>,
    pub definition_version: i32,
    pub avatar_style: String,
    pub avatar_glyph: Option<String>,
    pub avatar_tint: Option<String>,
    pub avatar_asset: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentCitizenError {
    #[error("agent identity is missing or not an Agent")]
    NotAgent,
    #[error("device is not enrolled under that agent")]
    Device,
    #[error("kind must be Creator or User")]
    Kind,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn valid_kind(kind: &str) -> bool {
    kind == "Creator" || kind == "User"
}

/// Claim or refresh the runtime device for an Agent. The caller already enrolled the device.
pub async fn claim_citizen(
    tx: &mut Transaction<'_, Postgres>,
    claim: &CitizenClaim,
) -> Result<(), AgentCitizenError> {
    if !valid_kind(&claim.kind) {
        return Err(AgentCitizenError::Kind);
    }
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT kind FROM identities WHERE id = $1 FOR SHARE",
    )
    .bind(&claim.agent)
    .fetch_optional(&mut **tx)
    .await?;
    match row.as_ref().map(|(k,)| k.as_str()) {
        Some("Agent") => {}
        _ => return Err(AgentCitizenError::NotAgent),
    }
    let device_ok: Option<(String,)> = sqlx::query_as(
        "SELECT device FROM devices WHERE device = $1 AND identity = $2 FOR SHARE",
    )
    .bind(&claim.device)
    .bind(&claim.agent)
    .fetch_optional(&mut **tx)
    .await?;
    if device_ok.is_none() {
        return Err(AgentCitizenError::Device);
    }
    sqlx::query(
        r#"
        INSERT INTO agent_citizens (
            agent, device, definition_version, kind, definition,
            avatar_style, avatar_glyph, avatar_tint, avatar_asset, last_seen_at
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9, now())
        ON CONFLICT (agent) DO UPDATE SET
            device = EXCLUDED.device,
            definition_version = EXCLUDED.definition_version,
            kind = EXCLUDED.kind,
            definition = EXCLUDED.definition,
            avatar_style = EXCLUDED.avatar_style,
            avatar_glyph = EXCLUDED.avatar_glyph,
            avatar_tint = EXCLUDED.avatar_tint,
            avatar_asset = EXCLUDED.avatar_asset,
            last_seen_at = now()
        "#,
    )
    .bind(&claim.agent)
    .bind(&claim.device)
    .bind(claim.definition_version)
    .bind(&claim.kind)
    .bind(&claim.definition)
    .bind(&claim.avatar_style)
    .bind(&claim.avatar_glyph)
    .bind(&claim.avatar_tint)
    .bind(&claim.avatar_asset)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Agent members of a Space that have a claimed runtime device (eligible to wake).
pub async fn wake_candidates(
    pool: &PgPool,
    space: &str,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        r#"
        SELECT c.agent, c.device
        FROM members m
        JOIN agent_citizens c ON c.agent = m.identity
        WHERE m.space = $1
        "#,
    )
    .bind(space)
    .fetch_all(pool)
    .await
}

/// Advance the agent's cursor after it applied traffic through `seq` (inclusive).
pub async fn advance_cursor(
    pool: &PgPool,
    agent: &str,
    space: &str,
    seq: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO agent_space_cursors (agent, space, last_seq)
        VALUES ($1, $2, $3)
        ON CONFLICT (agent, space) DO UPDATE SET
            last_seq = GREATEST(agent_space_cursors.last_seq, EXCLUDED.last_seq)
        "#,
    )
    .bind(agent)
    .bind(space)
    .bind(seq)
    .execute(pool)
    .await?;
    sqlx::query("UPDATE agent_citizens SET last_wake_at = now(), last_seen_at = now() WHERE agent = $1")
        .bind(agent)
        .execute(pool)
        .await?;
    Ok(())
}

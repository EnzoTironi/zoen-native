//! Agent citizens on the relay (migration 0028).
//!
//! The relay stores which device claimed an Agent identity, its owner, an opaque versioned
//! definition and the runtime's per-Space cursors. It never runs the model. `zoen-agentd`
//! wakes through the relay's ordinary fan-out: the agent's own session is a member like
//! anyone. These rows are the single source of truth the runtime resumes from.

use roda_proto::{AgentAction, AgentStateRecord};
use sqlx::{PgPool, Postgres, Transaction};

/// Rows of `agent_citizens`, for operators: (agent, device, kind).
pub async fn citizens(pool: &PgPool) -> Result<Vec<(String, String, Option<String>)>, sqlx::Error> {
    sqlx::query_as("SELECT agent, device, kind FROM agent_citizens ORDER BY claimed_at")
        .fetch_all(pool)
        .await
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitizenClaim {
    pub agent: String,
    pub device: String,
    pub avatar_style: String,
    pub avatar_glyph: Option<String>,
    pub avatar_tint: Option<String>,
    pub avatar_asset: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AgentCitizenError {
    #[error("agent identity is missing, not an Agent or has no owner")]
    NotAgent,
    #[error("device is not an active device of that agent")]
    Device,
    #[error("kind must be Creator or User")]
    Kind,
    #[error("only the agent's owner may do that")]
    Owner,
    #[error("the agent was revoked")]
    Revoked,
    #[error("definition version must grow")]
    Version,
    #[error("the agent is not a member of that Space")]
    NotMember,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn valid_kind(kind: &str) -> bool {
    kind == "Creator" || kind == "User"
}

/// Claim or move the runtime device for an Agent. The caller already enrolled the device.
/// The device must be active and the agent's own; a revoked agent can't be claimed again.
pub async fn claim_citizen(
    tx: &mut Transaction<'_, Postgres>,
    claim: &CitizenClaim,
) -> Result<(), AgentCitizenError> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT kind, owner FROM identities WHERE id = $1 FOR SHARE")
            .bind(&claim.agent)
            .fetch_optional(&mut **tx)
            .await?;
    let owner = match row {
        Some((k, Some(owner))) if k == "Agent" => owner,
        _ => return Err(AgentCitizenError::NotAgent),
    };
    let device_ok: Option<(String,)> = sqlx::query_as(
        "SELECT device FROM devices WHERE device = $1 AND identity = $2 AND revoked_at IS NULL FOR SHARE",
    )
    .bind(&claim.device)
    .bind(&claim.agent)
    .fetch_optional(&mut **tx)
    .await?;
    if device_ok.is_none() {
        return Err(AgentCitizenError::Device);
    }
    let r = sqlx::query(
        r#"
        INSERT INTO agent_citizens (
            agent, owner, device, avatar_style, avatar_glyph, avatar_tint, avatar_asset, last_seen_at
        ) VALUES ($1,$2,$3,$4,$5,$6,$7, now())
        ON CONFLICT (agent) DO UPDATE SET
            device = EXCLUDED.device,
            avatar_style = EXCLUDED.avatar_style,
            avatar_glyph = EXCLUDED.avatar_glyph,
            avatar_tint = EXCLUDED.avatar_tint,
            avatar_asset = EXCLUDED.avatar_asset,
            last_seen_at = now()
        WHERE agent_citizens.revoked_at IS NULL
        "#,
    )
    .bind(&claim.agent)
    .bind(&owner)
    .bind(&claim.device)
    .bind(&claim.avatar_style)
    .bind(&claim.avatar_glyph)
    .bind(&claim.avatar_tint)
    .bind(&claim.avatar_asset)
    .execute(&mut **tx)
    .await?;
    if r.rows_affected() != 1 {
        return Err(AgentCitizenError::Revoked);
    }
    Ok(())
}

/// The runtime session (`agent`, `device`) must be the live claim.
async fn runtime(pool: &PgPool, agent: &str, device: &str) -> Result<(), AgentCitizenError> {
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT revoked_at IS NULL FROM agent_citizens WHERE agent = $1 AND device = $2",
    )
    .bind(agent)
    .bind(device)
    .fetch_optional(pool)
    .await?;
    match row {
        Some((true,)) => Ok(()),
        Some((false,)) => Err(AgentCitizenError::Revoked),
        None => Err(AgentCitizenError::Device),
    }
}

/// kind, definition_version, definition, paused, revoked, device.
type StateRow = (Option<String>, i32, Vec<u8>, bool, bool, String);

/// Status, definition and cursors for the signed-in runtime. Cursors of Spaces the agent
/// is no longer in (`member_of`) are dropped here: membership lives in FoundationDB.
pub async fn state(
    pool: &PgPool,
    agent: &str,
    device: &str,
    member_of: &[String],
) -> Result<AgentStateRecord, AgentCitizenError> {
    let row: Option<StateRow> = sqlx::query_as(
        "SELECT kind, definition_version, definition, paused_at IS NOT NULL, revoked_at IS NOT NULL, device
         FROM agent_citizens WHERE agent = $1",
    )
    .bind(agent)
    .fetch_optional(pool)
    .await?;
    let Some((kind, version, definition, paused, revoked, claimed)) = row else {
        return Err(AgentCitizenError::Device);
    };
    if claimed != device {
        return Err(AgentCitizenError::Device);
    }
    sqlx::query("DELETE FROM agent_space_cursors WHERE agent = $1 AND NOT (space = ANY($2))")
        .bind(agent)
        .bind(member_of)
        .execute(pool)
        .await?;
    let cursors: Vec<(String, i64)> =
        sqlx::query_as("SELECT space, last_seq FROM agent_space_cursors WHERE agent = $1")
            .bind(agent)
            .fetch_all(pool)
            .await?;
    sqlx::query("UPDATE agent_citizens SET last_seen_at = now() WHERE agent = $1")
        .bind(agent)
        .execute(pool)
        .await?;
    Ok(AgentStateRecord {
        status: if revoked {
            "revoked"
        } else if paused {
            "paused"
        } else {
            "active"
        }
        .into(),
        kind: kind.unwrap_or_default(),
        definition_version: version.max(0) as u32,
        definition,
        cursors: cursors
            .into_iter()
            .map(|(s, q)| (s, q.max(0) as u64))
            .collect(),
    })
}

/// Advance the runtime's cursor after it handled traffic through `seq` (inclusive).
/// The caller checked membership. Never moves backwards.
pub async fn advance_cursor(
    pool: &PgPool,
    agent: &str,
    device: &str,
    space: &str,
    seq: u64,
) -> Result<(), AgentCitizenError> {
    runtime(pool, agent, device).await?;
    let seq = i64::try_from(seq).map_err(|_| AgentCitizenError::Version)?;
    sqlx::query(
        r#"
        INSERT INTO agent_space_cursors (agent, space, last_seq)
        VALUES ($1, $2, $3)
        ON CONFLICT (agent, space) DO UPDATE SET
            last_seq = GREATEST(agent_space_cursors.last_seq, EXCLUDED.last_seq),
            updated_at = now()
        "#,
    )
    .bind(agent)
    .bind(space)
    .bind(seq)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE agent_citizens SET last_wake_at = now(), last_seen_at = now() WHERE agent = $1",
    )
    .bind(agent)
    .execute(pool)
    .await?;
    Ok(())
}

/// A new definition version. The owner may always define; the agent's own runtime only
/// its first definition (registration), never a later one.
pub async fn define(
    pool: &PgPool,
    caller: &str,
    caller_device: &str,
    agent: &str,
    version: u32,
    kind: &str,
    definition: &[u8],
) -> Result<(), AgentCitizenError> {
    if !valid_kind(kind) {
        return Err(AgentCitizenError::Kind);
    }
    let mut tx = pool.begin().await?;
    let row: Option<(String, String, i32, bool)> = sqlx::query_as(
        "SELECT owner, device, definition_version, revoked_at IS NOT NULL FROM agent_citizens
         WHERE agent = $1 FOR UPDATE",
    )
    .bind(agent)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((owner, device, current, revoked)) = row else {
        return Err(AgentCitizenError::NotAgent);
    };
    if revoked {
        return Err(AgentCitizenError::Revoked);
    }
    let by_owner = caller == owner;
    let by_runtime = caller == agent && caller_device == device && current == 0;
    if !by_owner && !by_runtime {
        return Err(AgentCitizenError::Owner);
    }
    let version = i32::try_from(version).map_err(|_| AgentCitizenError::Version)?;
    if version <= current {
        return Err(AgentCitizenError::Version);
    }
    sqlx::query(
        "INSERT INTO agent_definitions (agent, version, kind, definition, defined_by)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(agent)
    .bind(version)
    .bind(kind)
    .bind(definition)
    .bind(caller)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE agent_citizens SET kind = $2, definition_version = $3, definition = $4 WHERE agent = $1",
    )
    .bind(agent)
    .bind(kind)
    .bind(version)
    .bind(definition)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Owner pauses, resumes or revokes an agent. Revoking also revokes its runtime device,
/// so the relay stops letting it in, and is final.
pub async fn control(
    pool: &PgPool,
    caller: &str,
    agent: &str,
    action: AgentAction,
) -> Result<(), AgentCitizenError> {
    let mut tx = pool.begin().await?;
    let row: Option<(String, String, bool)> = sqlx::query_as(
        "SELECT owner, device, revoked_at IS NOT NULL FROM agent_citizens WHERE agent = $1 FOR UPDATE",
    )
    .bind(agent)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((owner, device, revoked)) = row else {
        return Err(AgentCitizenError::NotAgent);
    };
    if caller != owner {
        return Err(AgentCitizenError::Owner);
    }
    if revoked {
        return Err(AgentCitizenError::Revoked);
    }
    let sql = match action {
        AgentAction::Pause => "UPDATE agent_citizens SET paused_at = now() WHERE agent = $1",
        AgentAction::Resume => "UPDATE agent_citizens SET paused_at = NULL WHERE agent = $1",
        AgentAction::Revoke => "UPDATE agent_citizens SET revoked_at = now() WHERE agent = $1",
    };
    sqlx::query(sql).bind(agent).execute(&mut *tx).await?;
    if action == AgentAction::Revoke {
        crate::db::revoke_device(&mut tx, agent, &device).await?;
    }
    tx.commit().await?;
    Ok(())
}

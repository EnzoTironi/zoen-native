//! Atomic lifecycle CAS in the existing citizen authority, not a second ledger.
use super::{AgentAction, AgentCitizenError};
use protocol::agent_control::Record;
use sqlx::PgPool;

type Row = (String, String, bool, bool, i64, String, String, String);
fn record(row: &Row) -> Record {
    Record {
        status: if row.3 {
            "revoked"
        } else if row.2 {
            "paused"
        } else {
            "active"
        }
        .into(),
        revision: row.4 as u64,
        operation: row.5.clone(),
        action: row.6.clone(),
        device: row.7.clone(),
    }
}
const SELECT: &str = "SELECT owner, device, paused_at IS NOT NULL, revoked_at IS NOT NULL,
 control_revision, control_operation, control_action, control_device FROM agent_citizens WHERE agent=$1";

pub async fn control_status(
    pool: &PgPool,
    owner: &str,
    agent: &str,
) -> Result<Record, AgentCitizenError> {
    let row: Row = sqlx::query_as(SELECT)
        .bind(agent)
        .fetch_optional(pool)
        .await?
        .ok_or(AgentCitizenError::NotAgent)?;
    if row.0 != owner {
        return Err(AgentCitizenError::Owner);
    }
    Ok(record(&row))
}

pub async fn control_reviewed(
    pool: &PgPool,
    owner: &str,
    device: &str,
    agent: &str,
    action: AgentAction,
    operation: &str,
    revision: u64,
) -> Result<Record, AgentCitizenError> {
    if operation.len() != 64
        || !operation.bytes().all(|b| b.is_ascii_hexdigit())
        || revision == 0
        || revision >= i64::MAX as u64
    {
        return Err(AgentCitizenError::Version);
    }
    let mut tx = pool.begin().await?;
    let row: Row = sqlx::query_as("SELECT owner, device, paused_at IS NOT NULL, revoked_at IS NOT NULL, control_revision, control_operation, control_action, control_device FROM agent_citizens WHERE agent=$1 FOR UPDATE")
        .bind(agent)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AgentCitizenError::NotAgent)?;
    if row.0 != owner {
        return Err(AgentCitizenError::Owner);
    }
    let current = record(&row);
    if current.operation == operation
        && (current.revision == revision
            || current.action != action.id()
            || current.device != device)
    {
        return Err(AgentCitizenError::Version);
    }
    // An exact duplicate returns its receipt; conflicts only observe the winner.
    // Never apply a delayed request at a revision advanced by another device.
    if current.revision != revision || current.status == "revoked" {
        return Ok(current);
    }
    sqlx::query("UPDATE agent_citizens SET
        paused_at=CASE WHEN $3='pause' THEN now() WHEN $3='resume' THEN NULL ELSE paused_at END,
        revoked_at=CASE WHEN $3='revoke' THEN now() ELSE revoked_at END,
        control_revision=control_revision+1, control_operation=$2, control_action=$3, control_device=$4 WHERE agent=$1")
        .bind(agent).bind(operation).bind(action.id()).bind(device).execute(&mut *tx).await?;
    if action == AgentAction::Revoke {
        crate::db::revoke_device(&mut tx, agent, &row.1).await?;
    }
    let next: Row = sqlx::query_as(SELECT)
        .bind(agent)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(record(&next))
}

/// SQL lifecycle is the permission boundary even if an old FDB link still exists.
/// Database errors and missing citizens never authorize an invite.
pub async fn invite_live(pool: &PgPool, agent: &str) -> Result<bool, AgentCitizenError> {
    let live: Option<bool> =
        sqlx::query_scalar("SELECT revoked_at IS NULL FROM agent_citizens WHERE agent=$1")
            .bind(agent)
            .fetch_optional(pool)
            .await?;
    Ok(matches!(live, Some(true)))
}

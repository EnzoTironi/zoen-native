//! Current conversation control is separate from a paid original's definition.
use super::*;

impl NativeDirectoryGuard {
    /// Scheduling observation only. Fresh admission still checks active state
    /// under its own directory locks before any financial or transport effect.
    pub(crate) async fn reply_control_revision(mut self) -> Result<Option<u64>, RuntimeError> {
        let row: Option<(String, i64)> = sqlx::query_as(
            "SELECT owner,control_revision FROM agent_citizens WHERE agent=$1 FOR SHARE",
        )
        .bind(&self.agent)
        .fetch_optional(&mut *self.tx)
        .await?;
        let revision = match row {
            Some((owner, revision)) if owner == self.owner && revision > 0 => {
                Some(u64::try_from(revision).map_err(|_| RuntimeError::InvalidBinding)?)
            }
            Some(_) => return Err(RuntimeError::InvalidBinding),
            None => None,
        };
        self.tx.commit().await?;
        Ok(revision)
    }

    /// Hold the canonical citizen control row through this authorization's SQL
    /// commit. Never use this for historical receipts or incurred-result recovery.
    pub(crate) async fn active_reply_actor(mut self) -> Result<Self, RuntimeError> {
        let row: Option<(String, bool, bool)> = sqlx::query_as(
            "SELECT owner,paused_at IS NOT NULL,revoked_at IS NOT NULL FROM agent_citizens WHERE agent=$1 FOR SHARE",
        ).bind(&self.agent).fetch_optional(&mut *self.tx).await?;
        if let Some((owner, paused, revoked)) = row {
            if owner != self.owner || paused || revoked {
                return Err(RuntimeError::Denied);
            }
        }
        // No citizen is the existing unclaimed text-only profile, not a fallback
        // for an existing paused/revoked record. Claims start active and do not
        // themselves authorize a native grant, model call or publication.
        Ok(self)
    }
}

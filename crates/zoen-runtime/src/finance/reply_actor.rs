//! Current conversation control is separate from a paid original's definition.
use super::*;

impl NativeDirectoryGuard {
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

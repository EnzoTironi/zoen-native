//! A read of the existing conversation index under the certified source turn.
use crate::{engine::R, Client, CoreError, MessageSearchPage};

/// A tool page stays small enough for the normal citizen context and round budget.
pub const AGENT_HISTORY_PAGE_LIMIT: u32 = 20;

impl Client {
    /// Search only the host-bound conversation as the current certified agent.
    /// The model cannot supply a conversation, actor, source or serving identity.
    ///
    /// # Errors
    /// Refuses changed account/device/member/source authority, artifact-only turns,
    /// invalid query/page bounds and stale cursors. Index/storage errors propagate.
    pub fn agent_search_messages(
        &self,
        space: &str,
        source: &str,
        serving: &str,
        query: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> R<MessageSearchPage> {
        let mut engine = self.lock();
        engine.agent_source_authority(space, source, serving, false)?;
        if !(1..=AGENT_HISTORY_PAGE_LIMIT).contains(&limit) {
            return Err(CoreError::Invalid {
                reason: "Search one page of 1 to 20 conversation messages.".into(),
            });
        }
        engine.search_messages(space, query, cursor, limit)
    }
}

#[cfg(test)]
mod tests;

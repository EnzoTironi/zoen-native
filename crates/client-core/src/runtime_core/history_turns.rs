//! Revalidate retained read data against the current native conversation.
use super::*;
use crate::MessageSearchPage;
use std::collections::BTreeMap;

/// Retained data, never a read permit. Every replay checks current authority.
pub struct ReplyHistoryRead {
    pub arguments: serde_json::Value,
    pub page: MessageSearchPage,
}
impl DeviceCore {
    /// New messages may arrive while a model is thinking. Keep using a retained
    /// result only if the genuine current read still returns the same page.
    pub fn verify_reply_history(
        mut self,
        original: &ReplyIntent,
        reads: &[ReplyHistoryRead],
        at_ms: i64,
    ) -> Result<()> {
        self.replay_history(original, reads, at_ms).map(|_| ())
    }

    /// Follow a cursor supplied by this turn's own verified earlier result.
    /// Translate it through fresh native pages rather than relaxing cursor checks.
    pub fn continue_reply_history(
        mut self,
        original: &ReplyIntent,
        reads: &[ReplyHistoryRead],
        arguments: &serde_json::Value,
        at_ms: i64,
    ) -> Result<MessageSearchPage> {
        let cursors = self.replay_history(original, reads, at_ms)?;
        let arguments = mapped(arguments, &cursors)?;
        self.search_history_page(original, &arguments, at_ms)
    }

    fn replay_history(
        &mut self,
        original: &ReplyIntent,
        reads: &[ReplyHistoryRead],
        at_ms: i64,
    ) -> Result<BTreeMap<String, String>> {
        if reads.len() > 3 {
            return Err(NativeError::Bounds);
        }
        self.refresh(original, at_ms)?;
        let mut cursors = BTreeMap::new();
        for read in reads {
            let arguments = mapped(&read.arguments, &cursors)?;
            let current = self.search_history_page(original, &arguments, at_ms)?;
            if current.hits != read.page.hits
                || current.next_cursor.is_some() != read.page.next_cursor.is_some()
            {
                return Err(NativeError::History);
            }
            if let (Some(old), Some(new)) = (&read.page.next_cursor, current.next_cursor) {
                super::history_search::retained_cursor(original, old)?;
                if old.len() > 4096
                    || cursors
                        .insert(old.clone(), new.clone())
                        .is_some_and(|prior| prior != new)
                {
                    return Err(NativeError::History);
                }
            }
        }
        Ok(cursors)
    }
}
fn mapped(
    arguments: &serde_json::Value,
    cursors: &BTreeMap<String, String>,
) -> Result<serde_json::Value> {
    let query =
        crate::agent_conversation_tools::parse(arguments).map_err(|_| NativeError::Bounds)?;
    let mut arguments = arguments.clone();
    if let Some(cursor) = query.cursor {
        let current = cursors.get(&cursor).ok_or(NativeError::History)?;
        arguments["cursor"] = current.clone().into();
    }
    Ok(arguments)
}

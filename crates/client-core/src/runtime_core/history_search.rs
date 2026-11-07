//! Read-only search consumes a scratch view; index changes never enter custody.
use super::*;
use crate::{CoreError, MessageSearchPage};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    intent: String,
    frontier: String,
    inner: String,
}
impl DeviceCore {
    /// Search the original owner conversation using the same Core read authority
    /// as Client. Consume this temporary view so index rebuilds cannot alter the
    /// retained device image or make a cold retry mint a new index generation.
    pub fn search_reply_history(
        mut self,
        original: &ReplyIntent,
        arguments: &serde_json::Value,
        at_ms: i64,
    ) -> Result<MessageSearchPage> {
        self.search_history_page(original, arguments, at_ms)
    }

    pub(super) fn search_history_page(
        &mut self,
        original: &ReplyIntent,
        arguments: &serde_json::Value,
        at_ms: i64,
    ) -> Result<MessageSearchPage> {
        let query = crate::agent_conversation_tools::parse(arguments).map_err(search_error)?;
        let facts = self.refresh(original, at_ms)?;
        let intent =
            event_log::sha256_hex(&serde_json::to_vec(original).map_err(|_| NativeError::History)?);
        let cursor = query
            .cursor
            .as_deref()
            .map(|value| {
                let value: Cursor = serde_json::from_str(value).map_err(|_| NativeError::Bounds)?;
                if value.intent != intent || value.frontier != facts.frontier().hash() {
                    return Err(NativeError::History);
                }
                Ok(value.inner)
            })
            .transpose()?;
        let message = self
            .engine
            .space_state(original.space())
            .map_err(search_error)?
            .entries
            .iter()
            .find(|entry| entry.hash == original.trigger())
            .ok_or(NativeError::History)?
            .id()
            .to_owned();
        // A prepared reply is visible locally before the relay orders it. It
        // cannot change the source page that authorized that same reply. Only
        // this consumed scratch index changes; the pending envelope stays intact.
        if self.engine.index_dirty {
            self.engine.reindex().map_err(search_error)?;
        }
        let pending: Vec<_> = self
            .engine
            .space_state(original.space())
            .map_err(search_error)?
            .entries
            .iter()
            .filter(|entry| entry.hash.is_empty())
            .map(|entry| entry.id().to_owned())
            .collect();
        for pending in pending {
            self.engine
                .store
                .index_remove_message(original.space(), &pending)
                .map_err(|_| NativeError::Storage)?;
        }
        let mut page = self
            .engine
            .agent_search_messages(
                original.space(),
                &message,
                &self.owner,
                &query.query,
                cursor.as_deref(),
                query.limit,
            )
            .map_err(search_error)?;
        page.next_cursor = page
            .next_cursor
            .map(|inner| {
                let encoded = serde_json::to_string(&Cursor {
                    intent,
                    frontier: facts.frontier().hash().into(),
                    inner,
                })
                .map_err(|_| NativeError::Storage)?;
                if encoded.len() > 4096 {
                    return Err(NativeError::Bounds);
                }
                Ok(encoded)
            })
            .transpose()?;
        Ok(page)
    }
}
pub(super) fn retained_cursor(original: &ReplyIntent, cursor: &str) -> Result<()> {
    if cursor.len() > 4096 {
        return Err(NativeError::Bounds);
    }
    let cursor: Cursor = serde_json::from_str(cursor).map_err(|_| NativeError::History)?;
    let intent =
        event_log::sha256_hex(&serde_json::to_vec(original).map_err(|_| NativeError::History)?);
    if cursor.intent != intent
        || cursor.frontier.len() != 64
        || !cursor.frontier.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(NativeError::History);
    }
    Ok(())
}

fn search_error(error: CoreError) -> NativeError {
    match error {
        CoreError::Storage { .. } => NativeError::Storage,
        CoreError::Invalid { .. } => NativeError::Bounds,
        CoreError::Stale { .. } => NativeError::History,
        CoreError::Forbidden { .. }
        | CoreError::NotFound { .. }
        | CoreError::DraftRejected { .. } => NativeError::Permission,
    }
}

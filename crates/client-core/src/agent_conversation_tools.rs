//! Closed, read-only conversation tools on the existing signed owner review.
use crate::{engine::R, Client, CoreError};
use serde::Deserialize;
use serde_json::Value;

mod contract;
mod recovery;
mod review;
pub(crate) use contract::{access, matches, parsed};
pub use review::ApprovedConversationSearch;

pub const SCHEMA: u8 = 7;
pub const TOOL: &str = "search_conversation_history";
const REVISION: &str = "zoen.conversation-search.v1";

#[must_use]
pub fn executor_binding() -> String {
    crate::agent_page_tools::digest(b"zoen.conversation-search.v1\0source-bound-native-index")
}

/// Arguments cannot choose an actor, conversation, source or document.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub query: String,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "page_limit")]
    pub limit: u32,
}
fn page_limit() -> u32 {
    10
}

pub fn parse(arguments: &Value) -> R<Query> {
    let query: Query = serde_json::from_value(arguments.clone()).map_err(|_| invalid())?;
    if query.query.trim().is_empty()
        || query.query.len() > 512
        || query
            .cursor
            .as_ref()
            .is_some_and(|cursor| cursor.len() > 4096)
        || !(1..=crate::AGENT_HISTORY_PAGE_LIMIT).contains(&query.limit)
    {
        return Err(invalid());
    }
    Ok(query)
}

fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: "Invalid conversation history review or query.".into(),
    }
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: "The reviewed conversation search, source or access changed.".into(),
    }
}

#[cfg(test)]
mod tests;

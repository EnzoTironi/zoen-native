//! Same scope/head/proposal contract as the existing managed Deck host.
use crate::{Result, failure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Scope {
    pub artifact_id: String,
    pub actor_id: String,
    pub actor_kind: ActorKind,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ActorKind {
    Human,
    Agent,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Head {
    pub version: String,
    pub sha256: String,
}
impl Head {
    /// # Errors
    /// Refuses noncanonical decimal versions, overflow or a non-lowercase SHA-256.
    pub fn number(&self) -> Result<u64> {
        if self.version.is_empty()
            || self.version.len() > 20
            || (self.version.len() > 1 && self.version.starts_with('0'))
            || !self.version.bytes().all(|b| b.is_ascii_digit())
            || self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(failure("invalid_head"));
        }
        self.version.parse().map_err(|_| failure("invalid_head"))
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Open {
    pub scope: Scope,
    pub head: Head,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Mutation {
    pub scope: Scope,
    pub base: Head,
    pub mutation_id: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub mutation: Mutation,
    pub action: Action,
}
pub use zoen_word_profile::action::{Action, TextReplace};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Accept {
    pub scope: Scope,
    pub mutation_id: String,
    pub head: Head,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Export {
    pub scope: Scope,
    pub head: Head,
    pub format: ExportFormat,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Docx,
    Pdf,
}
#[derive(Clone)]
pub struct Proposal {
    pub bytes: Vec<u8>,
    pub metadata: Value,
}
pub(crate) fn identifier(s: &str) -> bool {
    !s.is_empty() && s.len() <= 200 && !s.chars().any(char::is_control)
}

//! Private folder configuration; never a conversation membership grant.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum InboxFolderKind {
    Direct,
    Group,
    Community,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct InboxFolder {
    pub id: String,
    pub name: String,
    pub kinds: Vec<InboxFolderKind>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub exclude_read: bool,
    pub exclude_muted: bool,
    pub exclude_archived: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct InboxFolders {
    pub folders: Vec<InboxFolder>,
    pub default_folder: String,
}
impl Default for InboxFolders {
    fn default() -> Self {
        Self {
            folders: Vec::new(),
            default_folder: "all".into(),
        }
    }
}

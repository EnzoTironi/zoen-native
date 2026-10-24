//! Private Photos identities resume the same journal assets, never a file lease or send intent.
use super::{invalid, Attachment, R};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Gallery {
    pub entries: Vec<Selection>,
    pub forced_caption: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Selection {
    pub asset_id: String,
    pub photo_id: String,
}
impl Gallery {
    pub fn validate(&self, items: &[Attachment]) -> R<()> {
        if self.entries.is_empty() || self.entries.len() > items.len() {
            return Err(invalid());
        }
        if self.forced_caption
            && items[items.len() - self.entries.len()..]
                .iter()
                .skip(1)
                .any(|item| !item.caption.is_empty())
        {
            return Err(invalid());
        }
        let mut photos = HashSet::new();
        let suffix = &items[items.len() - self.entries.len()..];
        for (entry, item) in self.entries.iter().zip(suffix) {
            if entry.asset_id != item.asset_id
                || !entry.photo_id.starts_with("ph://")
                || entry.photo_id.len() <= 5
                || entry.photo_id.len() > 2048
                || entry.photo_id.chars().any(char::is_control)
                || !photos.insert(&entry.photo_id)
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

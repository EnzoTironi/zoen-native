//! Attachment intent is separate from the real file MIME and immutable bytes.
//! Albums are explicit signed membership, never inferred from timing or adjacency.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentPresentation {
    #[default]
    Media,
    File,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentGrouping {
    #[default]
    Separate,
    Grouped,
}

/// Options belong to the existing draft revision and frozen send intent.
/// There is deliberately no quality preset until a real transform can honor it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentSendOptions {
    pub presentation: AttachmentPresentation,
    pub grouping: AttachmentGrouping,
}

#[derive(Debug)]
pub enum MediaMetadataError {
    InvalidIntent,
    Encoding(serde_json::Error),
}

impl std::fmt::Display for MediaMetadataError {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIntent => {
                output.write_str("Invalid attachment count, index or mutation sequence.")
            }
            Self::Encoding(error) => {
                write!(output, "Could not encode attachment metadata: {error}")
            }
        }
    }
}

impl std::error::Error for MediaMetadataError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Encoding(error) => Some(error),
            Self::InvalidIntent => None,
        }
    }
}

impl AttachmentSendOptions {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    #[must_use]
    pub fn validate_count(&self, count: usize) -> bool {
        (1..=10).contains(&count) && (self.grouping != AttachmentGrouping::Grouped || count >= 2)
    }

    /// The sender derives the album id from the entire ordered immutable intent.
    /// Count, index and unique immutable mutations are checked before encoding.
    ///
    /// # Errors
    /// Rejects an invalid intent or a metadata encoding failure. Failure never
    /// degrades a grouped intent into separate messages or default presentation.
    pub fn metadata(
        &self,
        author: &str,
        space: &str,
        reply: Option<&str>,
        thread: bool,
        ordered_mutations: &[&str],
        index: usize,
    ) -> Result<Option<MessageMedia>, MediaMetadataError> {
        if !self.validate_count(ordered_mutations.len())
            || index >= ordered_mutations.len()
            || ordered_mutations
                .iter()
                .enumerate()
                .any(|(position, mutation)| {
                    mutation.is_empty() || ordered_mutations[..position].contains(mutation)
                })
        {
            return Err(MediaMetadataError::InvalidIntent);
        }
        if self.is_default() {
            return Ok(None);
        }
        let group = if self.grouping == AttachmentGrouping::Grouped {
            let bytes = serde_json::to_vec(&(
                "zoen.media.album.v1",
                author,
                space,
                reply,
                thread,
                ordered_mutations,
            ))
            .map_err(MediaMetadataError::Encoding)?;
            Some(MediaGroup {
                id: format!("album_{}", hex::encode(Sha256::digest(bytes))),
                index: u32::try_from(index).map_err(|_| MediaMetadataError::InvalidIntent)?,
                count: u32::try_from(ordered_mutations.len())
                    .map_err(|_| MediaMetadataError::InvalidIntent)?,
            })
        } else {
            None
        };
        Ok(Some(MessageMedia {
            presentation: self.presentation,
            group,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaGroup {
    pub id: String,
    pub index: u32,
    pub count: u32,
}

impl MediaGroup {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.id.strip_prefix("album_").is_some_and(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }) && (2..=10).contains(&self.count)
            && self.index < self.count
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageMedia {
    pub presentation: AttachmentPresentation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<MediaGroup>,
}

impl MessageMedia {
    pub fn is_valid(&self) -> bool {
        self.group.as_ref().is_none_or(MediaGroup::is_valid)
    }
    /// Forwarding one message keeps presentation but never joins the source album.
    #[must_use]
    pub fn without_group(&self) -> Self {
        Self {
            presentation: self.presentation,
            group: None,
        }
    }
}

/// Disposable ordered membership derived from signed message history. Failed local
/// recovery views never record claims here and can never authorize a later send.
#[derive(Clone, Default)]
pub struct MediaGroupTracker {
    groups: std::collections::HashMap<(String, String, String), GroupCursor>,
}
#[derive(Clone)]
struct GroupCursor {
    next: u32,
    count: u32,
    presentation: AttachmentPresentation,
    reply: Option<crate::ReplyRef>,
}
impl MediaGroupTracker {
    #[must_use]
    pub fn allows(
        &self,
        space: &str,
        author: &str,
        reply: &Option<crate::ReplyRef>,
        forwarded: bool,
        media: &MessageMedia,
    ) -> bool {
        if !media.is_valid() || (forwarded && media.group.is_some()) {
            return false;
        }
        let Some(group) = media.group.as_ref() else {
            return true;
        };
        match self
            .groups
            .get(&(space.to_owned(), author.to_owned(), group.id.clone()))
        {
            None => group.index == 0,
            Some(previous) => {
                previous.next == group.index
                    && previous.count == group.count
                    && previous.presentation == media.presentation
                    && &previous.reply == reply
            }
        }
    }
    /// Call only after the adapter has verified writer/file ownership and applied
    /// the event. Invalid or repeated membership cannot advance the cursor.
    pub fn record(
        &mut self,
        space: &str,
        author: &str,
        reply: &Option<crate::ReplyRef>,
        forwarded: bool,
        media: &MessageMedia,
    ) -> bool {
        if !self.allows(space, author, reply, forwarded, media) {
            return false;
        }
        if let Some(group) = media.group.as_ref() {
            self.groups.insert(
                (space.to_owned(), author.to_owned(), group.id.clone()),
                GroupCursor {
                    next: group.index + 1,
                    count: group.count,
                    presentation: media.presentation,
                    reply: reply.clone(),
                },
            );
        }
        true
    }
    pub fn extend(&mut self, other: &Self) {
        self.groups.extend(other.groups.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn album_membership_is_stable_ordered_and_scope_bound() {
        let options = AttachmentSendOptions {
            grouping: AttachmentGrouping::Grouped,
            ..Default::default()
        };
        let first = options
            .metadata("alice", "room", None, false, &["one", "two"], 0)
            .unwrap()
            .unwrap();
        let second = options
            .metadata("alice", "room", None, false, &["one", "two"], 1)
            .unwrap()
            .unwrap();
        assert!(first.is_valid());
        assert_eq!(
            first.group.as_ref().unwrap().id,
            second.group.as_ref().unwrap().id
        );
        assert_eq!(
            first.group.as_ref().unwrap().id,
            "album_9e65b44aa17a8c112d3a1d59ab5df83d2af5af80a279133ebd946368a5af8139",
            "valid immutable intents retain their original canonical hash bytes"
        );
        assert_eq!(
            (
                second.group.as_ref().unwrap().index,
                second.group.as_ref().unwrap().count
            ),
            (1, 2)
        );
        for changed in [
            options
                .metadata("bob", "room", None, false, &["one", "two"], 0)
                .unwrap(),
            options
                .metadata("alice", "other", None, false, &["one", "two"], 0)
                .unwrap(),
            options
                .metadata("alice", "room", Some("root"), true, &["one", "two"], 0)
                .unwrap(),
            options
                .metadata("alice", "room", None, false, &["two", "one"], 0)
                .unwrap(),
        ] {
            assert_ne!(
                first.group.as_ref().unwrap().id,
                changed.unwrap().group.unwrap().id
            );
        }
        assert!(!options.validate_count(1));
        assert!(!options.validate_count(11));
        assert!(AttachmentSendOptions::default()
            .metadata("alice", "room", None, false, &["one"], 0)
            .unwrap()
            .is_none());
    }

    #[test]
    fn invalid_album_intent_is_an_error_instead_of_default_metadata() {
        let grouped = AttachmentSendOptions {
            grouping: AttachmentGrouping::Grouped,
            ..Default::default()
        };
        for (mutations, index) in [
            (vec![], 0),
            (vec!["one"], 0),
            (vec!["one", "two"], 2),
            (vec!["one", "two"], usize::MAX),
            (vec!["same", "same"], 0),
            (vec!["", "two"], 0),
            (
                vec!["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k"],
                0,
            ),
        ] {
            assert!(matches!(
                grouped.metadata("alice", "room", None, false, &mutations, index),
                Err(MediaMetadataError::InvalidIntent)
            ));
        }
        assert!(matches!(
            AttachmentSendOptions::default().metadata("alice", "room", None, false, &[], 0),
            Err(MediaMetadataError::InvalidIntent)
        ));
        let ten = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
        let last = grouped
            .metadata("alice", "room", None, false, &ten, 9)
            .unwrap()
            .unwrap();
        assert!(last.is_valid());
        assert_eq!(last.group.unwrap().count, 10);
        let single_file = AttachmentSendOptions {
            presentation: AttachmentPresentation::File,
            ..Default::default()
        }
        .metadata("alice", "room", None, false, &["one"], 0)
        .unwrap()
        .unwrap();
        assert!(single_file.is_valid());
        assert!(single_file.group.is_none());
    }
}

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

impl AttachmentSendOptions {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate_count(&self, count: usize) -> bool {
        (1..=10).contains(&count) && (self.grouping != AttachmentGrouping::Grouped || count >= 2)
    }

    /// The sender derives the album id from the entire ordered immutable intent.
    /// Callers validate the count and uniqueness before reaching this helper.
    pub fn metadata(
        &self,
        author: &str,
        space: &str,
        reply: Option<&str>,
        thread: bool,
        ordered_mutations: &[&str],
        index: usize,
    ) -> Option<MessageMedia> {
        if self.is_default() {
            return None;
        }
        let group = (self.grouping == AttachmentGrouping::Grouped).then(|| {
            let bytes = serde_json::to_vec(&(
                "zoen.media.album.v1",
                author,
                space,
                reply,
                thread,
                ordered_mutations,
            ))
            .expect("serializing strings cannot fail");
            MediaGroup {
                id: format!("album_{}", hex::encode(Sha256::digest(bytes))),
                index: index as u32,
                count: ordered_mutations.len() as u32,
            }
        });
        Some(MessageMedia {
            presentation: self.presentation,
            group,
        })
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
    pub fn without_group(&self) -> Self {
        Self {
            presentation: self.presentation,
            group: None,
        }
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
            .unwrap();
        let second = options
            .metadata("alice", "room", None, false, &["one", "two"], 1)
            .unwrap();
        assert!(first.is_valid());
        assert_eq!(
            first.group.as_ref().unwrap().id,
            second.group.as_ref().unwrap().id
        );
        assert_eq!(
            (
                second.group.as_ref().unwrap().index,
                second.group.as_ref().unwrap().count
            ),
            (1, 2)
        );
        for changed in [
            options.metadata("bob", "room", None, false, &["one", "two"], 0),
            options.metadata("alice", "other", None, false, &["one", "two"], 0),
            options.metadata("alice", "room", Some("root"), true, &["one", "two"], 0),
            options.metadata("alice", "room", None, false, &["two", "one"], 0),
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
            .is_none());
    }
}

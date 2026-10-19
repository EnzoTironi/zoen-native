//! Changes to a message after it was posted: edit, delete for everyone, react, unreact.
//!
//! One small signed event, `EventBody::MessageAnnotated`, names its target by the timeline
//! entry id and says what changed. Nothing is rewritten: the original stays in the log and
//! every reader folds the annotations over it. Readers enforce who may do what, because an
//! end-to-end relay cannot read the event: only the message's author can edit or delete it;
//! anyone in the Space can react. Older readers keep the bytes as `Unsupported` and simply
//! show the original message.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Annotation {
    /// New text for the whole message (author only). The latest edit wins.
    Edit {
        text: String,
    },
    /// Delete for everyone (author only). Final: later edits are ignored.
    Delete,
    /// One reaction per person per message (WhatsApp): a new one replaces the old.
    React {
        emoji: String,
    },
    Unreact,
}

impl Annotation {
    /// Whether only the message's author may write this.
    pub fn author_only(&self) -> bool {
        matches!(self, Annotation::Edit { .. } | Annotation::Delete)
    }
}

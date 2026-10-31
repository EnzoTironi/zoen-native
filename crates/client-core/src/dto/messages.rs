//! Conversation timeline, inline replies, media and delivery projections.
use super::{BackgroundDto, ItemCard, MessageMediaDto, Persona};

#[derive(Debug, Clone, PartialEq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[expect(
    clippy::large_enum_variant,
    reason = "uniffi enums cross the FFI by value; a Box isn't representable"
)]
pub enum EntryKind {
    Message {
        text: String,
        card: Option<ItemCard>,
    },
    ItemEdited {
        item_id: String,
        title: String,
        version: u32,
        note: String,
        is_undo: bool,
    },
    Request {
        request_id: String,
        title: String,
        approved: Option<bool>,
    },
    System {
        text: String,
    },
    /// The chat background changed (a typed log event, not a text marker).
    Background {
        background: BackgroundDto,
    },
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct TimelineEntry {
    /// Stable logical event identity of the currently visible source text.
    #[uniffi(default = None)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub text_revision: Option<String>,
    pub id: String,
    pub seq: u64,
    pub author: Persona,
    pub at_ms: i64,
    pub kind: EntryKind,
    /// Signed attachment presentation and explicit album membership. Legacy messages omit it.
    #[uniffi(default = None)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub media: Option<MessageMediaDto>,
    /// Where this entry is on its way to the others.
    pub delivery: Delivery,
    /// Inline reply: a quote of the message this one answers.
    #[uniffi(default = None)]
    pub reply_to: Option<ReplyQuote>,
    /// Thread reply: the root message's id. The main timeline leaves these out and shows
    /// `thread_replies` under the root instead.
    #[uniffi(default = None)]
    pub in_thread: Option<String>,
    /// On a thread root: how many replies its thread has.
    #[uniffi(default = 0)]
    pub thread_replies: u32,
    /// The author changed the text after posting.
    #[uniffi(default = false)]
    pub edited: bool,
    /// Deleted for everyone: the text is gone; apps show "This message was deleted".
    #[uniffi(default = false)]
    pub deleted: bool,
    #[uniffi(default = false)]
    pub forwarded: bool,
    /// Forwarder-signed original display name; not a profile link or proof of origin.
    #[uniffi(default = None)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub forwarded_author: Option<String>,
    /// Grouped by emoji, in the order they first appeared.
    #[uniffi(default = [])]
    pub reactions: Vec<ReactionDto>,
    /// A voice note: play `item_id`'s current bytes (`file_bytes`).
    #[uniffi(default = None)]
    pub voice: Option<VoiceNoteDto>,
}

/// A voice note in the chat. The audio is an end-to-end encrypted file Item; until its
/// pieces arrive `file_bytes` returns `None` and the player waits.
#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct VoiceNoteDto {
    pub item_id: String,
    /// The current signed audio hash, used to invalidate loaded players after edits.
    pub sha256: String,
    pub mime: String,
    /// 0 when the sender didn't say (players read the length from the audio).
    pub ms: u64,
    /// Waveform bars, 0…1 (empty when the sender didn't say).
    pub levels: Vec<f32>,
    pub transcript: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ReactionDto {
    pub emoji: String,
    pub people: Vec<Persona>,
    /// I am one of them (tapping the chip removes mine).
    pub mine: bool,
}

/// What an inline reply shows of the message it answers.
#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ReplyQuote {
    #[uniffi(default = None)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub origin: Option<crate::replies::external::ReplyOrigin>,
    #[uniffi(default = None)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub selection: Option<crate::ReplySelection>,
    pub id: String,
    pub author: Persona,
    pub text: String,
}

/// `Local`: a Space that lives only on this device. `Sending`: signed and queued for the
/// relay (offline-safe). `Sent`: the relay ordered it (✓). `Delivered`: every other member
/// has it on a device (✓✓). `Read`: every other member read it (blue ✓✓). `Failed`: the
/// relay refused it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum Delivery {
    Local,
    Sending,
    Sent,
    Delivered,
    Read,
    Failed,
}

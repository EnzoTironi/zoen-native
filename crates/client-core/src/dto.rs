//! Tipos expostos ao Swift (`UniFFI`). São "visões" da projeção, nunca a fonte da verdade.

mod agents;
mod apps;
mod community;
pub use agents::*;
pub use apps::*;
pub use community::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum PersonaKind {
    Person,
    Agent,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct Persona {
    pub id: String,
    pub kind: PersonaKind,
    pub name: String,
    pub handle: String,
    pub initials: String,
    pub tint_hex: String,
    pub glyph: Option<String>,
    pub bio: String,
    pub owner_id: Option<String>,
    pub owner_name: Option<String>,
    pub owner_tint_hex: Option<String>,
    pub photo_sha256: Option<String>,
    pub photo_ready: bool,
    pub photo_mime: Option<String>,
    pub is_me: bool,
    /// O agente é meu (eu pago, eu configuro).
    pub is_mine: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum SpaceKindDto {
    Direct,
    Group,
    Community,
    #[serde(rename = "saved")]
    Saved,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MemberRoleDto {
    pub identity_id: String,
    /// owner, admin, member, reader
    pub role: String,
}

/// An end-to-end chat's MLS group as this device has it. Members compare `digest` for
/// the same `epoch` to know they share one group (ADR 0026).
#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct GroupKeysDto {
    pub epoch: u64,
    pub digest: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum PrivacyDto {
    EndToEnd,
    Closed,
    Public,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    uniffi::Enum,
    serde::Serialize,
    serde::Deserialize,
)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum TrustLevelDto {
    Listen,
    Suggest,
    Act,
    Autonomous,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct SpaceSummary {
    pub id: String,
    pub title: String,
    pub kind: SpaceKindDto,
    pub privacy: PrivacyDto,
    /// Current projected write permission, shared with the original Core writer.
    #[serde(default)]
    #[uniffi(default = false)]
    pub can_write: bool,
    pub members: Vec<Persona>,
    /// Para DMs: a outra ponta (pessoa ou agente).
    pub counterpart: Option<Persona>,
    pub last_preview: String,
    pub last_author: Option<Persona>,
    pub last_at_ms: i64,
    pub unread: u32,
    /// Local manual reminder; does not alter the actual unread-message count.
    #[serde(default)]
    #[uniffi(default = false)]
    #[cfg_attr(feature = "client-types", ts(optional = nullable))]
    pub manual_unread: bool,
    /// Ticks for the last message when I wrote it (the chat list shows them).
    #[uniffi(default = None)]
    pub last_delivery: Option<Delivery>,
    /// My prefs for this chat, synced across my devices.
    #[uniffi(default = false)]
    pub pinned: bool,
    #[uniffi(default = false)]
    pub archived: bool,
    #[uniffi(default = false)]
    pub muted: bool,
    #[uniffi(default = None)]
    pub muted_until_ms: Option<i64>,
    pub pending_requests: u32,
    pub event_count: u64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ItemCard {
    pub item_id: String,
    pub title: String,
    pub summary: String,
    pub kind_label: String,
    /// Stable kind id for code paths ("plan", "task", "note", "app"); `kind_label` is for people.
    pub kind_id: String,
    pub version: u32,
    pub total_cents: Option<i64>,
    pub budget_cents: Option<i64>,
    pub line_count: u32,
    pub done_count: u32,
    /// Mini-app (MCP App), quando o Item é um.
    pub app: Option<AppStateDto>,
}

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

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct PlanLineDto {
    pub id: String,
    pub text: String,
    pub cost_cents: i64,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct PlanSectionDto {
    pub title: String,
    pub lines: Vec<PlanLineDto>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct PlanDto {
    pub title: String,
    pub summary: String,
    pub budget_cents: Option<i64>,
    pub sections: Vec<PlanSectionDto>,
    /// Calculado pelo núcleo (ignorado na entrada).
    pub total_cents: i64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct VersionDto {
    pub number: u32,
    pub author: Persona,
    pub at_ms: i64,
    pub note: String,
    pub is_undo: bool,
    pub total_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ItemDetail {
    pub id: String,
    pub space_id: String,
    pub space_title: String,
    pub kind_label: String,
    /// Stable kind id for code paths ("plan", "task", "note", "app"); `kind_label` is for people.
    pub kind_id: String,
    pub title: String,
    pub origin: String,
    pub created_by: Persona,
    pub plan: Option<PlanDto>,
    pub text: Option<String>,
    pub version: u32,
    pub versions: Vec<VersionDto>,
    /// Pedidos de agentes ligados a linhas deste Item (linha → status).
    pub linked_requests: Vec<AgentRequestDto>,
    pub app: Option<AppStateDto>,
    /// Folder path inside the Space for pages and files ("" at the top).
    pub path: String,
    pub file: Option<crate::files::FileDto>,
}

// ───────────────────────────── MCP Apps (mini-apps) ─────────────────────────────

/// Uma ferramenta de um mini-app, no formato do MCP (`Tool` + `_meta.ui`).
#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct UndoToken {
    pub item_id: String,
    /// Versão para a qual "Desfazer" volta.
    pub restore_version: u32,
    /// Admitted semantic binding after this mutation.
    pub observed_hash: String,
    /// Item version this mutation created.
    pub result_version: u32,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct EditOutcome {
    pub item: ItemDetail,
    pub undo: UndoToken,
    /// Reação do agente (regra determinística do núcleo, não IA).
    pub reaction: Option<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct LogReport {
    pub space_id: String,
    pub space_title: String,
    pub events: u64,
    pub head_hash: String,
    pub valid: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct LogEventDto {
    pub seq: u64,
    pub label: String,
    pub author: Persona,
    pub at_ms: i64,
    pub hash: String,
    pub prev: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct SearchHit {
    pub space_id: String,
    pub space_title: String,
    pub item_id: Option<String>,
    pub title: String,
    pub snippet: String,
    pub author: Persona,
    pub at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct Mention {
    pub space_id: String,
    pub space_title: String,
    pub entry: TimelineEntry,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct CoreStats {
    pub spaces: u32,
    pub events: u64,
    pub items: u32,
    pub identities: u32,
    pub all_logs_valid: bool,
    pub db_path: String,
}

/// Uma ação de um agente, lida do log assinado (tela "Atividade do agente").
#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AgentActivityDto {
    pub label: String,
    pub detail: String,
    pub space_id: String,
    pub space_title: String,
    pub at_ms: i64,
    /// Custo de IA registrado logo depois desta ação (`UsageRecorded`), se houver.
    pub cost_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MediaRefDto {
    pub sha256: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
}

/// A chat background (see `types::BackgroundSpec`).
#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct BackgroundDto {
    pub style: String,
    pub media: Option<MediaRefDto>,
    pub zoom_pm: u32,
    pub offset_x_pm: i32,
    pub offset_y_pm: i32,
    pub dim_pm: Option<u32>,
    pub blur_pm: u32,
    pub appearance: String,
}

impl From<&types::BackgroundSpec> for BackgroundDto {
    fn from(b: &types::BackgroundSpec) -> Self {
        BackgroundDto {
            style: b.style.clone(),
            media: b.media.as_ref().map(|m| MediaRefDto {
                sha256: m.sha256.clone(),
                mime: m.mime.clone(),
                width: m.width,
                height: m.height,
                bytes: m.bytes,
            }),
            zoom_pm: b.zoom_pm,
            offset_x_pm: b.offset_x_pm,
            offset_y_pm: b.offset_y_pm,
            dim_pm: b.dim_pm,
            blur_pm: b.blur_pm,
            appearance: b.appearance.clone(),
        }
    }
}

impl From<&BackgroundDto> for types::BackgroundSpec {
    fn from(b: &BackgroundDto) -> Self {
        types::BackgroundSpec {
            style: b.style.clone(),
            media: b.media.as_ref().map(|m| types::MediaRef {
                sha256: m.sha256.clone(),
                mime: m.mime.clone(),
                width: m.width,
                height: m.height,
                bytes: m.bytes,
                key: None,
                blob: None,
            }),
            zoom_pm: b.zoom_pm.clamp(1000, 5000),
            offset_x_pm: b.offset_x_pm.clamp(-1000, 1000),
            offset_y_pm: b.offset_y_pm.clamp(-1000, 1000),
            dim_pm: b.dim_pm.map(|d| d.min(900)),
            blur_pm: b.blur_pm.min(1000),
            appearance: b.appearance.clone(),
        }
    }
}

/// One universal-search hit. `kind`: "message", "person", "agent", "space", "item" or
/// "app". Snippets mark the matched text with `[[` and `]]`.
#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct UniversalHit {
    pub kind: String,
    pub ref_id: String,
    pub space_id: Option<String>,
    pub space_title: Option<String>,
    pub title: String,
    pub title_snippet: String,
    pub snippet: String,
    pub at_ms: i64,
    pub persona: Option<Persona>,
}

/// A bounded conversation-scoped message page. Restart after a mutation makes a
/// cursor stale; `ref_id` on each hit is the exact original message ID.
#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MessageSearchPage {
    pub hits: Vec<UniversalHit>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct UniversalResults {
    pub hits: Vec<UniversalHit>,
    /// Query time in microseconds (index refresh included).
    pub took_us: u64,
}

/// Presentation does not change a file's real MIME, hash or encrypted bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AttachmentPresentationDto {
    Media,
    File,
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MediaGroupDto {
    pub id: String,
    pub index: u32,
    pub count: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MessageMediaDto {
    pub presentation: AttachmentPresentationDto,
    pub group: Option<MediaGroupDto>,
}
impl From<&types::MessageMedia> for MessageMediaDto {
    fn from(value: &types::MessageMedia) -> Self {
        Self {
            presentation: match value.presentation {
                types::AttachmentPresentation::Media => AttachmentPresentationDto::Media,
                types::AttachmentPresentation::File => AttachmentPresentationDto::File,
            },
            group: value.group.as_ref().map(|group| MediaGroupDto {
                id: group.id.clone(),
                index: group.index,
                count: group.count,
            }),
        }
    }
}

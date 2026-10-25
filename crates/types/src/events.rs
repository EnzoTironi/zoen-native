//! Signed event envelopes and the Space log substrate.

use crate::{
    AgentRequest, Annotation, Grant, GrantId, IdentityId, ItemContent, ItemId, ItemKind,
    MessageMedia, Privacy, ReplyRef, RequestId, RequestResolution, Role, SpaceId, SpaceKind,
};
use serde::{Deserialize, Serialize};

// ───────────────────────────── Evento (substrato) ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum EventBody {
    SpaceCreated {
        title: String,
        kind: SpaceKind,
        privacy: Privacy,
    },
    MemberAdded {
        identity: IdentityId,
        role: Role,
    },
    MemberRemoved {
        identity: IdentityId,
    },
    MessagePosted {
        message: ItemId,
        text: String,
        attaches: Option<ItemId>,
        /// Signed per-message presentation and explicit album membership. Omitted for legacy sends.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media: Option<MessageMedia>,
        /// Inline reply or thread reply (see [`crate::reply`]). Absent on plain messages, so their
        /// bytes are exactly what older peers expect.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply: Option<ReplyRef>,
        /// Forwarded from another chat (shown as "Forwarded"). Absent when false, so plain
        /// messages keep their exact bytes.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        forwarded: bool,
    },
    /// A scoped App request uses its own discriminator: older consumers retain
    /// these signed bytes as Unsupported, never as an unrestricted chat prompt.
    AppEditRequested {
        message: ItemId,
        text: String,
        reply: ReplyRef,
    },
    /// A new reader receives only this explicitly shared current App state.
    /// Older consumers retain the whole variant as Unsupported, including grant.
    AppCurrentShared {
        share: Box<crate::AppCurrentShare>,
    },
    /// Closed v1 signaling; carried inside MLS for an encrypted person-to-person DM.
    CallV1 {
        signal: crate::calls::CallSignal,
    },
    /// Authenticated SDP/ICE for the two accepted devices, never clear relay ephemera.
    CallTransportV1 {
        signal: crate::call_transport::CallTransportSignal,
    },
    /// How I keep one of my chats (pinned, archived, muted). Lives only in my Personal
    /// Space, so it syncs across my own devices and nobody else ever sees it. The whole
    /// record is restated each time: the latest one wins.
    ChatPrefsSet {
        space: SpaceId,
        prefs: ChatPrefs,
    },
    /// An edit, delete for everyone or reaction on an earlier message (see [`crate::annotation`]).
    MessageAnnotated {
        target: String,
        note: Annotation,
    },
    ItemCreated {
        item: ItemId,
        kind: ItemKind,
        content: ItemContent,
        origin: String,
    },
    ItemVersioned {
        item: ItemId,
        content: ItemContent,
        note: String,
    },
    /// Desfazer/restaurar nunca apaga: cria uma versão nova igual a uma anterior.
    ItemReverted {
        item: ItemId,
        to_version: u32,
        note: String,
    },
    GrantIssued {
        grant: Grant,
    },
    GrantRevoked {
        grant: GrantId,
    },
    RequestOpened {
        request: AgentRequest,
    },
    /// The owner asks for preparation of another review of one frozen action.
    /// This event grants nothing and never changes the previous decision.
    ToolReviewRequested {
        request: RequestId,
        content_hash: String,
    },
    RequestResolved {
        request: RequestId,
        approved: bool,
        content_hash: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolution: Option<RequestResolution>,
    },
    /// Explicit read-only discovery, bound to one canonical pending image review.
    OwnerPolicyOptionsRequested {
        agent: IdentityId,
        signed: crate::owner_policy_context::SignedOwnerPolicyInquiry,
    },
    OwnerPolicyOptionsRefused {
        source: String,
        inquiry_digest: String,
    },
    OwnerPolicyOptionsReported {
        source: String,
        options: crate::owner_policy_context::OwnerPolicyOptions,
    },
    /// Dedicated owner control, never detected from ordinary message text.
    OwnerPolicySubmitted {
        agent: IdentityId,
        signed: crate::owner_budget::SignedOwnerPolicy,
    },
    /// Authenticated host result for the exact original policy, not a new budget.
    OwnerPolicyReported {
        source: String,
        policy_digest: String,
        outcome: OwnerPolicyOutcome,
    },
    /// Uso de IA de um agente (debitado do orçamento do dono).
    UsageRecorded {
        agent: IdentityId,
        cents: i64,
        what: String,
    },
    /// The chat's background changed (shared with everyone in the chat). Photos travel as a
    /// content-addressed attachment: the event carries only its hash and framing.
    BackgroundSet {
        background: BackgroundSpec,
    },
    /// The author's profile key, sealed separately to each recipient's agreement key
    /// (ADR 0016). `version` is the first profile version the key opens. Only the named
    /// recipient can open a share; from M2 on the whole event rides inside MLS.
    ProfileKeyShared {
        version: u64,
        shares: Vec<ProfileKeyShare>,
    },
    /// A relay-readable Space becomes end-to-end from this entry on (ADR 0027). There is
    /// no event for the other direction: a Space's privacy only ever goes up.
    SpaceEncrypted,
    /// A member's statement about an end-to-end Space (ADR 0026): having applied the log
    /// through `upto`, its MLS group is at `epoch`, and `digest` is SHA-256 over a tag, the
    /// group id, the epoch and the epoch authenticator. Only members of that exact group can
    /// compute it, so members who were shown different groups (a forked relay) disagree in
    /// public. The relay checks `upto` is on its chain and keeps the newest per member.
    Checkpoint {
        upto: Seen,
        epoch: u64,
        digest: String,
    },
    /// The author's `device` needs a fresh leaf in this end-to-end Space's group from here
    /// (ADR 0026, ADR 0045): a device just linked, or one back after the pruning ceiling let
    /// history it never fetched go. Whoever may commit for it adds it (taking an old leaf
    /// out first); the relay holds pruning for it from this entry until it checkpoints.
    DeviceJoining {
        device: String,
    },
    /// An entry of an end-to-end Space this device keeps as the relay ordered it: an MLS
    /// handshake (`Commit`, `Welcome`), or ciphertext from before it joined. Derived from
    /// the signed outer bytes, never signed as a body itself.
    Sealed {
        kind: String,
    },
    /// A kind this build doesn't know yet (a newer client wrote it). The signed bytes are
    /// kept verbatim, so the event still verifies, syncs and chains; it just isn't shown.
    Unsupported {
        kind: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum OwnerPolicyOutcome {
    Installed,
    Refused,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatPrefs {
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub archived: bool,
    /// Muted until this time (ms since epoch); `i64::MAX` = always. Absent = not muted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted_until_ms: Option<i64>,
}

/// One recipient's copy of a profile key: X25519 with a fresh ephemeral key, then
/// XChaCha20-Poly1305 (hex fields).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProfileKeyShare {
    pub to: IdentityId,
    pub ephemeral: String,
    pub sealed: String,
}

/// A content-addressed attachment (sha256 of the bytes, lowercase hex).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MediaRef {
    pub sha256: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    /// In relay-synced chats: the key (hex, 32 bytes) that opens the encrypted copy on the
    /// relay. Carried inside the chat's log, which MLS encrypts from M2 on (ADR 0007).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// sha256 of the encrypted copy: its address in the relay's blob store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

/// A chat background. Numbers are integers in thousandths so the event stays exact
/// (no floats in the signed log).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BackgroundSpec {
    /// `none`, `color:<key>`, `gradient:<key>`, `doodles:<key>`, `builtin:<key>` or `photo`.
    pub style: String,
    /// The photo (style "photo").
    pub media: Option<MediaRef>,
    /// Photo framing: zoom (1000 = fill the screen) and the pan as a fraction of the
    /// overflow on each axis (-1000…1000, 0 = centered).
    pub zoom_pm: u32,
    pub offset_x_pm: i32,
    pub offset_y_pm: i32,
    /// Dim over the photo in thousandths; `None` = automatic (from the photo's luminance).
    pub dim_pm: Option<u32>,
    pub blur_pm: u32,
    /// "auto", "light" or "dark": the chat's appearance over this background.
    pub appearance: String,
}

/// The newest relay-ordered event the author had applied in this Space when signing: a
/// causal link inside the signed bytes. A sequencer can't place the event before it or show
/// the author's words on a different history without breaking the link.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub seq: u64,
    pub hash: String,
}

/// The per-Space chain: whoever sequences (the relay, or this device for local-only
/// Spaces) links each event to the previous one. `wire` is the hash of what actually
/// traveled: the signed content for plaintext events, the sealed envelope for E2EE ones.
#[derive(Serialize, Clone, Debug)]
pub struct ChainLink<'a> {
    pub space: &'a str,
    pub seq: u64,
    pub prev: &'a str,
    pub wire: &'a str,
}

pub const EVENT_FORMAT: u8 = 3;

/// One event. `content` is the exact byte string the author signed (format v3, see
/// `event_log::content`); the other fields are views decoded from it, plus what the
/// sequencer added around it (`seq`, `prev`, `hash`). Nobody re-encodes `content`, so a
/// field a newer client added survives every hop and the signature never depends on the
/// verifier's schema.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub space: SpaceId,
    /// Sequence per Space, assigned by the relay (or by this device for local-only Spaces).
    pub seq: u64,
    /// Chain hash of the previous event (64 zeros at genesis).
    pub prev: String,
    pub author: IdentityId,
    pub at_ms: i64,
    pub body: EventBody,
    /// Chain hash: SHA-256 of [`ChainLink`].
    pub hash: String,
    /// Ed25519 signature (by `device`, or by `author` when `device` is empty) over the
    /// content hash of `content`.
    pub sig: String,
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// The identity's signature over the device key ("this device is me").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen: Option<Seen>,
    #[serde(with = "hex_bytes")]
    pub content: Vec<u8>,
    /// Hash of the sealed envelope when the event traveled end-to-end encrypted.
    /// `None` = it traveled in the clear and `wire` is the content hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealed_wire: Option<String>,
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        hex::decode(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

pub const GENESIS_PREV: &str = "0000000000000000000000000000000000000000000000000000000000000000";

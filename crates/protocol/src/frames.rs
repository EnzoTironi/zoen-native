//! Existing client/server frame and request/reply definitions.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub space: SpaceId,
    /// The first sequence number this device doesn't have yet.
    pub next_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EphemeralKind {
    Typing,
    StoppedTyping,
    /// The agent (or person) is working: "processing", "building", "in_call"…
    Status {
        status: String,
    },
    /// Read up to and including `seq`.
    Read {
        seq: u64,
    },
    /// This person's devices hold everything up to and including `seq`. Like `Read`, one
    /// aggregated watermark per person per Space; the relay keeps the newest and replays it
    /// on sync, so a sender who was offline still sees its ticks.
    Delivered {
        seq: u64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientFrame {
    Hello {
        protocol: u32,
        capabilities: Vec<String>,
        identity: IdentityId,
        device: String,
        cert: String,
    },
    Auth {
        sig: String,
    },
    Req {
        id: u64,
        op: Op,
    },
    Publish {
        env: Envelope,
    },
    Sync {
        cursors: Vec<Cursor>,
        all: bool,
    },
    Ephemeral {
        space: SpaceId,
        kind: EphemeralKind,
    },
    Ping,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// Publishes this identity's profile and claims its handle.
    Register {
        profile: Identity,
    },
    /// Exact handle, or a prefix search when `prefix` is set.
    Lookup {
        handle: String,
        prefix: bool,
    },
    Profiles {
        ids: Vec<IdentityId>,
    },
    CreateInvite {
        space: SpaceId,
        /// When set, this invite opens a chat with the agent instead of joining a Space.
        agent: Option<IdentityId>,
        role: Role,
        max_uses: u32,
        ttl_secs: u64,
    },
    PreviewInvite {
        code: String,
    },
    /// Spends one use of an agent link, atomically against its `max_uses`, and returns
    /// its preview. Space codes are redeemed by joining instead.
    RedeemAgentInvite {
        code: String,
    },
    /// The owner takes an agent's share link down; the next `CreateInvite` mints a new one.
    RevokeAgentInvite {
        agent: IdentityId,
    },
    /// Registers this device's push token ("apns", "apns-sandbox" or "fcm"); an empty
    /// token removes it. The relay only ever sends a content-free wake to it.
    RegisterPush {
        platform: String,
        token: String,
    },
    /// Publishes this identity's X25519 key for receiving profile keys (ADR 0016).
    PublishAgreementKey {
        public: String,
        signed: DeviceSigned,
    },
    AgreementKeys {
        ids: Vec<IdentityId>,
    },
    /// Stores this identity's encrypted profile; `version` must grow.
    PutProfile {
        profile: SealedProfile,
    },
    GetProfiles {
        ids: Vec<IdentityId>,
    },
    /// Publishes MLS key packages for this device (ADR 0026). Each must name this identity
    /// and device; `last_resort` replaces the device's previous one.
    PublishKeyPackages {
        packages: Vec<Vec<u8>>,
        last_resort: Option<Vec<u8>>,
    },
    /// Takes one key package for each device of each identity, to add them to a group.
    ClaimKeyPackages {
        ids: Vec<IdentityId>,
        /// Absent in legacy clients. Stable across reconnects for one logical claim.
        operation_id: Option<String>,
    },
    /// Leaves a sealed link box for a device being linked (ADR 0045), under `id` = SHA-256
    /// of the link secret in its QR code. Only the new device can open it. The identity
    /// box also enrolls its certified device; history manifests carry no device.
    DeliverLink {
        id: String,
        sealed: Vec<u8>,
        device: Option<DeviceCertificate>,
    },
    /// Takes the link box under `id`, if it is there yet. The one op a device being linked
    /// (signed in as itself, not registered) may use besides registering.
    FetchLink {
        id: String,
    },
    /// This identity's devices, unlinked ones included.
    Devices,
    /// Unlinks one of this identity's devices: it can't sign in again and its key packages
    /// go. Its leaves leave the groups through commits by the other devices.
    Unlink {
        device: String,
    },
    /// Sends sealed bytes to another linked device of this identity that is online (ADR
    /// 0043: history pages). Nothing is stored; an offline device just doesn't get it.
    SendDevice {
        to: String,
        sealed: Vec<u8>,
    },
    /// Agent citizens (migration 0028): the runtime's own state and cursors, or an
    /// owner's control over an agent it answers for.
    Agent(AgentRequest),
    /// Relay-only transport allocation; no call consent or media permission.
    CallTurn {
        space: SpaceId,
        peer: IdentityId,
    },
}

/// What an agent runtime or its owner asks the relay about an agent citizen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentRequest {
    /// The signed-in agent: its status, current definition and per-Space cursors.
    State,
    /// The signed-in agent handled its Space through `seq` (inclusive).
    Advance { space: SpaceId, seq: u64 },
    /// A new definition version. The owner may always; the agent only for its first.
    Define {
        agent: IdentityId,
        version: u32,
        kind: String,
        definition: Vec<u8>,
    },
    /// Owner only: one of your agents' status ("active", "paused" or "revoked").
    Status { agent: IdentityId },
    /// Owner only.
    Control {
        agent: IdentityId,
        action: AgentAction,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAction {
    Pause,
    Resume,
    /// Final: the agent's runtime device is revoked too.
    Revoke,
}

impl AgentAction {
    pub fn id(self) -> &'static str {
        match self {
            AgentAction::Pause => "pause",
            AgentAction::Resume => "resume",
            AgentAction::Revoke => "revoke",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pause" => Some(AgentAction::Pause),
            "resume" => Some(AgentAction::Resume),
            "revoke" => Some(AgentAction::Revoke),
            _ => None,
        }
    }
}

/// The relay's single source of truth for an agent runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentStateRecord {
    /// "active", "paused" or "revoked".
    pub status: String,
    /// "Creator" or "User"; empty until the first definition.
    pub kind: String,
    pub definition_version: u32,
    pub definition: Vec<u8>,
    /// Last handled seq per Space.
    pub cursors: Vec<(SpaceId, u64)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceCertificate {
    pub device: String,
    pub cert: String,
}

impl Op {
    /// A stable name for metrics and traces; carries none of the op's data.
    pub fn name(&self) -> &'static str {
        match self {
            Op::Register { .. } => "register",
            Op::Lookup { .. } => "lookup",
            Op::Profiles { .. } => "profiles",
            Op::CreateInvite { .. } => "create_invite",
            Op::PreviewInvite { .. } => "preview_invite",
            Op::RedeemAgentInvite { .. } => "redeem_agent_invite",
            Op::RevokeAgentInvite { .. } => "revoke_agent_invite",
            Op::RegisterPush { .. } => "register_push",
            Op::PublishAgreementKey { .. } => "publish_agreement_key",
            Op::AgreementKeys { .. } => "agreement_keys",
            Op::PutProfile { .. } => "put_profile",
            Op::GetProfiles { .. } => "get_profiles",
            Op::PublishKeyPackages { .. } => "publish_key_packages",
            Op::ClaimKeyPackages { .. } => "claim_key_packages",
            Op::DeliverLink { .. } => "deliver_link",
            Op::FetchLink { .. } => "fetch_link",
            Op::Devices => "devices",
            Op::Unlink { .. } => "unlink",
            Op::SendDevice { .. } => "send_device",
            Op::Agent(AgentRequest::State) => "agent_state",
            Op::Agent(AgentRequest::Advance { .. }) => "agent_advance",
            Op::Agent(AgentRequest::Define { .. }) => "agent_define",
            Op::Agent(AgentRequest::Control { .. }) => "agent_control",
            Op::Agent(AgentRequest::Status { .. }) => "agent_status",
            Op::CallTurn { .. } => "call_turn",
        }
    }
}

/// An identity's agreement key as the directory serves it, with the device signature
/// that lets anyone check it came from that identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgreementKeyRecord {
    pub identity: IdentityId,
    pub public: String,
    pub signed: DeviceSigned,
}

/// An encrypted profile: what the relay stores and serves. Only holders of the owner's
/// profile key can read `ciphertext`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedProfile {
    pub identity: IdentityId,
    pub version: u64,
    pub ciphertext: Vec<u8>,
    pub signed: DeviceSigned,
}

/// The typed answer to each [`Op`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// Register: the profile as stored (handle normalized).
    Registered(Identity),
    /// Lookup and Profiles.
    Profiles(Vec<Identity>),
    Invite(InviteCreated),
    Preview(InvitePreview),
    Done,
    AgreementKeys(Vec<AgreementKeyRecord>),
    SealedProfiles(Vec<SealedProfile>),
    KeyPackages(Vec<KeyPackageRecord>),
    /// FetchLink: the sealed box, `None` while nobody left one.
    Link(Option<Vec<u8>>),
    Devices(Vec<DeviceRecord>),
    AgentState(AgentStateRecord),
    CallTurn(crate::turn::TurnCredentials),
}

/// One of an identity's devices, as the directory has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceRecord {
    pub device: String,
    pub revoked: bool,
}

/// One device's MLS key package, as claimed. Members re-check the leaf inside; the relay's
/// check only keeps junk out of the table.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KeyPackageRecord {
    pub identity: IdentityId,
    pub device: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvitePreview {
    pub space: SpaceId,
    pub role: Role,
    pub title: String,
    pub members: u32,
    pub inviter: Option<Identity>,
    /// `space` or `agent`.
    pub kind: String,
    /// Present when `kind` is `agent`.
    pub agent: Option<Box<Identity>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteCreated {
    pub code: String,
    pub expires_at_ms: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    Other,
    /// The client's protocol is older than [`MIN_PROTOCOL_VERSION`]: update the app.
    UpgradeRequired,
    /// Login or certificate failed: retrying won't help.
    Unauthorized,
    /// A dependency is down: retry with backoff.
    Unavailable,
    /// Too much, too fast: retry after the delay in the message ([`retry_hint`]).
    RateLimited,
}

/// The relay's words for every rate-limit refusal; [`retry_hint`] reads the delay back.
pub fn slow_down(retry: std::time::Duration) -> String {
    format!("slow down, retry in {:.1} s", retry.as_secs_f64().max(0.1))
}

pub fn retry_hint(message: &str) -> Option<std::time::Duration> {
    let secs = message.split("retry in ").nth(1)?.split(' ').next()?;
    let secs: f64 = secs
        .parse()
        .ok()
        .filter(|s: &f64| s.is_finite() && *s >= 0.0)?;
    Some(std::time::Duration::from_secs_f64(secs.min(3_600.0)))
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerFrame {
    Challenge {
        nonce: String,
        relay: String,
        protocol: u32,
        capabilities: Vec<String>,
        /// Optional PG clock sample; required before relying on claim receipts.
        server_time_ms: Option<i64>,
    },
    Ready {
        identity: IdentityId,
        registered: bool,
    },
    Res {
        id: u64,
        result: Result<Reply, String>,
    },
    Event {
        ev: Sequenced,
    },
    Accepted {
        space: SpaceId,
        client_id: String,
        seq: u64,
    },
    Rejected {
        space: SpaceId,
        client_id: String,
        reason: String,
        permanent: bool,
    },
    Ephemeral {
        space: SpaceId,
        from: IdentityId,
        kind: EphemeralKind,
    },
    Presence {
        identity: IdentityId,
        online: bool,
    },
    /// Someone you share a Space with stored a new profile version.
    ProfileChanged {
        identity: IdentityId,
        version: u64,
    },
    /// One of your devices is running out of single-use MLS key packages (ADR 0026): it
    /// should publish more. Sent at login and after a claim leaves it under
    /// [`KEY_PACKAGES_LOW`]. Other devices of the identity ignore it.
    KeyPackagesLow {
        device: String,
        remaining: u32,
    },
    /// Sealed bytes from another device of yours (`Op::SendDevice`); the others ignore it.
    DeviceMessage {
        from: String,
        to: String,
        sealed: Vec<u8>,
    },
    /// You were added to a Space: sync it from the start.
    Joined {
        space: SpaceId,
    },
    SyncDone,
    Pong {
        server_time_ms: Option<i64>,
    },
    Error {
        code: ErrorCode,
        message: String,
    },
}

impl ServerFrame {
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        ServerFrame::Error {
            code,
            message: message.into(),
        }
    }
}

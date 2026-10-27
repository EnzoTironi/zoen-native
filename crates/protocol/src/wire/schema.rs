use super::Message;

#[derive(Clone, PartialEq, Message)]
pub struct PbClientFrame {
    #[prost(oneof = "pb_client_frame::F", tags = "1, 2, 3, 4, 5, 6, 7")]
    pub f: Option<pb_client_frame::F>,
}

pub mod pb_client_frame {
    use super::{PbEmpty, PbEnvelope, PbEphemeral, PbHello, PbReq, PbSync};

    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum F {
        #[prost(message, tag = "1")]
        Hello(PbHello),
        #[prost(string, tag = "2")]
        Auth(String),
        #[prost(message, tag = "3")]
        Req(PbReq),
        #[prost(message, tag = "4")]
        Publish(PbEnvelope),
        #[prost(message, tag = "5")]
        Sync(PbSync),
        #[prost(message, tag = "6")]
        Ephemeral(PbEphemeral),
        #[prost(message, tag = "7")]
        Ping(PbEmpty),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct PbEmpty {}

#[derive(Clone, PartialEq, Message)]
pub struct PbPushToken {
    #[prost(string, tag = "1")]
    pub platform: String,
    #[prost(string, tag = "2")]
    pub token: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbHello {
    #[prost(uint32, tag = "1")]
    pub protocol: u32,
    #[prost(string, repeated, tag = "2")]
    pub capabilities: Vec<String>,
    #[prost(string, tag = "3")]
    pub identity: String,
    #[prost(string, tag = "4")]
    pub device: String,
    #[prost(string, tag = "5")]
    pub cert: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbReq {
    #[prost(uint64, tag = "1")]
    pub id: u64,
    #[prost(
        oneof = "pb_req::Op",
        tags = "2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25"
    )]
    pub op: Option<pb_req::Op>,
}

pub mod pb_req {
    use super::{
        PbAgentReq, PbAgreementKey, PbCallTurnRequest, PbClaimKeyPackages, PbCreateInvite,
        PbDeviceMessage, PbEmpty, PbIds, PbLinkBox, PbLookup, PbProfile, PbPublishKeyPackages,
        PbPushToken, PbRuntimeBindingRequest, PbSealedProfile,
    };

    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Op {
        #[prost(message, tag = "2")]
        Register(PbProfile),
        #[prost(message, tag = "3")]
        Lookup(PbLookup),
        #[prost(message, tag = "4")]
        Profiles(PbIds),
        #[prost(message, tag = "5")]
        CreateInvite(PbCreateInvite),
        #[prost(string, tag = "6")]
        PreviewInvite(String),
        #[prost(message, tag = "7")]
        PublishAgreementKey(PbAgreementKey),
        #[prost(message, tag = "8")]
        AgreementKeys(PbIds),
        #[prost(message, tag = "9")]
        PutProfile(PbSealedProfile),
        #[prost(message, tag = "10")]
        GetProfiles(PbIds),
        #[prost(message, tag = "11")]
        PublishKeyPackages(PbPublishKeyPackages),
        #[prost(message, tag = "12")]
        ClaimKeyPackages(PbClaimKeyPackages),
        #[prost(message, tag = "13")]
        DeliverLink(PbLinkBox),
        #[prost(string, tag = "14")]
        FetchLink(String),
        #[prost(message, tag = "15")]
        Devices(PbEmpty),
        #[prost(string, tag = "16")]
        Unlink(String),
        #[prost(message, tag = "17")]
        SendDevice(PbDeviceMessage),
        #[prost(string, tag = "18")]
        RedeemAgentInvite(String),
        #[prost(string, tag = "19")]
        RevokeAgentInvite(String),
        #[prost(message, tag = "20")]
        Agent(PbAgentReq),
        #[prost(message, tag = "21")]
        RegisterPush(PbPushToken),
        #[prost(message, tag = "22")]
        CallTurn(PbCallTurnRequest),
        #[prost(message, tag = "23")]
        RuntimeBinding(PbRuntimeBindingRequest),
        #[prost(message, tag = "24")]
        CompleteRead(crate::complete_reads::Request),
        #[prost(message, tag = "25")]
        EnsureSavedMessages(crate::saved_messages::Genesis),
    }
}

/// Agent runtime operations, legacy owner control, and revision-bound `control_*`.
#[derive(Clone, PartialEq, Message)]
pub struct PbAgentReq {
    #[prost(string, tag = "1")]
    pub op: String,
    #[prost(string, tag = "2")]
    pub agent: String,
    #[prost(string, tag = "3")]
    pub space: String,
    #[prost(uint64, tag = "4")]
    pub seq: u64,
    #[prost(uint32, tag = "5")]
    pub version: u32,
    #[prost(string, tag = "6")]
    pub kind: String,
    #[prost(bytes = "vec", tag = "7")]
    pub definition: Vec<u8>,
    #[prost(string, tag = "8")]
    pub operation: String,
    #[prost(uint64, tag = "9")]
    pub revision: u64,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbAgentCursor {
    #[prost(string, tag = "1")]
    pub space: String,
    #[prost(uint64, tag = "2")]
    pub seq: u64,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbAgentState {
    #[prost(string, tag = "1")]
    pub status: String,
    #[prost(string, tag = "2")]
    pub kind: String,
    #[prost(uint32, tag = "3")]
    pub definition_version: u32,
    #[prost(bytes = "vec", tag = "4")]
    pub definition: Vec<u8>,
    #[prost(message, repeated, tag = "5")]
    pub cursors: Vec<PbAgentCursor>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbDeviceSigned {
    #[prost(string, tag = "1")]
    pub device: String,
    #[prost(string, tag = "2")]
    pub sig: String,
    #[prost(string, tag = "3")]
    pub cert: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbAgreementKey {
    #[prost(string, tag = "1")]
    pub identity: String,
    #[prost(string, tag = "2")]
    pub public: String,
    #[prost(message, optional, tag = "3")]
    pub signed: Option<PbDeviceSigned>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbSealedProfile {
    #[prost(string, tag = "1")]
    pub identity: String,
    #[prost(uint64, tag = "2")]
    pub version: u64,
    #[prost(bytes = "vec", tag = "3")]
    pub ciphertext: Vec<u8>,
    #[prost(message, optional, tag = "4")]
    pub signed: Option<PbDeviceSigned>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbAgreementKeys {
    #[prost(message, repeated, tag = "1")]
    pub keys: Vec<PbAgreementKey>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbSealedProfiles {
    #[prost(message, repeated, tag = "1")]
    pub profiles: Vec<PbSealedProfile>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbPublishKeyPackages {
    #[prost(bytes = "vec", repeated, tag = "1")]
    pub packages: Vec<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub last_resort: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbClaimKeyPackages {
    #[prost(string, repeated, tag = "1")]
    pub ids: Vec<String>,
    #[prost(string, optional, tag = "2")]
    pub operation_id: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbKeyPackage {
    #[prost(string, tag = "1")]
    pub identity: String,
    #[prost(string, tag = "2")]
    pub device: String,
    #[prost(bytes = "vec", tag = "3")]
    pub data: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbKeyPackages {
    #[prost(message, repeated, tag = "1")]
    pub packages: Vec<PbKeyPackage>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbLinkBox {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(bytes = "vec", tag = "2")]
    pub sealed: Vec<u8>,
    #[prost(message, optional, tag = "3")]
    pub device: Option<PbDeviceCertificate>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbDeviceCertificate {
    #[prost(string, tag = "1")]
    pub device: String,
    #[prost(string, tag = "2")]
    pub cert: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbDeviceMessage {
    #[prost(string, tag = "1")]
    pub from: String,
    #[prost(string, tag = "2")]
    pub to: String,
    #[prost(bytes = "vec", tag = "3")]
    pub sealed: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbDevice {
    #[prost(string, tag = "1")]
    pub device: String,
    #[prost(bool, tag = "2")]
    pub revoked: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbDevices {
    #[prost(message, repeated, tag = "1")]
    pub devices: Vec<PbDevice>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbKeyPackagesLow {
    #[prost(string, tag = "1")]
    pub device: String,
    #[prost(uint32, tag = "2")]
    pub remaining: u32,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbProfileChanged {
    #[prost(string, tag = "1")]
    pub identity: String,
    #[prost(uint64, tag = "2")]
    pub version: u64,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbProfile {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(enumeration = "PbIdentityKind", tag = "2")]
    pub kind: i32,
    #[prost(string, tag = "3")]
    pub name: String,
    #[prost(string, tag = "4")]
    pub handle: String,
    #[prost(string, tag = "5")]
    pub tint_hex: String,
    #[prost(string, optional, tag = "6")]
    pub glyph: Option<String>,
    #[prost(string, optional, tag = "7")]
    pub owner: Option<String>,
    #[prost(string, tag = "8")]
    pub bio: String,
    #[prost(message, optional, tag = "9")]
    pub owner_proof: Option<PbAgentOwnerProof>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbAgentOwnerProof {
    #[prost(string, tag = "1")]
    pub device: String,
    #[prost(string, tag = "2")]
    pub cert: String,
    #[prost(string, tag = "3")]
    pub signature: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum PbIdentityKind {
    Person = 0,
    Agent = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum PbRole {
    Member = 0,
    Owner = 1,
    Admin = 2,
    Reader = 3,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbLookup {
    #[prost(string, tag = "1")]
    pub handle: String,
    #[prost(bool, tag = "2")]
    pub prefix: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbIds {
    #[prost(string, repeated, tag = "1")]
    pub ids: Vec<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbCreateInvite {
    #[prost(string, tag = "1")]
    pub space: String,
    #[prost(enumeration = "PbRole", tag = "2")]
    pub role: i32,
    #[prost(uint32, tag = "3")]
    pub max_uses: u32,
    #[prost(uint64, tag = "4")]
    pub ttl_secs: u64,
    /// Agent identity id; empty means a Space invite.
    #[prost(string, optional, tag = "5")]
    pub agent: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbEnvelope {
    #[prost(bytes = "vec", tag = "1")]
    pub content: Vec<u8>,
    #[prost(string, tag = "2")]
    pub sig: String,
    #[prost(string, optional, tag = "3")]
    pub cert: Option<String>,
    #[prost(string, optional, tag = "4")]
    pub invite: Option<String>,
    /// Persisted legacy v3 stub's original signed hash. Never reused for new stubs.
    #[prost(string, optional, tag = "5")]
    pub pruned: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbSync {
    #[prost(message, repeated, tag = "1")]
    pub cursors: Vec<PbCursor>,
    #[prost(bool, tag = "2")]
    pub all: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbCursor {
    #[prost(string, tag = "1")]
    pub space: String,
    #[prost(uint64, tag = "2")]
    pub next_seq: u64,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbEphemeral {
    #[prost(string, tag = "1")]
    pub space: String,
    #[prost(string, tag = "2")]
    pub from: String,
    #[prost(oneof = "pb_ephemeral::Kind", tags = "3, 4, 5, 6, 7")]
    pub kind: Option<pb_ephemeral::Kind>,
}

pub mod pb_ephemeral {
    use super::PbEmpty;

    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Kind {
        #[prost(message, tag = "3")]
        Typing(PbEmpty),
        #[prost(message, tag = "4")]
        StoppedTyping(PbEmpty),
        #[prost(string, tag = "5")]
        Status(String),
        #[prost(uint64, tag = "6")]
        Read(u64),
        #[prost(uint64, tag = "7")]
        Delivered(u64),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct PbServerFrame {
    #[prost(
        oneof = "pb_server_frame::F",
        tags = "1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15"
    )]
    pub f: Option<pb_server_frame::F>,
}

pub mod pb_server_frame {
    use super::{
        PbAccepted, PbChallenge, PbClock, PbDeviceMessage, PbEmpty, PbEphemeral, PbError,
        PbKeyPackagesLow, PbPresence, PbProfileChanged, PbReady, PbRejected, PbRes, PbSequenced,
    };

    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum F {
        #[prost(message, tag = "1")]
        Challenge(PbChallenge),
        #[prost(message, tag = "2")]
        Ready(PbReady),
        #[prost(message, tag = "3")]
        Res(PbRes),
        #[prost(message, tag = "4")]
        Event(PbSequenced),
        #[prost(message, tag = "5")]
        Accepted(PbAccepted),
        #[prost(message, tag = "6")]
        Rejected(PbRejected),
        #[prost(message, tag = "7")]
        Ephemeral(PbEphemeral),
        #[prost(message, tag = "8")]
        Presence(PbPresence),
        #[prost(string, tag = "9")]
        Joined(String),
        #[prost(message, tag = "10")]
        SyncDone(PbEmpty),
        #[prost(message, tag = "11")]
        Pong(PbClock),
        #[prost(message, tag = "12")]
        Error(PbError),
        #[prost(message, tag = "13")]
        ProfileChanged(PbProfileChanged),
        #[prost(message, tag = "14")]
        KeyPackagesLow(PbKeyPackagesLow),
        #[prost(message, tag = "15")]
        DeviceMessage(PbDeviceMessage),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct PbChallenge {
    #[prost(string, tag = "1")]
    pub nonce: String,
    #[prost(string, tag = "2")]
    pub relay: String,
    #[prost(uint32, tag = "3")]
    pub protocol: u32,
    #[prost(string, repeated, tag = "4")]
    pub capabilities: Vec<String>,
    #[prost(int64, optional, tag = "5")]
    pub server_time_ms: Option<i64>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbClock {
    #[prost(int64, optional, tag = "1")]
    pub server_time_ms: Option<i64>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbReady {
    #[prost(string, tag = "1")]
    pub identity: String,
    #[prost(bool, tag = "2")]
    pub registered: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbRes {
    #[prost(uint64, tag = "1")]
    pub id: u64,
    #[prost(
        oneof = "pb_res::R",
        tags = "2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18"
    )]
    pub r: Option<pb_res::R>,
}

pub mod pb_res {
    use super::{
        PbAgentState, PbAgreementKeys, PbCallTurnReply, PbDevices, PbEmpty, PbInviteCreated,
        PbInvitePreview, PbKeyPackages, PbLinkBox, PbProfile, PbProfiles, PbRuntimeBindingProof,
        PbSealedProfiles,
    };

    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum R {
        #[prost(string, tag = "2")]
        Err(String),
        #[prost(message, tag = "3")]
        Registered(PbProfile),
        #[prost(message, tag = "4")]
        Profiles(PbProfiles),
        #[prost(message, tag = "5")]
        Invite(PbInviteCreated),
        #[prost(message, tag = "6")]
        Preview(PbInvitePreview),
        #[prost(message, tag = "7")]
        Done(PbEmpty),
        #[prost(message, tag = "8")]
        AgreementKeys(PbAgreementKeys),
        #[prost(message, tag = "9")]
        SealedProfiles(PbSealedProfiles),
        #[prost(message, tag = "10")]
        KeyPackages(PbKeyPackages),
        #[prost(message, tag = "11")]
        Link(PbLinkBox),
        #[prost(message, tag = "12")]
        Devices(PbDevices),
        #[prost(message, tag = "13")]
        AgentState(PbAgentState),
        #[prost(message, tag = "14")]
        CallTurn(PbCallTurnReply),
        #[prost(message, tag = "15")]
        RuntimeBinding(PbRuntimeBindingProof),
        #[prost(message, tag = "16")]
        CompletePage(crate::complete_reads::Page),
        #[prost(message, tag = "17")]
        AgentControl(crate::agent_control::Record),
        #[prost(message, tag = "18")]
        SavedMessages(crate::saved_messages::Receipt),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct PbRuntimeBindingRequest {
    #[prost(string, tag = "1")]
    pub namespace: String,
    #[prost(bytes = "vec", tag = "2")]
    pub nonce: Vec<u8>,
    #[prost(message, optional, tag = "3")]
    pub managed: Option<PbManagedCheckpoint>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbManagedCheckpoint {
    #[prost(uint64, tag = "1")]
    pub generation: u64,
    #[prost(uint64, tag = "2")]
    pub sequence: u64,
    #[prost(string, tag = "3")]
    pub digest: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbRuntimeBindingProof {
    #[prost(string, tag = "1")]
    pub cell: String,
    #[prost(bytes = "vec", tag = "2")]
    pub tag: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbCallTurnRequest {
    #[prost(string, tag = "1")]
    pub space: String,
    #[prost(string, tag = "2")]
    pub peer: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbCallTurnReply {
    #[prost(string, repeated, tag = "1")]
    pub urls: Vec<String>,
    #[prost(string, tag = "2")]
    pub username: String,
    #[prost(string, tag = "3")]
    pub credential: String,
    #[prost(int64, tag = "4")]
    pub expires_at_ms: i64,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbProfiles {
    #[prost(message, repeated, tag = "1")]
    pub profiles: Vec<PbProfile>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbInviteCreated {
    #[prost(string, tag = "1")]
    pub code: String,
    #[prost(int64, tag = "2")]
    pub expires_at_ms: i64,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbInvitePreview {
    #[prost(string, tag = "1")]
    pub space: String,
    #[prost(enumeration = "PbRole", tag = "2")]
    pub role: i32,
    #[prost(string, tag = "3")]
    pub title: String,
    #[prost(uint32, tag = "4")]
    pub members: u32,
    #[prost(message, optional, tag = "5")]
    pub inviter: Option<PbProfile>,
    /// `space` or `agent`; empty on the wire means space (legacy).
    #[prost(string, tag = "6")]
    pub kind: String,
    #[prost(message, optional, boxed, tag = "7")]
    pub agent: Option<Box<PbProfile>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbSequenced {
    #[prost(uint64, tag = "1")]
    pub seq: u64,
    #[prost(string, tag = "2")]
    pub prev: String,
    #[prost(string, tag = "3")]
    pub hash: String,
    #[prost(message, optional, tag = "4")]
    pub env: Option<PbEnvelope>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbAccepted {
    #[prost(string, tag = "1")]
    pub space: String,
    #[prost(string, tag = "2")]
    pub client_id: String,
    #[prost(uint64, tag = "3")]
    pub seq: u64,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbRejected {
    #[prost(string, tag = "1")]
    pub space: String,
    #[prost(string, tag = "2")]
    pub client_id: String,
    #[prost(string, tag = "3")]
    pub reason: String,
    #[prost(bool, tag = "4")]
    pub permanent: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbPresence {
    #[prost(string, tag = "1")]
    pub identity: String,
    #[prost(bool, tag = "2")]
    pub online: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbError {
    #[prost(enumeration = "PbErrorCode", tag = "1")]
    pub code: i32,
    #[prost(string, tag = "2")]
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum PbErrorCode {
    Other = 0,
    UpgradeRequired = 1,
    Unauthorized = 2,
    Unavailable = 3,
    RateLimited = 4,
}

// ───────────────────────────── mapping ─────────────────────────────

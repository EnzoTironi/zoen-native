//! The protobuf schema of Zoen Sync v2 (`zoen.sync.v2`) and its mapping to the domain
//! frames. The schema lives here, in Rust, so the types and the wire can't drift; field
//! numbers are append-only and never reused. Decoders ignore unknown fields, which is how
//! an older peer tolerates a newer one.

use prost::Message;
use roda_types::{Identity, IdentityKind, Role};

use crate::{
    AgreementKeyRecord, ClientFrame, Cursor, DeviceRecord, DeviceSigned, Envelope, EphemeralKind,
    ErrorCode, InviteCreated, InvitePreview, KeyPackageRecord, Op, Reply, SealedProfile, Sequenced,
    ServerFrame,
};

/// Why a frame didn't decode.
#[derive(Debug, PartialEq, Eq)]
pub struct DecodeError(pub String);

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bad frame: {}", self.0)
    }
}

fn bad(what: &str) -> DecodeError {
    DecodeError(what.to_string())
}

#[derive(Clone, PartialEq, Message)]
pub struct PbClientFrame {
    #[prost(oneof = "pb_client_frame::F", tags = "1, 2, 3, 4, 5, 6, 7")]
    pub f: Option<pb_client_frame::F>,
}

pub mod pb_client_frame {
    use super::*;

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
        tags = "2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17"
    )]
    pub op: Option<pb_req::Op>,
}

pub mod pb_req {
    use super::*;

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
        ClaimKeyPackages(PbIds),
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
    }
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
    #[prost(oneof = "pb_ephemeral::Kind", tags = "3, 4, 5, 6")]
    pub kind: Option<pb_ephemeral::Kind>,
}

pub mod pb_ephemeral {
    use super::*;

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
    use super::*;

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
        Pong(PbEmpty),
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
    #[prost(oneof = "pb_res::R", tags = "2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12")]
    pub r: Option<pb_res::R>,
}

pub mod pb_res {
    use super::*;

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
    }
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

fn signed_to(s: &DeviceSigned) -> Option<PbDeviceSigned> {
    Some(PbDeviceSigned {
        device: s.device.clone(),
        sig: s.sig.clone(),
        cert: s.cert.clone(),
    })
}

fn signed_from(s: Option<PbDeviceSigned>) -> Result<DeviceSigned, DecodeError> {
    let s = s.ok_or_else(|| bad("unsigned statement"))?;
    Ok(DeviceSigned {
        device: s.device,
        sig: s.sig,
        cert: s.cert,
    })
}

fn agreement_to(k: &AgreementKeyRecord) -> PbAgreementKey {
    PbAgreementKey {
        identity: k.identity.clone(),
        public: k.public.clone(),
        signed: signed_to(&k.signed),
    }
}

fn agreement_from(k: PbAgreementKey) -> Result<AgreementKeyRecord, DecodeError> {
    Ok(AgreementKeyRecord {
        identity: k.identity,
        public: k.public,
        signed: signed_from(k.signed)?,
    })
}

fn sealed_profile_to(p: &SealedProfile) -> PbSealedProfile {
    PbSealedProfile {
        identity: p.identity.clone(),
        version: p.version,
        ciphertext: p.ciphertext.clone(),
        signed: signed_to(&p.signed),
    }
}

fn sealed_profile_from(p: PbSealedProfile) -> Result<SealedProfile, DecodeError> {
    Ok(SealedProfile {
        identity: p.identity,
        version: p.version,
        ciphertext: p.ciphertext,
        signed: signed_from(p.signed)?,
    })
}

fn role_to(r: Role) -> i32 {
    (match r {
        Role::Owner => PbRole::Owner,
        Role::Admin => PbRole::Admin,
        Role::Member => PbRole::Member,
        Role::Reader => PbRole::Reader,
    }) as i32
}

fn role_from(r: i32) -> Result<Role, DecodeError> {
    match PbRole::try_from(r).map_err(|_| bad("role"))? {
        PbRole::Owner => Ok(Role::Owner),
        PbRole::Admin => Ok(Role::Admin),
        PbRole::Member => Ok(Role::Member),
        PbRole::Reader => Ok(Role::Reader),
    }
}

fn profile_to(p: &Identity) -> PbProfile {
    PbProfile {
        id: p.id.clone(),
        kind: match p.kind {
            IdentityKind::Person => PbIdentityKind::Person,
            IdentityKind::Agent => PbIdentityKind::Agent,
        } as i32,
        name: p.name.clone(),
        handle: p.handle.clone(),
        tint_hex: p.tint_hex.clone(),
        glyph: p.glyph.clone(),
        owner: p.owner.clone(),
        bio: p.bio.clone(),
    }
}

fn profile_from(p: PbProfile) -> Result<Identity, DecodeError> {
    Ok(Identity {
        id: p.id,
        kind: match PbIdentityKind::try_from(p.kind).map_err(|_| bad("identity kind"))? {
            PbIdentityKind::Person => IdentityKind::Person,
            PbIdentityKind::Agent => IdentityKind::Agent,
        },
        name: p.name,
        handle: p.handle,
        tint_hex: p.tint_hex,
        glyph: p.glyph,
        owner: p.owner,
        bio: p.bio,
    })
}

fn envelope_to(e: &Envelope) -> PbEnvelope {
    PbEnvelope {
        content: e.content().to_vec(),
        sig: e.sig.clone(),
        cert: e.cert.clone(),
        invite: e.invite.clone(),
        pruned: e.legacy_pruned_hash().map(str::to_string),
    }
}

fn envelope_from(e: PbEnvelope) -> Result<Envelope, DecodeError> {
    Envelope::new(e.content, e.sig, e.cert, e.invite)
        .and_then(|env| env.with_legacy_pruned(e.pruned))
        .ok_or_else(|| bad("envelope content"))
}

fn ephemeral_to(space: &str, from: &str, k: &EphemeralKind) -> PbEphemeral {
    use pb_ephemeral::Kind;
    PbEphemeral {
        space: space.to_string(),
        from: from.to_string(),
        kind: Some(match k {
            EphemeralKind::Typing => Kind::Typing(PbEmpty {}),
            EphemeralKind::StoppedTyping => Kind::StoppedTyping(PbEmpty {}),
            EphemeralKind::Status { status } => Kind::Status(status.clone()),
            EphemeralKind::Read { seq } => Kind::Read(*seq),
        }),
    }
}

fn ephemeral_kind(k: Option<pb_ephemeral::Kind>) -> Result<EphemeralKind, DecodeError> {
    use pb_ephemeral::Kind;
    Ok(match k.ok_or_else(|| bad("ephemeral kind"))? {
        Kind::Typing(_) => EphemeralKind::Typing,
        Kind::StoppedTyping(_) => EphemeralKind::StoppedTyping,
        Kind::Status(status) => EphemeralKind::Status { status },
        Kind::Read(seq) => EphemeralKind::Read { seq },
    })
}

impl Sequenced {
    /// The stored form of a log entry: the same bytes the wire carries.
    pub fn encode(&self) -> Vec<u8> {
        PbSequenced {
            seq: self.seq,
            prev: self.prev.clone(),
            hash: self.hash.clone(),
            env: Some(envelope_to(&self.env)),
        }
        .encode_to_vec()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let s = PbSequenced::decode(bytes).map_err(|e| DecodeError(e.to_string()))?;
        Ok(Sequenced {
            seq: s.seq,
            prev: s.prev,
            hash: s.hash,
            env: envelope_from(s.env.ok_or_else(|| bad("entry without envelope"))?)?,
        })
    }
}

impl ClientFrame {
    pub fn encode(&self) -> Vec<u8> {
        use pb_client_frame::F;
        let f = match self {
            ClientFrame::Hello {
                protocol,
                capabilities,
                identity,
                device,
                cert,
            } => F::Hello(PbHello {
                protocol: *protocol,
                capabilities: capabilities.clone(),
                identity: identity.clone(),
                device: device.clone(),
                cert: cert.clone(),
            }),
            ClientFrame::Auth { sig } => F::Auth(sig.clone()),
            ClientFrame::Req { id, op } => F::Req(PbReq {
                id: *id,
                op: Some(match op {
                    Op::Register { profile } => pb_req::Op::Register(profile_to(profile)),
                    Op::Lookup { handle, prefix } => pb_req::Op::Lookup(PbLookup {
                        handle: handle.clone(),
                        prefix: *prefix,
                    }),
                    Op::Profiles { ids } => pb_req::Op::Profiles(PbIds { ids: ids.clone() }),
                    Op::CreateInvite {
                        space,
                        role,
                        max_uses,
                        ttl_secs,
                    } => pb_req::Op::CreateInvite(PbCreateInvite {
                        space: space.clone(),
                        role: role_to(*role),
                        max_uses: *max_uses,
                        ttl_secs: *ttl_secs,
                    }),
                    Op::PreviewInvite { code } => pb_req::Op::PreviewInvite(code.clone()),
                    Op::PublishAgreementKey { public, signed } => {
                        pb_req::Op::PublishAgreementKey(PbAgreementKey {
                            identity: String::new(),
                            public: public.clone(),
                            signed: signed_to(signed),
                        })
                    }
                    Op::AgreementKeys { ids } => {
                        pb_req::Op::AgreementKeys(PbIds { ids: ids.clone() })
                    }
                    Op::PutProfile { profile } => {
                        pb_req::Op::PutProfile(sealed_profile_to(profile))
                    }
                    Op::GetProfiles { ids } => pb_req::Op::GetProfiles(PbIds { ids: ids.clone() }),
                    Op::PublishKeyPackages {
                        packages,
                        last_resort,
                    } => pb_req::Op::PublishKeyPackages(PbPublishKeyPackages {
                        packages: packages.clone(),
                        last_resort: last_resort.clone(),
                    }),
                    Op::ClaimKeyPackages { ids } => {
                        pb_req::Op::ClaimKeyPackages(PbIds { ids: ids.clone() })
                    }
                    Op::DeliverLink { id, sealed } => pb_req::Op::DeliverLink(PbLinkBox {
                        id: id.clone(),
                        sealed: sealed.clone(),
                    }),
                    Op::FetchLink { id } => pb_req::Op::FetchLink(id.clone()),
                    Op::Devices => pb_req::Op::Devices(PbEmpty {}),
                    Op::Unlink { device } => pb_req::Op::Unlink(device.clone()),
                    Op::SendDevice { to, sealed } => pb_req::Op::SendDevice(PbDeviceMessage {
                        from: String::new(),
                        to: to.clone(),
                        sealed: sealed.clone(),
                    }),
                }),
            }),
            ClientFrame::Publish { env } => F::Publish(envelope_to(env)),
            ClientFrame::Sync { cursors, all } => F::Sync(PbSync {
                cursors: cursors
                    .iter()
                    .map(|c| PbCursor {
                        space: c.space.clone(),
                        next_seq: c.next_seq,
                    })
                    .collect(),
                all: *all,
            }),
            ClientFrame::Ephemeral { space, kind } => F::Ephemeral(ephemeral_to(space, "", kind)),
            ClientFrame::Ping => F::Ping(PbEmpty {}),
        };
        PbClientFrame { f: Some(f) }.encode_to_vec()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        use pb_client_frame::F;
        let pb = PbClientFrame::decode(bytes).map_err(|e| DecodeError(e.to_string()))?;
        Ok(match pb.f.ok_or_else(|| bad("empty client frame"))? {
            F::Hello(h) => ClientFrame::Hello {
                protocol: h.protocol,
                capabilities: h.capabilities,
                identity: h.identity,
                device: h.device,
                cert: h.cert,
            },
            F::Auth(sig) => ClientFrame::Auth { sig },
            F::Req(r) => ClientFrame::Req {
                id: r.id,
                op: match r.op.ok_or_else(|| bad("empty request"))? {
                    pb_req::Op::Register(p) => Op::Register {
                        profile: profile_from(p)?,
                    },
                    pb_req::Op::Lookup(l) => Op::Lookup {
                        handle: l.handle,
                        prefix: l.prefix,
                    },
                    pb_req::Op::Profiles(p) => Op::Profiles { ids: p.ids },
                    pb_req::Op::CreateInvite(c) => Op::CreateInvite {
                        space: c.space,
                        role: role_from(c.role)?,
                        max_uses: c.max_uses,
                        ttl_secs: c.ttl_secs,
                    },
                    pb_req::Op::PreviewInvite(code) => Op::PreviewInvite { code },
                    pb_req::Op::PublishAgreementKey(k) => Op::PublishAgreementKey {
                        public: k.public,
                        signed: signed_from(k.signed)?,
                    },
                    pb_req::Op::AgreementKeys(p) => Op::AgreementKeys { ids: p.ids },
                    pb_req::Op::PutProfile(p) => Op::PutProfile {
                        profile: sealed_profile_from(p)?,
                    },
                    pb_req::Op::GetProfiles(p) => Op::GetProfiles { ids: p.ids },
                    pb_req::Op::PublishKeyPackages(p) => Op::PublishKeyPackages {
                        packages: p.packages,
                        last_resort: p.last_resort,
                    },
                    pb_req::Op::ClaimKeyPackages(p) => Op::ClaimKeyPackages { ids: p.ids },
                    pb_req::Op::DeliverLink(b) => Op::DeliverLink {
                        id: b.id,
                        sealed: b.sealed,
                    },
                    pb_req::Op::FetchLink(id) => Op::FetchLink { id },
                    pb_req::Op::Devices(_) => Op::Devices,
                    pb_req::Op::Unlink(device) => Op::Unlink { device },
                    pb_req::Op::SendDevice(m) => Op::SendDevice {
                        to: m.to,
                        sealed: m.sealed,
                    },
                },
            },
            // Only the relay prunes: a device can't hand it a stub.
            F::Publish(e) => {
                let env = envelope_from(e)?;
                if env.is_pruned() {
                    return Err(bad("a pruned envelope"));
                }
                ClientFrame::Publish { env }
            }
            F::Sync(s) => ClientFrame::Sync {
                cursors: s
                    .cursors
                    .into_iter()
                    .map(|c| Cursor {
                        space: c.space,
                        next_seq: c.next_seq,
                    })
                    .collect(),
                all: s.all,
            },
            F::Ephemeral(e) => ClientFrame::Ephemeral {
                space: e.space,
                kind: ephemeral_kind(e.kind)?,
            },
            F::Ping(_) => ClientFrame::Ping,
        })
    }
}

impl ServerFrame {
    pub fn encode(&self) -> Vec<u8> {
        use pb_server_frame::F;
        let f = match self {
            ServerFrame::Challenge {
                nonce,
                relay,
                protocol,
                capabilities,
            } => F::Challenge(PbChallenge {
                nonce: nonce.clone(),
                relay: relay.clone(),
                protocol: *protocol,
                capabilities: capabilities.clone(),
            }),
            ServerFrame::Ready {
                identity,
                registered,
            } => F::Ready(PbReady {
                identity: identity.clone(),
                registered: *registered,
            }),
            ServerFrame::Res { id, result } => F::Res(PbRes {
                id: *id,
                r: Some(match result {
                    Err(e) => pb_res::R::Err(e.clone()),
                    Ok(Reply::Registered(p)) => pb_res::R::Registered(profile_to(p)),
                    Ok(Reply::Profiles(ps)) => pb_res::R::Profiles(PbProfiles {
                        profiles: ps.iter().map(profile_to).collect(),
                    }),
                    Ok(Reply::Invite(i)) => pb_res::R::Invite(PbInviteCreated {
                        code: i.code.clone(),
                        expires_at_ms: i.expires_at_ms,
                    }),
                    Ok(Reply::Preview(p)) => pb_res::R::Preview(PbInvitePreview {
                        space: p.space.clone(),
                        role: role_to(p.role),
                        title: p.title.clone(),
                        members: p.members,
                        inviter: p.inviter.as_ref().map(profile_to),
                    }),
                    Ok(Reply::Done) => pb_res::R::Done(PbEmpty {}),
                    Ok(Reply::AgreementKeys(ks)) => pb_res::R::AgreementKeys(PbAgreementKeys {
                        keys: ks.iter().map(agreement_to).collect(),
                    }),
                    Ok(Reply::SealedProfiles(ps)) => pb_res::R::SealedProfiles(PbSealedProfiles {
                        profiles: ps.iter().map(sealed_profile_to).collect(),
                    }),
                    Ok(Reply::KeyPackages(ks)) => pb_res::R::KeyPackages(PbKeyPackages {
                        packages: ks
                            .iter()
                            .map(|k| PbKeyPackage {
                                identity: k.identity.clone(),
                                device: k.device.clone(),
                                data: k.data.clone(),
                            })
                            .collect(),
                    }),
                    Ok(Reply::Link(sealed)) => pb_res::R::Link(PbLinkBox {
                        id: String::new(),
                        sealed: sealed.clone().unwrap_or_default(),
                    }),
                    Ok(Reply::Devices(ds)) => pb_res::R::Devices(PbDevices {
                        devices: ds
                            .iter()
                            .map(|d| PbDevice {
                                device: d.device.clone(),
                                revoked: d.revoked,
                            })
                            .collect(),
                    }),
                }),
            }),
            ServerFrame::Event { ev } => F::Event(PbSequenced {
                seq: ev.seq,
                prev: ev.prev.clone(),
                hash: ev.hash.clone(),
                env: Some(envelope_to(&ev.env)),
            }),
            ServerFrame::Accepted {
                space,
                client_id,
                seq,
            } => F::Accepted(PbAccepted {
                space: space.clone(),
                client_id: client_id.clone(),
                seq: *seq,
            }),
            ServerFrame::Rejected {
                space,
                client_id,
                reason,
                permanent,
            } => F::Rejected(PbRejected {
                space: space.clone(),
                client_id: client_id.clone(),
                reason: reason.clone(),
                permanent: *permanent,
            }),
            ServerFrame::Ephemeral { space, from, kind } => {
                F::Ephemeral(ephemeral_to(space, from, kind))
            }
            ServerFrame::Presence { identity, online } => F::Presence(PbPresence {
                identity: identity.clone(),
                online: *online,
            }),
            ServerFrame::ProfileChanged { identity, version } => {
                F::ProfileChanged(PbProfileChanged {
                    identity: identity.clone(),
                    version: *version,
                })
            }
            ServerFrame::KeyPackagesLow { device, remaining } => {
                F::KeyPackagesLow(PbKeyPackagesLow {
                    device: device.clone(),
                    remaining: *remaining,
                })
            }
            ServerFrame::DeviceMessage { from, to, sealed } => F::DeviceMessage(PbDeviceMessage {
                from: from.clone(),
                to: to.clone(),
                sealed: sealed.clone(),
            }),
            ServerFrame::Joined { space } => F::Joined(space.clone()),
            ServerFrame::SyncDone => F::SyncDone(PbEmpty {}),
            ServerFrame::Pong => F::Pong(PbEmpty {}),
            ServerFrame::Error { code, message } => F::Error(PbError {
                code: match code {
                    ErrorCode::Other => PbErrorCode::Other,
                    ErrorCode::UpgradeRequired => PbErrorCode::UpgradeRequired,
                    ErrorCode::Unauthorized => PbErrorCode::Unauthorized,
                    ErrorCode::Unavailable => PbErrorCode::Unavailable,
                    ErrorCode::RateLimited => PbErrorCode::RateLimited,
                } as i32,
                message: message.clone(),
            }),
        };
        PbServerFrame { f: Some(f) }.encode_to_vec()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        use pb_server_frame::F;
        let pb = PbServerFrame::decode(bytes).map_err(|e| DecodeError(e.to_string()))?;
        Ok(match pb.f.ok_or_else(|| bad("empty server frame"))? {
            F::Challenge(c) => ServerFrame::Challenge {
                nonce: c.nonce,
                relay: c.relay,
                protocol: c.protocol,
                capabilities: c.capabilities,
            },
            F::Ready(r) => ServerFrame::Ready {
                identity: r.identity,
                registered: r.registered,
            },
            F::Res(r) => ServerFrame::Res {
                id: r.id,
                result: match r.r.ok_or_else(|| bad("empty result"))? {
                    pb_res::R::Err(e) => Err(e),
                    pb_res::R::Registered(p) => Ok(Reply::Registered(profile_from(p)?)),
                    pb_res::R::Profiles(ps) => Ok(Reply::Profiles(
                        ps.profiles
                            .into_iter()
                            .map(profile_from)
                            .collect::<Result<_, _>>()?,
                    )),
                    pb_res::R::Invite(i) => Ok(Reply::Invite(InviteCreated {
                        code: i.code,
                        expires_at_ms: i.expires_at_ms,
                    })),
                    pb_res::R::Preview(p) => Ok(Reply::Preview(InvitePreview {
                        space: p.space,
                        role: role_from(p.role)?,
                        title: p.title,
                        members: p.members,
                        inviter: p.inviter.map(profile_from).transpose()?,
                    })),
                    pb_res::R::Done(_) => Ok(Reply::Done),
                    pb_res::R::AgreementKeys(ks) => Ok(Reply::AgreementKeys(
                        ks.keys
                            .into_iter()
                            .map(agreement_from)
                            .collect::<Result<_, _>>()?,
                    )),
                    pb_res::R::SealedProfiles(ps) => Ok(Reply::SealedProfiles(
                        ps.profiles
                            .into_iter()
                            .map(sealed_profile_from)
                            .collect::<Result<_, _>>()?,
                    )),
                    pb_res::R::KeyPackages(ks) => Ok(Reply::KeyPackages(
                        ks.packages
                            .into_iter()
                            .map(|k| KeyPackageRecord {
                                identity: k.identity,
                                device: k.device,
                                data: k.data,
                            })
                            .collect(),
                    )),
                    pb_res::R::Link(b) => {
                        Ok(Reply::Link((!b.sealed.is_empty()).then_some(b.sealed)))
                    }
                    pb_res::R::Devices(ds) => Ok(Reply::Devices(
                        ds.devices
                            .into_iter()
                            .map(|d| DeviceRecord {
                                device: d.device,
                                revoked: d.revoked,
                            })
                            .collect(),
                    )),
                },
            },
            F::Event(s) => ServerFrame::Event {
                ev: Sequenced {
                    seq: s.seq,
                    prev: s.prev,
                    hash: s.hash,
                    env: envelope_from(s.env.ok_or_else(|| bad("event without envelope"))?)?,
                },
            },
            F::Accepted(a) => ServerFrame::Accepted {
                space: a.space,
                client_id: a.client_id,
                seq: a.seq,
            },
            F::Rejected(r) => ServerFrame::Rejected {
                space: r.space,
                client_id: r.client_id,
                reason: r.reason,
                permanent: r.permanent,
            },
            F::Ephemeral(e) => ServerFrame::Ephemeral {
                space: e.space,
                from: e.from,
                kind: ephemeral_kind(e.kind)?,
            },
            F::Presence(p) => ServerFrame::Presence {
                identity: p.identity,
                online: p.online,
            },
            F::ProfileChanged(p) => ServerFrame::ProfileChanged {
                identity: p.identity,
                version: p.version,
            },
            F::KeyPackagesLow(k) => ServerFrame::KeyPackagesLow {
                device: k.device,
                remaining: k.remaining,
            },
            F::DeviceMessage(m) => ServerFrame::DeviceMessage {
                from: m.from,
                to: m.to,
                sealed: m.sealed,
            },
            F::Joined(space) => ServerFrame::Joined { space },
            F::SyncDone(_) => ServerFrame::SyncDone,
            F::Pong(_) => ServerFrame::Pong,
            F::Error(e) => ServerFrame::Error {
                code: match PbErrorCode::try_from(e.code).unwrap_or(PbErrorCode::Other) {
                    PbErrorCode::Other => ErrorCode::Other,
                    PbErrorCode::UpgradeRequired => ErrorCode::UpgradeRequired,
                    PbErrorCode::Unauthorized => ErrorCode::Unauthorized,
                    PbErrorCode::Unavailable => ErrorCode::Unavailable,
                    PbErrorCode::RateLimited => ErrorCode::RateLimited,
                },
                message: e.message,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roda_log::{Author, Signer, SpaceLog};
    use roda_types::{EventBody, Privacy, SpaceKind};

    fn signed() -> (Envelope, SpaceLog) {
        let root = Signer::generate();
        let phone = Author::device(&root, Signer::generate());
        let mut log = SpaceLog::new("sp_1");
        let e = phone.sign_event(
            "sp_1",
            "01J",
            5,
            None,
            EventBody::SpaceCreated {
                title: "x".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        );
        log.sequence(e.clone());
        (Envelope::plain(&e), log)
    }

    #[test]
    fn a_published_envelope_crosses_the_wire_byte_for_byte_and_verifies() {
        let (env, _) = signed();
        let back =
            ClientFrame::decode(&ClientFrame::Publish { env: env.clone() }.encode()).unwrap();
        let ClientFrame::Publish { env: back } = back else {
            panic!("publish")
        };
        assert_eq!(back.content(), env.content());
        assert!(back.verify().is_ok());
        let mut forged = back.content().to_vec();
        let at = forged.len() - 3;
        forged[at] ^= 1;
        let forged = Envelope::new(forged, back.sig.clone(), back.cert.clone(), None);
        assert!(forged.is_none_or(|f| f.verify().is_err()));
    }

    fn signed_statement() -> DeviceSigned {
        DeviceSigned {
            device: "d".into(),
            sig: "s".into(),
            cert: "c".into(),
        }
    }

    fn sealed_profile() -> SealedProfile {
        SealedProfile {
            identity: "i".into(),
            version: 3,
            ciphertext: vec![1, 2, 3],
            signed: signed_statement(),
        }
    }

    #[test]
    fn every_server_frame_roundtrips() {
        let (env, log) = signed();
        let first = &log.events()[0];
        let profile = Identity {
            id: "ab".into(),
            kind: IdentityKind::Agent,
            name: "Pousada".into(),
            handle: "pousada".into(),
            tint_hex: "#fff".into(),
            glyph: Some("bed.double".into()),
            owner: Some("cd".into()),
            bio: String::new(),
        };
        let frames = vec![
            ServerFrame::Challenge {
                nonce: "n".into(),
                relay: "r".into(),
                protocol: 2,
                capabilities: vec!["blobs".into()],
            },
            ServerFrame::Ready {
                identity: "i".into(),
                registered: true,
            },
            ServerFrame::Res {
                id: 7,
                result: Ok(Reply::Profiles(vec![profile.clone()])),
            },
            ServerFrame::Res {
                id: 8,
                result: Ok(Reply::Registered(profile.clone())),
            },
            ServerFrame::Res {
                id: 9,
                result: Err("nope".into()),
            },
            ServerFrame::Res {
                id: 10,
                result: Ok(Reply::Preview(InvitePreview {
                    space: "sp".into(),
                    role: Role::Reader,
                    title: "t".into(),
                    members: 3,
                    inviter: Some(profile),
                })),
            },
            ServerFrame::Res {
                id: 11,
                result: Ok(Reply::Invite(InviteCreated {
                    code: "ABCD".into(),
                    expires_at_ms: 9,
                })),
            },
            ServerFrame::Event {
                ev: Sequenced {
                    seq: 0,
                    prev: first.prev.clone(),
                    hash: first.hash.clone(),
                    env,
                },
            },
            ServerFrame::Accepted {
                space: "sp".into(),
                client_id: "c".into(),
                seq: 4,
            },
            ServerFrame::Rejected {
                space: "sp".into(),
                client_id: "c".into(),
                reason: "slow down".into(),
                permanent: false,
            },
            ServerFrame::Ephemeral {
                space: "sp".into(),
                from: "f".into(),
                kind: EphemeralKind::Read { seq: 3 },
            },
            ServerFrame::Presence {
                identity: "i".into(),
                online: false,
            },
            ServerFrame::Res {
                id: 12,
                result: Ok(Reply::Done),
            },
            ServerFrame::Res {
                id: 13,
                result: Ok(Reply::AgreementKeys(vec![AgreementKeyRecord {
                    identity: "i".into(),
                    public: "p".into(),
                    signed: signed_statement(),
                }])),
            },
            ServerFrame::Res {
                id: 14,
                result: Ok(Reply::SealedProfiles(vec![sealed_profile()])),
            },
            ServerFrame::Res {
                id: 15,
                result: Ok(Reply::KeyPackages(vec![KeyPackageRecord {
                    identity: "i".into(),
                    device: "d".into(),
                    data: vec![1, 2],
                }])),
            },
            ServerFrame::ProfileChanged {
                identity: "i".into(),
                version: 4,
            },
            ServerFrame::KeyPackagesLow {
                device: "d".into(),
                remaining: 3,
            },
            ServerFrame::DeviceMessage {
                from: "a".into(),
                to: "b".into(),
                sealed: vec![7],
            },
            ServerFrame::Joined { space: "sp".into() },
            ServerFrame::SyncDone,
            ServerFrame::Pong,
            ServerFrame::error(ErrorCode::UpgradeRequired, "update"),
        ];
        for f in frames {
            assert_eq!(ServerFrame::decode(&f.encode()).unwrap(), f);
        }
    }

    #[test]
    fn every_client_frame_roundtrips() {
        let frames = vec![
            ClientFrame::Hello {
                protocol: 2,
                capabilities: vec!["presence".into()],
                identity: "i".into(),
                device: "d".into(),
                cert: "c".into(),
            },
            ClientFrame::Auth { sig: "s".into() },
            ClientFrame::Req {
                id: 1,
                op: Op::Lookup {
                    handle: "an".into(),
                    prefix: true,
                },
            },
            ClientFrame::Req {
                id: 2,
                op: Op::CreateInvite {
                    space: "sp".into(),
                    role: Role::Admin,
                    max_uses: 5,
                    ttl_secs: 60,
                },
            },
            ClientFrame::Req {
                id: 3,
                op: Op::PreviewInvite { code: "X".into() },
            },
            ClientFrame::Req {
                id: 4,
                op: Op::Profiles {
                    ids: vec!["a".into()],
                },
            },
            ClientFrame::Req {
                id: 5,
                op: Op::PublishAgreementKey {
                    public: "p".into(),
                    signed: signed_statement(),
                },
            },
            ClientFrame::Req {
                id: 6,
                op: Op::AgreementKeys {
                    ids: vec!["a".into()],
                },
            },
            ClientFrame::Req {
                id: 7,
                op: Op::PutProfile {
                    profile: sealed_profile(),
                },
            },
            ClientFrame::Req {
                id: 8,
                op: Op::GetProfiles {
                    ids: vec!["a".into()],
                },
            },
            ClientFrame::Req {
                id: 9,
                op: Op::PublishKeyPackages {
                    packages: vec![vec![1], vec![2]],
                    last_resort: Some(vec![3]),
                },
            },
            ClientFrame::Req {
                id: 10,
                op: Op::ClaimKeyPackages {
                    ids: vec!["a".into()],
                },
            },
            ClientFrame::Sync {
                cursors: vec![Cursor {
                    space: "sp".into(),
                    next_seq: 9,
                }],
                all: true,
            },
            ClientFrame::Ephemeral {
                space: "sp".into(),
                kind: EphemeralKind::Status {
                    status: "building".into(),
                },
            },
            ClientFrame::Ping,
        ];
        for f in frames {
            assert_eq!(ClientFrame::decode(&f.encode()).unwrap(), f);
        }
    }

    #[test]
    fn garbage_never_panics_the_decoder() {
        for bytes in [
            &[][..],
            &[0xff, 0xff, 0xff],
            &[0x0a, 0x05, 0x01],
            &[0x22, 0x02, 0x0a, 0x00],
        ] {
            let _ = ClientFrame::decode(bytes);
            let _ = ServerFrame::decode(bytes);
        }
        assert!(ClientFrame::decode(&[]).is_err());
    }
}

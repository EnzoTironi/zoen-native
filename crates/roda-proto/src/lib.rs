//! # roda-proto — Zoen Sync, the wire protocol (version 2)
//!
//! One WebSocket per device, one protobuf message per binary frame ([`wire`]).
//!
//! ```text
//! client                                         relay
//!   Hello{protocol, caps, identity, device, cert} ──▶
//!                                                ◀── Challenge{nonce, relay, protocol, caps}
//!   Auth{sig(nonce)}                              ──▶
//!                                                ◀── Ready{registered}
//!   Req{Register{profile}}                        ──▶  (first time only)
//!   Sync{cursors}                                 ──▶
//!                                                ◀── Event{..}* SyncDone
//!   Publish{envelope}                             ──▶  sequenced (per-Space order)
//!                                                ◀── Accepted{seq} + Event{..} to every member online
//!   Ephemeral{typing}                             ──▶  forwarded, never stored
//! ```
//!
//! The relay orders and stores; it never rewrites. An envelope is the author's exact
//! signed bytes ([`roda_log::content`]) plus the signature: the relay decodes them to route,
//! the client decodes them to show, and nobody encodes them again. Backpressure is the
//! socket's: the relay writes through a bounded queue and disconnects a device that falls
//! too far behind, which then resumes from its cursors.

pub mod experiments;
pub mod wire;

use roda_log::content::{decode_body, signed_hash, stub_content, Payload, SignedContent};
pub use roda_log::content::{Sealed, SealedKind};
use roda_log::{event_from_content, verify_author, verify_sig, Author, LogError};
use roda_types::{Event, EventBody, Identity, IdentityId, Role, Seen, SpaceId};

pub use roda_log::profile::DeviceSigned;

/// The version this build speaks, and the oldest one it still accepts.
pub const PROTOCOL_VERSION: u32 = 2;
/// Why a relay refuses a clear event in an end-to-end Space. A client that wrote it before
/// it learned the Space went end-to-end (ADR 0027) seals it and sends it again.
pub const SEAL_REQUIRED: &str = "this space is end-to-end encrypted; seal the event";
/// Under this many single-use key packages the relay asks the device to publish more.
pub const KEY_PACKAGES_LOW: u32 = 8;
/// Why a relay refuses a commit: another one already took its epoch (ADR 0026). The author
/// drops it, applies the winner from the log, and commits again if anything is still owed.
pub const STALE_COMMIT: &str = "stale_epoch: another commit took this epoch";
/// Why a relay refuses a message sealed at an epoch the group has left (a device that
/// sealed before catching up). It waits, catches up, and seals again at the new epoch.
pub const STALE_SEAL: &str = "stale_epoch: sealed at an epoch the group has left";
pub const MIN_PROTOCOL_VERSION: u32 = 2;
/// Domain tag for what devices sign outside the log (login, blob uploads).
pub const PROTOCOL: &str = "zoen-sync/2";
/// Optional features; each side announces its own and uses the intersection.
pub const CAPABILITIES: &[&str] = &["blobs", "invites", "presence", "profiles"];

/// The capabilities both sides have, in our order.
pub fn negotiate(theirs: &[String]) -> Vec<String> {
    CAPABILITIES
        .iter()
        .filter(|c| theirs.iter().any(|t| t == *c))
        .map(|c| c.to_string())
        .collect()
}

/// What a device publishes: the author's signed bytes, not sequenced yet.
#[derive(Clone, Debug, PartialEq)]
pub struct Envelope {
    content: Vec<u8>,
    parsed: SignedContent,
    pub sig: String,
    pub cert: Option<String>,
    /// Transport only, never stored: the invite code that lets a newcomer add themselves.
    pub invite: Option<String>,
}

impl Envelope {
    /// `None` when `content` isn't a v3 signed content.
    pub fn new(
        content: Vec<u8>,
        sig: String,
        cert: Option<String>,
        invite: Option<String>,
    ) -> Option<Self> {
        let parsed = SignedContent::parse(&content)?;
        parsed.payload.as_ref()?;
        Some(Self {
            content,
            parsed,
            sig,
            cert,
            invite,
        })
    }

    /// The stub a pruned sealed entry leaves (ADR 0026): the same header and kind and the
    /// hash of the MLS bytes, without them. It hashes and verifies like the original.
    /// `None` for anything that isn't sealed, or is already a stub.
    pub fn pruned(&self) -> Option<Envelope> {
        let content = stub_content(&self.content)?;
        Envelope::new(content, self.sig.clone(), self.cert.clone(), None)
    }

    /// A stub the relay left when it pruned this entry.
    pub fn is_pruned(&self) -> bool {
        matches!(&self.parsed.payload, Some(Payload::Sealed(s)) if s.is_stub())
    }

    /// Wraps a signed, unsequenced plaintext event.
    pub fn plain(e: &Event) -> Self {
        Self::new(e.content.clone(), e.sig.clone(), e.cert.clone(), None)
            .expect("events carry v3 content")
    }

    /// Wraps MLS ciphertext signed by `author`'s device.
    pub fn sealed(
        author: &Author,
        space: &str,
        client_id: &str,
        at_ms: i64,
        seen: Option<&Seen>,
        sealed: Sealed,
    ) -> Self {
        let (content, sig) = author.sign_sealed(space, client_id, at_ms, seen, sealed);
        Self::new(content, sig, author.cert.clone(), None).expect("sealed content parses")
    }

    /// The MLS bytes and their kind, for envelopes MLS opens.
    pub fn sealed_data(&self) -> Option<(SealedKind, &[u8])> {
        match &self.parsed.payload {
            Some(Payload::Sealed(s)) => Some((
                SealedKind::try_from(s.kind).unwrap_or(SealedKind::Unspecified),
                &s.data,
            )),
            _ => None,
        }
    }

    pub fn content(&self) -> &[u8] {
        &self.content
    }

    pub fn space(&self) -> &str {
        &self.parsed.space
    }

    pub fn client_id(&self) -> &str {
        &self.parsed.client_id
    }

    pub fn author(&self) -> &str {
        &self.parsed.author
    }

    pub fn device(&self) -> Option<&str> {
        self.parsed.device.as_deref()
    }

    pub fn at_ms(&self) -> i64 {
        self.parsed.at_ms
    }

    pub fn seen(&self) -> Option<Seen> {
        self.parsed.seen()
    }

    pub fn is_sealed(&self) -> bool {
        matches!(self.parsed.payload, Some(Payload::Sealed(_)))
    }

    /// What a sealed envelope carries, from its clear framing (`None` when plain).
    pub fn sealed_kind(&self) -> Option<SealedKind> {
        self.sealed_data().map(|(kind, _)| kind)
    }

    /// The body, for envelopes the relay may read (`None` when sealed).
    pub fn body(&self) -> Option<EventBody> {
        match &self.parsed.payload {
            Some(Payload::Body(b)) => Some(decode_body(b)),
            _ => None,
        }
    }

    /// The hash the chain links: over the exact bytes, plain or sealed.
    pub fn wire_hash(&self) -> String {
        signed_hash(&self.content)
    }

    /// Checks the device certificate and the author's signature over the bytes.
    pub fn verify(&self) -> Result<(), LogError> {
        if let Some(e) = self.plain_event() {
            return verify_author(&e);
        }
        let signer = match self.device() {
            Some(d) => {
                let cert = self
                    .cert
                    .as_deref()
                    .ok_or(LogError::BadDeviceCert { seq: 0 })?;
                if !verify_sig(self.author(), &roda_log::device_cert_message(d), cert) {
                    return Err(LogError::BadDeviceCert { seq: 0 });
                }
                d
            }
            None => self.author(),
        };
        if verify_sig(signer, self.wire_hash().as_bytes(), &self.sig) {
            Ok(())
        } else {
            Err(LogError::BadSignature { seq: 0 })
        }
    }

    /// The event as the relay sees it (plain only), unsequenced.
    pub fn plain_event(&self) -> Option<Event> {
        if self.is_sealed() {
            return None;
        }
        event_from_content(
            self.content.clone(),
            self.sig.clone(),
            self.cert.clone(),
            0,
            String::new(),
            String::new(),
        )
        .ok()
    }

    /// Size guard input: the bytes the relay will store.
    pub fn stored_len(&self) -> usize {
        self.content.len() + self.sig.len() + self.cert.as_ref().map_or(0, String::len)
    }
}

/// An envelope after the relay put it in the Space's order.
#[derive(Clone, Debug, PartialEq)]
pub struct Sequenced {
    pub seq: u64,
    pub prev: String,
    pub hash: String,
    pub env: Envelope,
}

impl Sequenced {
    /// The plaintext event with its place in the chain (sealed envelopes need MLS first).
    pub fn plain_event(&self) -> Option<Event> {
        let mut e = self.env.plain_event()?;
        e.seq = self.seq;
        e.prev = self.prev.clone();
        e.hash = self.hash.clone();
        Some(e)
    }
}

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
        role: Role,
        max_uses: u32,
        ttl_secs: u64,
    },
    PreviewInvite {
        code: String,
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
    },
    /// Leaves a sealed link box for a device being linked (ADR 0045), under `id` = SHA-256
    /// of the link secret in its QR code. Only the new device can open it.
    DeliverLink {
        id: String,
        sealed: Vec<u8>,
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
}

/// One of an identity's devices, as the directory has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceRecord {
    pub device: String,
    pub revoked: bool,
}

/// One device's MLS key package, as claimed. Members re-check the leaf inside; the relay's
/// check only keeps junk out of the table.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    Pong,
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

/// What the device signs to log in: binds the nonce to this relay and protocol.
pub fn auth_message(nonce: &str, relay: &str) -> Vec<u8> {
    format!("{PROTOCOL}:auth:{relay}:{nonce}").into_bytes()
}

/// What a device signs to upload a blob: binds the content hash and a timestamp to this
/// relay, so a captured header only re-uploads the same bytes for a few minutes.
/// What a device signs to put or delete a chunk of a history transfer (ADR 0045).
/// `op` is "put" or "delete"; `n` and `sha256` are empty for a delete.
pub fn transfer_message(op: &str, transfer: &str, n: &str, sha256: &str, ts_ms: i64) -> Vec<u8> {
    format!("{PROTOCOL}:transfer-{op}:{transfer}:{n}:{sha256}:{ts_ms}").into_bytes()
}

pub fn blob_put_message(sha256: &str, ts_ms: i64, relay: &str) -> Vec<u8> {
    format!("{PROTOCOL}:blob-put:{relay}:{sha256}:{ts_ms}").into_bytes()
}

/// Handles: 3–24 chars, lowercase letters, digits, `_` and `.`; must start with a letter.
pub fn normalize_handle(raw: &str) -> Option<String> {
    let h = raw.trim().trim_start_matches('@').to_lowercase();
    let ok_len = (3..=24).contains(&h.chars().count());
    let ok_chars = h
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.');
    let ok_start = h
        .chars()
        .next()
        .map(|c| c.is_ascii_lowercase())
        .unwrap_or(false);
    (ok_len && ok_chars && ok_start).then_some(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_are_normalized() {
        assert_eq!(normalize_handle("@Enzo.T").as_deref(), Some("enzo.t"));
        assert_eq!(normalize_handle("1abc"), None);
        assert_eq!(normalize_handle("ab"), None);
        assert_eq!(normalize_handle("a b c"), None);
    }

    #[test]
    fn capabilities_are_the_intersection() {
        assert_eq!(
            negotiate(&["presence".into(), "telepathy".into(), "blobs".into()]),
            vec!["blobs", "presence"]
        );
    }
}

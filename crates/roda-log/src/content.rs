//! Format v3: what an author signs, as protobuf bytes that are encoded exactly once.
//!
//! ```text
//! message SignedContent {            // zoen.log.v3
//!   uint32 v = 1;                    // 3
//!   string space = 2;
//!   string client_id = 3;            // idempotency key chosen by the author's device (ULID)
//!   string author = 4;               // identity key, hex
//!   optional string device = 5;      // device key, hex (certified by the identity)
//!   int64 at_ms = 6;
//!   Seen seen = 7;                   // causal link: newest event the author had applied
//!   oneof payload {
//!     bytes body = 8;                // EventBody as JSON (typed by the Rust enum)
//!     Sealed sealed = 9;             // MLS ciphertext; the relay sees only bytes
//!   }
//! }
//! message Seen { uint64 seq = 1; string hash = 2; }
//! message Sealed { SealedKind kind = 1; uint32 suite = 2; bytes data = 3; }
//! ```
//!
//! The content hash is SHA-256 over a domain tag plus these bytes, and the signature is
//! over that hash. Decoders keep the bytes; unknown fields and unknown body kinds survive
//! every hop because nothing is re-encoded.

use prost::Message;
use roda_types::{EventBody, Seen};
use sha2::{Digest, Sha256};

pub const DOMAIN: &[u8] = b"zoen-content-v3\0";

#[derive(Clone, PartialEq, Message)]
pub struct SignedContent {
    #[prost(uint32, tag = "1")]
    pub v: u32,
    #[prost(string, tag = "2")]
    pub space: String,
    #[prost(string, tag = "3")]
    pub client_id: String,
    #[prost(string, tag = "4")]
    pub author: String,
    #[prost(string, optional, tag = "5")]
    pub device: Option<String>,
    #[prost(int64, tag = "6")]
    pub at_ms: i64,
    #[prost(message, optional, tag = "7")]
    pub seen: Option<SeenLink>,
    #[prost(oneof = "Payload", tags = "8, 9")]
    pub payload: Option<Payload>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SeenLink {
    #[prost(uint64, tag = "1")]
    pub seq: u64,
    #[prost(string, tag = "2")]
    pub hash: String,
}

#[derive(Clone, PartialEq, prost::Oneof)]
pub enum Payload {
    #[prost(bytes, tag = "8")]
    Body(Vec<u8>),
    #[prost(message, tag = "9")]
    Sealed(Sealed),
}

#[derive(Clone, PartialEq, Message)]
pub struct Sealed {
    #[prost(enumeration = "SealedKind", tag = "1")]
    pub kind: i32,
    /// MLS ciphersuite id (RFC 9420 registry), so the format is crypto-agile.
    #[prost(uint32, tag = "2")]
    pub suite: u32,
    #[prost(bytes = "vec", tag = "3")]
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum SealedKind {
    Unspecified = 0,
    /// MLS application message carrying an inner signed event.
    Application = 1,
    /// MLS commit (membership or key change).
    Commit = 2,
    /// MLS welcome for newly added members.
    Welcome = 3,
}

impl Sealed {
    pub fn new(kind: SealedKind, suite: u32, data: Vec<u8>) -> Self {
        Self {
            kind: kind as i32,
            suite,
            data,
        }
    }
}

impl From<&Seen> for SeenLink {
    fn from(s: &Seen) -> Self {
        SeenLink {
            seq: s.seq,
            hash: s.hash.clone(),
        }
    }
}

impl From<&SeenLink> for Seen {
    fn from(s: &SeenLink) -> Self {
        Seen {
            seq: s.seq,
            hash: s.hash.clone(),
        }
    }
}

/// The hash an author signs: SHA-256(domain ‖ content), lowercase hex.
pub fn content_hash(content: &[u8]) -> String {
    hex::encode(
        Sha256::new()
            .chain_update(DOMAIN)
            .chain_update(content)
            .finalize(),
    )
}

/// Decodes the body, or names the kind this build doesn't know.
pub fn decode_body(json: &[u8]) -> EventBody {
    serde_json::from_slice(json).unwrap_or_else(|_| {
        let kind = serde_json::from_slice::<serde_json::Value>(json)
            .ok()
            .and_then(|v| match v {
                serde_json::Value::Object(m) => m.keys().next().cloned(),
                serde_json::Value::String(s) => Some(s),
                _ => None,
            })
            .unwrap_or_else(|| "unknown".into());
        EventBody::Unsupported { kind }
    })
}

pub fn encode_body(body: &EventBody) -> Vec<u8> {
    serde_json::to_vec(body).expect("event bodies always serialize")
}

impl SignedContent {
    pub fn encode(&self) -> Vec<u8> {
        self.encode_to_vec()
    }

    pub fn parse(bytes: &[u8]) -> Option<Self> {
        Self::decode(bytes).ok()
    }

    pub fn seen(&self) -> Option<Seen> {
        self.seen.as_ref().map(Seen::from)
    }
}

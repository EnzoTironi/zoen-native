//! Signed protobuf content: v3 clear events and v4 sealed events with pruning proofs.
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
//! message Sealed { SealedKind kind = 1; uint32 suite = 2; bytes data = 3; bytes data_hash = 4; }
//! ```
//!
//! The content hash is SHA-256 over a domain tag plus these bytes, and the signature is
//! over that hash. Decoders keep the bytes; unknown fields and unknown body kinds survive
//! every hop because nothing is re-encoded.

use prost::Message;
use roda_types::{EventBody, Seen};
use sha2::{Digest, Sha256};

pub const DOMAIN: &[u8] = b"zoen-content-v3\0";
/// Sealed entries with an authenticated pruning digest. Clear events remain v3.
pub const SEALED_CONTENT_VERSION: u32 = 4;

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
    /// Only in a pruned stub (ADR 0026): SHA-256 of the MLS bytes the relay took out.
    #[prost(bytes = "vec", tag = "4")]
    pub data_hash: Vec<u8>,
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

/// What an MLS application message carries: the author's inner signed event. The outer
/// envelope repeats its space, client id, author, device, time and `seen`; members check
/// both agree before they accept it.
#[derive(Clone, PartialEq, Message)]
pub struct InnerEvent {
    #[prost(bytes = "vec", tag = "1")]
    pub content: Vec<u8>,
    #[prost(string, tag = "2")]
    pub sig: String,
}

impl InnerEvent {
    pub fn encode(&self) -> Vec<u8> {
        self.encode_to_vec()
    }
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        <Self as Message>::decode(bytes).ok()
    }
}

impl SealedKind {
    /// The name a device records for an entry it keeps sealed.
    pub fn name(self) -> &'static str {
        match self {
            SealedKind::Unspecified => "Unspecified",
            SealedKind::Application => "Application",
            SealedKind::Commit => "Commit",
            SealedKind::Welcome => "Welcome",
        }
    }
}

impl Sealed {
    pub fn new(kind: SealedKind, suite: u32, data: Vec<u8>) -> Self {
        Self {
            kind: kind as i32,
            suite,
            data,
            data_hash: Vec::new(),
        }
    }

    /// What a pruned stub keeps of this: no MLS bytes, their hash.
    pub fn stub(&self) -> Sealed {
        Sealed {
            data: Vec::new(),
            data_hash: Sha256::digest(&self.data).to_vec(),
            ..self.clone()
        }
    }

    pub fn is_stub(&self) -> bool {
        self.data.is_empty() && !self.data_hash.is_empty()
    }

    fn data_digest(&self) -> Vec<u8> {
        if self.is_stub() {
            self.data_hash.clone()
        } else {
            Sha256::digest(&self.data).to_vec()
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

/// Domain of a sealed entry's hash.
pub const SEALED_DOMAIN: &[u8] = b"zoen-sealed-v1\0";

/// V4 signatures cannot be replayed as opaque legacy v3 stub hashes. Legacy signatures
/// signed only the digest, so their surviving headers remain a historical trust limit.
pub fn signature_message(content: &[u8], hash: &str) -> Vec<u8> {
    let mut message = Vec::new();
    if SignedContent::parse(content).is_some_and(|c| {
        c.v >= SEALED_CONTENT_VERSION && matches!(c.payload, Some(Payload::Sealed(_)))
    }) {
        message.extend_from_slice(b"zoen-sealed-signature-v4\0");
    }
    message.extend_from_slice(hash.as_bytes());
    message
}

/// The hash an author signs and the chain links (ADR 0026). V3 keeps its exact-byte hash.
/// V4 sealed content: SHA-256(sealed domain ‖ hash of the header with the MLS bytes out ‖
/// SHA-256 of the MLS bytes). A pruned stub keeps the header and the bytes' hash, so it
/// hashes (and verifies) the same as the original, and only a sealed entry can become one.
pub fn signed_hash(content: &[u8]) -> String {
    let Some(c) = SignedContent::parse(content) else {
        return content_hash(content);
    };
    let Some(Payload::Sealed(s)) = &c.payload else {
        return content_hash(content);
    };
    if c.v < SEALED_CONTENT_VERSION {
        return content_hash(content);
    }
    let Some(header) = rewrite_sealed(content, strip_sealed_data) else {
        return content_hash(content);
    };
    sealed_hash(&header, &s.data_digest())
}

fn sealed_hash(header: &[u8], digest: &[u8]) -> String {
    hex::encode(
        Sha256::new()
            .chain_update(SEALED_DOMAIN)
            .chain_update(content_hash(header).as_bytes())
            .chain_update(digest)
            .finalize(),
    )
}

/// Compatibility for the unversioned pruning hash briefly emitted by M2 branches.
/// Readers select this only when the author's signature authenticates it.
pub fn unversioned_sealed_hash(content: &[u8]) -> Option<String> {
    let mut c = SignedContent::parse(content)?;
    let Some(Payload::Sealed(s)) = &mut c.payload else {
        return None;
    };
    let digest = s.data_digest();
    s.data = Vec::new();
    s.data_hash = Vec::new();
    Some(sealed_hash(&c.encode(), &digest))
}

/// The stub content a pruned sealed entry leaves; `None` unless sealed with its bytes.
pub fn stub_content(content: &[u8]) -> Option<Vec<u8>> {
    let mut c = SignedContent::parse(content)?;
    let Some(Payload::Sealed(sealed)) = &c.payload else {
        return None;
    };
    if c.v >= SEALED_CONTENT_VERSION {
        if sealed.is_stub() {
            return None;
        }
        // Protobuf merges repeated message fields. Every raw occurrence must carry
        // the digest of that final merged payload, including an empty last occurrence.
        let digest = sealed.data_digest();
        return rewrite_sealed(content, |bytes| {
            let mut header = strip_sealed_data(bytes)?;
            append_bytes(&mut header, 4, &digest);
            Some(header)
        });
    }
    match &mut c.payload {
        Some(Payload::Sealed(s)) if !s.is_stub() => *s = s.stub(),
        _ => return None,
    }
    Some(c.encode())
}

// Preserve raw protobuf fields, including unknown extensions, in the signed header.
// Re-encoding through prost would silently remove those fields before hashing a stub.
fn rewrite_sealed(content: &[u8], rewrite: impl Fn(&[u8]) -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    let mut input = content;
    let mut output = Vec::new();
    while !input.is_empty() {
        let field = take_field(&mut input)?;
        if field.tag == 9 && field.wire == 2 {
            append_bytes(&mut output, 9, &rewrite(field.body?)?);
        } else {
            output.extend_from_slice(field.encoded);
        }
    }
    Some(output)
}

fn strip_sealed_data(mut input: &[u8]) -> Option<Vec<u8>> {
    let mut output = Vec::new();
    while !input.is_empty() {
        let field = take_field(&mut input)?;
        if field.wire != 2 || !matches!(field.tag, 3 | 4) {
            output.extend_from_slice(field.encoded);
        }
    }
    Some(output)
}

struct Field<'a> {
    tag: u64,
    wire: u8,
    encoded: &'a [u8],
    body: Option<&'a [u8]>,
}

fn take_varint(input: &mut &[u8]) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let (&byte, rest) = input.split_first()?;
        *input = rest;
        if shift == 63 && byte > 1 {
            return None;
        }
        value |= u64::from(byte & 127) << shift;
        if byte < 128 {
            return Some(value);
        }
    }
    None
}

fn take_field<'a>(input: &mut &'a [u8]) -> Option<Field<'a>> {
    let start = *input;
    let key = take_varint(input)?;
    let tag = key >> 3;
    if tag == 0 {
        return None;
    }
    let wire = (key & 7) as u8;
    let body = match wire {
        0 => {
            take_varint(input)?;
            None
        }
        1 | 5 => {
            let size = if wire == 1 { 8 } else { 4 };
            *input = input.get(size..)?;
            None
        }
        2 => {
            let size = usize::try_from(take_varint(input)?).ok()?;
            let bytes = input.get(..size)?;
            *input = input.get(size..)?;
            Some(bytes)
        }
        _ => return None,
    };
    Some(Field {
        tag,
        wire,
        encoded: &start[..start.len() - input.len()],
        body,
    })
}

fn append_varint(output: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        output.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    output.push(value as u8);
}

fn append_bytes(output: &mut Vec<u8>, tag: u64, bytes: &[u8]) {
    append_varint(output, (tag << 3) | 2);
    append_varint(output, bytes.len() as u64);
    output.extend_from_slice(bytes);
}

/// SHA-256(domain ‖ content), lowercase hex: the hash of clear content.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_four_authenticates_unknown_fields_inside_the_sealed_header() {
        let author = crate::Author::root(crate::Signer::from_secret(&[53; 32]));
        let (content, _) = author.sign_sealed(
            "space",
            "message",
            1,
            None,
            Sealed::new(SealedKind::Application, 1, vec![10, 20, 30]),
        );
        let content = rewrite_sealed(&content, |inner| {
            let mut bytes = inner.to_vec();
            append_bytes(&mut bytes, 127, b"future");
            Some(bytes)
        })
        .unwrap();
        let hash = signed_hash(&content);
        let sig = author.key.sign(&signature_message(&content, &hash));
        let stub = stub_content(&content).unwrap();
        assert_eq!(signed_hash(&stub), hash);
        assert!(crate::verify_sig(
            &author.identity,
            &signature_message(&stub, &signed_hash(&stub)),
            &sig
        ));
        let changed = rewrite_sealed(&stub, |inner| {
            let mut bytes = inner.to_vec();
            let unknown = bytes.windows(6).position(|b| b == b"future").unwrap();
            bytes[unknown] ^= 1;
            Some(bytes)
        })
        .unwrap();
        assert!(!crate::verify_sig(
            &author.identity,
            &signature_message(&changed, &signed_hash(&changed)),
            &sig
        ));
    }

    #[test]
    fn repeated_sealed_fields_keep_the_final_merged_digest_when_pruned() {
        let author = crate::Author::root(crate::Signer::from_secret(&[57; 32]));
        let (mut content, _) = author.sign_sealed(
            "space",
            "message",
            1,
            None,
            Sealed::new(SealedKind::Application, 1, vec![10, 20, 30]),
        );
        // An empty second message merges with the first; it does not replace data.
        append_bytes(&mut content, 9, &[]);
        let hash = signed_hash(&content);
        let sig = author.key.sign(&signature_message(&content, &hash));
        let stub = stub_content(&content).unwrap();
        assert_eq!(signed_hash(&stub), hash);
        assert!(crate::verify_sig(
            &author.identity,
            &signature_message(&stub, &signed_hash(&stub)),
            &sig,
        ));
        let parsed = SignedContent::parse(&stub).unwrap();
        let Some(Payload::Sealed(sealed)) = parsed.payload else {
            panic!("sealed fixture");
        };
        assert!(sealed.data.is_empty());
        assert_eq!(sealed.data_hash, Sha256::digest([10, 20, 30]).to_vec());
    }
}

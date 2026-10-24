//! # event-log
//!
//! The event log of each Space: **append-only, signed and hash-chained** (format v3).
//!
//! Three independent proofs per event:
//! - **Who said it:** the author (or one of the author's devices, certified by the
//!   identity key) signs the hash of the exact content bytes ([`content`]): space, client
//!   id, author, device, time, causal link and body. Nobody can change what was said or
//!   who said it, and nobody re-encodes it.
//! - **Where it sits:** whoever sequences the Space (the relay, or this device for
//!   local-only Spaces) links `hash = SHA-256(ChainLink{space, seq, prev, wire})`.
//! - **What the author had seen:** the content carries the `(seq, hash)` of the newest
//!   event the author had applied. A sequencer that reorders, or shows an author's words
//!   on a different history, breaks that link on every honest device.
//!
//! The author does not sign its own `seq`/`prev`: those are assigned by the relay after
//! the fact, which is what makes offline-first sending with a total order per Space work.

pub mod agent_owner;
pub mod content;
pub mod media;
pub mod owner_budget;
pub mod owner_policy_inquiry;
pub mod profile;
pub mod profile_sync;
pub mod recovery;

use content::{Payload, SignedContent};
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use thiserror::Error;
use types::{ChainLink, Event, EventBody, IdentityId, Seen, SpaceId, EVENT_FORMAT, GENESIS_PREV};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LogError {
    #[error("evento #{seq} pertence a outro Espaço")]
    WrongSpace { seq: u64 },
    #[error("sequência quebrada: esperava #{expected}, veio #{got}")]
    BadSequence { expected: u64, got: u64 },
    #[error("cadeia quebrada no evento #{seq}")]
    BrokenChain { seq: u64 },
    #[error("hash não confere no evento #{seq} (conteúdo alterado)")]
    BadHash { seq: u64 },
    #[error("assinatura inválida no evento #{seq}")]
    BadSignature { seq: u64 },
    #[error("chave de autor inválida no evento #{seq}")]
    BadAuthorKey { seq: u64 },
    #[error("aparelho não certificado pelo autor no evento #{seq}")]
    BadDeviceCert { seq: u64 },
    #[error("conteúdo assinado ilegível ou diferente do evento #{seq}")]
    BadContent { seq: u64 },
    #[error("o evento #{seq} aponta para uma história que este aparelho não tem")]
    BrokenCausalLink { seq: u64 },
    #[error("o evento #{seq} não diz o que o autor já tinha visto")]
    MissingCausalLink { seq: u64 },
    #[error("o evento #{seq} cria de novo um Espaço que já existe")]
    Recreated { seq: u64 },
}

/// An Ed25519 signing key (identity root key or device key).
#[derive(Clone)]
pub struct Signer {
    key: SigningKey,
}

impl Signer {
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).expect("entropia do sistema");
        Self {
            key: SigningKey::from_bytes(&seed),
        }
    }

    pub fn from_secret(secret: &[u8; 32]) -> Self {
        Self {
            key: SigningKey::from_bytes(secret),
        }
    }

    pub fn secret(&self) -> [u8; 32] {
        self.key.to_bytes()
    }

    /// The public key in hex. For an identity key, this *is* the Identity.
    pub fn id(&self) -> IdentityId {
        hex::encode(self.key.verifying_key().to_bytes())
    }

    pub fn sign(&self, msg: &[u8]) -> String {
        hex::encode(self.key.sign(msg).to_bytes())
    }
}

/// Who signs events for an identity on this device: the identity key itself, or a
/// device key plus the identity's certificate over it.
#[derive(Clone)]
pub struct Author {
    pub identity: IdentityId,
    pub key: Signer,
    pub device: Option<String>,
    pub cert: Option<String>,
}

impl Author {
    /// The identity key signs directly (local personas, agents without devices).
    pub fn root(key: Signer) -> Self {
        Self {
            identity: key.id(),
            key,
            device: None,
            cert: None,
        }
    }

    /// A device key certified by the identity key.
    pub fn device(identity: &Signer, device: Signer) -> Self {
        let cert = identity.sign(&device_cert_message(&device.id()));
        Self {
            identity: identity.id(),
            device: Some(device.id()),
            cert: Some(cert),
            key: device,
        }
    }

    /// A device key with a certificate made earlier (the identity key may live elsewhere).
    pub fn certified(identity: IdentityId, device: Signer, cert: String) -> Self {
        Self {
            identity,
            device: Some(device.id()),
            cert: Some(cert),
            key: device,
        }
    }

    /// Builds and signs an event that isn't sequenced yet (`seq`/`prev`/`hash` empty).
    /// `seen` is the newest relay-ordered event this device has applied in the Space.
    pub fn sign_event(
        &self,
        space: &str,
        client_id: &str,
        at_ms: i64,
        seen: Option<Seen>,
        body: EventBody,
    ) -> Event {
        let content = SignedContent {
            v: EVENT_FORMAT as u32,
            space: space.to_string(),
            client_id: client_id.to_string(),
            author: self.identity.clone(),
            device: self.device.clone(),
            at_ms,
            seen: seen.as_ref().map(Into::into),
            payload: Some(Payload::Body(content::encode_body(&body))),
        }
        .encode();
        let sig = self.key.sign(content::content_hash(&content).as_bytes());
        Event {
            space: space.to_string(),
            seq: 0,
            prev: String::new(),
            author: self.identity.clone(),
            at_ms,
            body,
            hash: String::new(),
            sig,
            client_id: client_id.to_string(),
            device: self.device.clone(),
            cert: self.cert.clone(),
            seen,
            content,
            sealed_wire: None,
        }
    }

    /// Signs MLS ciphertext for `space` (ADR 0026). The relay sees the framing (kind,
    /// suite, author, device, `seen`) and orders it; the event inside is MLS's to open.
    /// Returns the content bytes and the signature over their hash.
    pub fn sign_sealed(
        &self,
        space: &str,
        client_id: &str,
        at_ms: i64,
        seen: Option<&Seen>,
        sealed: content::Sealed,
    ) -> (Vec<u8>, String) {
        let content = SignedContent {
            v: content::SEALED_CONTENT_VERSION,
            space: space.to_string(),
            client_id: client_id.to_string(),
            author: self.identity.clone(),
            device: self.device.clone(),
            at_ms,
            seen: seen.map(Into::into),
            payload: Some(Payload::Sealed(sealed)),
        }
        .encode();
        let sig = self.key.sign(&content::signature_message(
            &content,
            &content::signed_hash(&content),
        ));
        (content, sig)
    }
}

/// Rebuilds an event from what traveled: the author's exact bytes, the signature and
/// certificate, and the sequencer's position. Every view field comes from the bytes.
pub fn event_from_content(
    content: Vec<u8>,
    sig: String,
    cert: Option<String>,
    seq: u64,
    prev: String,
    hash: String,
) -> Result<Event, LogError> {
    let c = SignedContent::parse(&content).ok_or(LogError::BadContent { seq })?;
    let body = match &c.payload {
        Some(Payload::Body(body)) => content::decode_body(body),
        Some(Payload::Sealed(s)) => sealed_body(s),
        None => return Err(LogError::BadContent { seq }),
    };
    Ok(Event {
        body,
        seen: c.seen(),
        space: c.space,
        seq,
        prev,
        author: c.author,
        at_ms: c.at_ms,
        hash,
        sig,
        client_id: c.client_id,
        device: c.device,
        cert,
        content,
        sealed_wire: None,
    })
}

/// The view of an entry kept sealed: its kind, from the clear framing.
fn sealed_body(s: &content::Sealed) -> EventBody {
    let kind = content::SealedKind::try_from(s.kind).unwrap_or(content::SealedKind::Unspecified);
    EventBody::Sealed {
        kind: kind.name().to_string(),
    }
}

/// What an identity signs to certify a device key.
pub fn device_cert_message(device: &str) -> Vec<u8> {
    format!("zoen-device-v1:{device}").into_bytes()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The hash the author signed: over the exact content bytes for clear events, over the
/// header and the hash of the MLS bytes for sealed ones (`content::signed_hash`).
pub fn content_hash_of(e: &Event) -> String {
    if let Some(original) = legacy_stub_hash(e) {
        return original.to_string();
    }
    signed_content_hash(&e.content, &e.sig)
}

/// Select the signed hash without changing any legacy chain links. M2 briefly emitted
/// a pruning hash with v3; accept that variant only with its authentic signature.
pub fn signed_content_hash(bytes: &[u8], sig: &str) -> String {
    let hash = content::signed_hash(bytes);
    if let Some(c) = SignedContent::parse(bytes) {
        if c.v == 3 && matches!(c.payload, Some(Payload::Sealed(_))) {
            let signer = c.device.as_deref().unwrap_or(&c.author);
            if !verify_sig(signer, hash.as_bytes(), sig) {
                if let Some(previous) = content::unversioned_sealed_hash(bytes) {
                    if verify_sig(signer, previous.as_bytes(), sig) {
                        return previous;
                    }
                }
            }
        }
    }
    hash
}

fn legacy_stub_hash(e: &Event) -> Option<&str> {
    let c = SignedContent::parse(&e.content)?;
    if c.v != 3
        || !matches!(c.payload, Some(Payload::Sealed(s)) if s.data.is_empty() && s.data_hash.is_empty())
    {
        return None;
    }
    let original = e.sealed_wire.as_deref()?;
    (original.len() == 64 && original.bytes().all(|b| b.is_ascii_hexdigit())).then_some(original)
}

/// What traveled: the sealed envelope's hash, or the content hash for plaintext.
pub fn wire_hash_of(e: &Event) -> String {
    e.sealed_wire.clone().unwrap_or_else(|| content_hash_of(e))
}

/// The chain hash for `e` at (`seq`, `prev`).
pub fn chain_hash(space: &str, seq: u64, prev: &str, wire: &str) -> String {
    sha256_hex(
        &serde_json::to_vec(&ChainLink {
            space,
            seq,
            prev,
            wire,
        })
        .expect("serialização"),
    )
}

/// SHA-256 of any serializable value (used to pin approvals to content).
pub fn content_hash<T: serde::Serialize>(value: &T) -> String {
    sha256_hex(&serde_json::to_vec(value).expect("serialização"))
}

/// Whether `hexkey` is an Ed25519 public key (a device or identity key).
pub fn parse_device_key(hexkey: &str) -> Option<()> {
    parse_key(hexkey).map(|_| ())
}

fn parse_key(hexkey: &str) -> Option<VerifyingKey> {
    let bytes: [u8; 32] = hex::decode(hexkey).ok()?.try_into().ok()?;
    VerifyingKey::from_bytes(&bytes).ok()
}

fn parse_sig(hexsig: &str) -> Option<Signature> {
    let bytes: [u8; 64] = hex::decode(hexsig).ok()?.try_into().ok()?;
    Some(Signature::from_bytes(&bytes))
}

/// Verifies a raw Ed25519 signature (hex key, hex signature).
pub fn verify_sig(pubkey_hex: &str, msg: &[u8], sig_hex: &str) -> bool {
    match (parse_key(pubkey_hex), parse_sig(sig_hex)) {
        (Some(k), Some(s)) => k.verify(msg, &s).is_ok(),
        _ => false,
    }
}

/// Who-said-it check: the view fields match the signed bytes, the device certificate (if
/// any) holds, and the signature covers the bytes.
pub fn verify_author(e: &Event) -> Result<(), LogError> {
    let seq = e.seq;
    let c = SignedContent::parse(&e.content).ok_or(LogError::BadContent { seq })?;
    let body_matches = match &c.payload {
        // `Sealed` describes outer bytes; a body claiming it is a forgery.
        Some(Payload::Body(b)) => {
            !matches!(e.body, EventBody::Sealed { .. }) && content::decode_body(b) == e.body
        }
        Some(Payload::Sealed(s)) => sealed_body(s) == e.body,
        None => false,
    };
    if c.space != e.space
        || c.client_id != e.client_id
        || c.author != e.author
        || c.device != e.device
        || c.at_ms != e.at_ms
        || c.seen() != e.seen
        || !body_matches
    {
        return Err(LogError::BadContent { seq });
    }
    let author = parse_key(&e.author).ok_or(LogError::BadAuthorKey { seq })?;
    let signer = match &e.device {
        Some(device) => {
            let cert = e
                .cert
                .as_deref()
                .and_then(parse_sig)
                .ok_or(LogError::BadDeviceCert { seq })?;
            author
                .verify(&device_cert_message(device), &cert)
                .map_err(|_| LogError::BadDeviceCert { seq })?;
            parse_key(device).ok_or(LogError::BadDeviceCert { seq })?
        }
        None => author,
    };
    let sig = parse_sig(&e.sig).ok_or(LogError::BadSignature { seq })?;
    signer
        .verify(
            &content::signature_message(&e.content, &content_hash_of(e)),
            &sig,
        )
        .map_err(|_| LogError::BadSignature { seq })
}

/// Checks the causal link against the events before it. Only a Space's creation and a
/// newcomer adding themselves may come from an author who had seen nothing.
pub fn verify_causal_link(before: &[Event], e: &Event) -> Result<(), LogError> {
    let seq = e.seq;
    match &e.seen {
        Some(s) => match before.get(s.seq as usize) {
            Some(target) if s.seq < seq && target.hash == s.hash => Ok(()),
            _ => Err(LogError::BrokenCausalLink { seq }),
        },
        None => match &e.body {
            EventBody::SpaceCreated { .. } => Ok(()),
            EventBody::MemberAdded { identity, .. } if *identity == e.author => Ok(()),
            _ => Err(LogError::MissingCausalLink { seq }),
        },
    }
}

/// What an event may be at its place in the log: causally linked, and never a second
/// `SpaceCreated` (which would reset the Space's privacy, ADR 0027).
fn verify_place(before: &[Event], e: &Event) -> Result<(), LogError> {
    if !before.is_empty() && matches!(e.body, EventBody::SpaceCreated { .. }) {
        return Err(LogError::Recreated { seq: e.seq });
    }
    verify_causal_link(before, e)
}

/// Full check of one event at its position: chain hash plus author signature.
pub fn verify_event(e: &Event) -> Result<(), LogError> {
    if chain_hash(&e.space, e.seq, &e.prev, &wire_hash_of(e)) != e.hash {
        return Err(LogError::BadHash { seq: e.seq });
    }
    verify_author(e)
}

#[derive(Clone, Debug)]
pub struct SpaceLog {
    space: SpaceId,
    events: Vec<Event>,
}

impl SpaceLog {
    pub fn new(space: impl Into<SpaceId>) -> Self {
        Self {
            space: space.into(),
            events: Vec::new(),
        }
    }

    /// Rebuilds a log from persisted events, verifying everything.
    pub fn from_events(space: impl Into<SpaceId>, events: Vec<Event>) -> Result<Self, LogError> {
        let log = Self {
            space: space.into(),
            events,
        };
        log.verify()?;
        Ok(log)
    }

    pub fn space(&self) -> &str {
        &self.space
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn head_hash(&self) -> &str {
        self.events
            .last()
            .map(|e| e.hash.as_str())
            .unwrap_or(GENESIS_PREV)
    }

    /// The causal link a new event signed on top of this log carries.
    pub fn head(&self) -> Option<Seen> {
        self.events.last().map(|e| Seen {
            seq: e.seq,
            hash: e.hash.clone(),
        })
    }

    /// Next sequence number.
    pub fn next_seq(&self) -> u64 {
        self.events.len() as u64
    }

    /// Sequences a signed event here (local-only Spaces: this device is the sequencer).
    pub fn sequence(&mut self, mut e: Event) -> &Event {
        e.seq = self.next_seq();
        e.prev = self.head_hash().to_string();
        e.hash = chain_hash(&self.space, e.seq, &e.prev, &wire_hash_of(&e));
        self.events.push(e);
        self.events.last().expect("acabou de entrar")
    }

    /// Signs and sequences locally in one step (local-only Spaces and tests).
    ///
    /// # Errors
    /// Returns identifier generation failures before signing or changing the log.
    pub fn append(
        &mut self,
        signer: &Signer,
        at_ms: i64,
        body: EventBody,
    ) -> Result<&Event, types::IdError> {
        let client_id = types::new_ulid(at_ms)?;
        let e = Author::root(signer.clone()).sign_event(
            &self.space,
            &client_id,
            at_ms,
            self.head(),
            body,
        );
        Ok(self.sequence(e))
    }

    /// Accepts an event sequenced elsewhere (the relay): it must be the next one, link
    /// to the head and carry a valid signature.
    pub fn accept(&mut self, e: Event) -> Result<&Event, LogError> {
        if e.space != self.space {
            return Err(LogError::WrongSpace { seq: e.seq });
        }
        if e.seq != self.next_seq() {
            return Err(LogError::BadSequence {
                expected: self.next_seq(),
                got: e.seq,
            });
        }
        if e.prev != self.head_hash() {
            return Err(LogError::BrokenChain { seq: e.seq });
        }
        verify_event(&e)?;
        verify_place(&self.events, &e)?;
        self.events.push(e);
        Ok(self.events.last().expect("acabou de entrar"))
    }

    /// Verifies sequence, hash chain and every signature.
    pub fn verify(&self) -> Result<(), LogError> {
        let mut prev = GENESIS_PREV.to_string();
        for (i, e) in self.events.iter().enumerate() {
            if e.space != self.space {
                return Err(LogError::WrongSpace { seq: e.seq });
            }
            if e.seq != i as u64 {
                return Err(LogError::BadSequence {
                    expected: i as u64,
                    got: e.seq,
                });
            }
            if e.prev != prev {
                return Err(LogError::BrokenChain { seq: e.seq });
            }
            verify_event(e)?;
            verify_place(&self.events[..i], e)?;
            prev = e.hash.clone();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

//! # roda-log
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

pub mod content;
pub mod profile;

use content::{Payload, SignedContent};
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier, VerifyingKey};
use roda_types::{
    ChainLink, Event, EventBody, IdentityId, Seen, SpaceId, EVENT_FORMAT, GENESIS_PREV,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

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
            v: EVENT_FORMAT as u32,
            space: space.to_string(),
            client_id: client_id.to_string(),
            author: self.identity.clone(),
            device: self.device.clone(),
            at_ms,
            seen: seen.map(Into::into),
            payload: Some(Payload::Sealed(sealed)),
        }
        .encode();
        let sig = self.key.sign(content::content_hash(&content).as_bytes());
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

/// The hash the author signed: over the exact content bytes, never a re-encoding.
pub fn content_hash_of(e: &Event) -> String {
    content::content_hash(&e.content)
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
        .verify(content_hash_of(e).as_bytes(), &sig)
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
    pub fn append(&mut self, signer: &Signer, at_ms: i64, body: EventBody) -> &Event {
        let client_id = roda_types::new_ulid(at_ms);
        let e = Author::root(signer.clone()).sign_event(
            &self.space,
            &client_id,
            at_ms,
            self.head(),
            body,
        );
        self.sequence(e)
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
        verify_causal_link(&self.events, &e)?;
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
            verify_causal_link(&self.events[..i], e)?;
            prev = e.hash.clone();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roda_types::{Privacy, Role, SpaceKind};

    fn sample() -> (SpaceLog, Signer, Signer) {
        let enzo = Signer::generate();
        let marina = Signer::generate();
        let mut log = SpaceLog::new("sp_paraty");
        log.append(
            &enzo,
            1,
            EventBody::SpaceCreated {
                title: "Paraty".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        );
        log.append(
            &enzo,
            2,
            EventBody::MemberAdded {
                identity: marina.id(),
                role: Role::Member,
            },
        );
        log.append(
            &marina,
            3,
            EventBody::MessagePosted {
                message: "m1".into(),
                text: "Bora pra Paraty?".into(),
                attaches: None,
            },
        );
        log.append(
            &enzo,
            4,
            EventBody::MessagePosted {
                message: "m2".into(),
                text: "Bora!".into(),
                attaches: None,
            },
        );
        (log, enzo, marina)
    }

    #[test]
    fn valid_log_verifies_and_chains() {
        let (log, _, _) = sample();
        assert_eq!(log.len(), 4);
        assert!(log.verify().is_ok());
        assert_eq!(log.events()[0].prev, GENESIS_PREV);
        for w in log.events().windows(2) {
            assert_eq!(w[1].prev, w[0].hash);
        }
        assert_eq!(log.head_hash(), log.events()[3].hash);
    }

    #[test]
    fn roundtrip_through_json_still_verifies() {
        let (log, _, _) = sample();
        let json = serde_json::to_string(log.events()).unwrap();
        let events: Vec<Event> = serde_json::from_str(&json).unwrap();
        assert!(SpaceLog::from_events("sp_paraty", events).is_ok());
    }

    /// Rewrites the signed bytes the way an attacker with the database would.
    fn rewrite(e: &Event, edit: impl FnOnce(&mut SignedContent)) -> Event {
        let mut c = SignedContent::parse(&e.content).unwrap();
        edit(&mut c);
        event_from_content(
            c.encode(),
            e.sig.clone(),
            e.cert.clone(),
            e.seq,
            e.prev.clone(),
            e.hash.clone(),
        )
        .unwrap()
    }

    fn rehash(e: &mut Event) {
        e.hash = chain_hash(&e.space, e.seq, &e.prev, &wire_hash_of(e));
    }

    #[test]
    fn tampered_content_is_detected() {
        let (log, _, _) = sample();
        let mut events = log.events().to_vec();
        events[2] = rewrite(&events[2], |c| {
            c.payload = Some(Payload::Body(content::encode_body(
                &EventBody::MessagePosted {
                    message: "m1".into(),
                    text: "Bora pra Búzios?".into(),
                    attaches: None,
                },
            )))
        });
        assert_eq!(
            SpaceLog::from_events("sp_paraty", events).unwrap_err(),
            LogError::BadHash { seq: 2 }
        );
    }

    #[test]
    fn a_view_that_disagrees_with_the_signed_bytes_is_rejected() {
        let (log, _, _) = sample();
        let mut events = log.events().to_vec();
        events[2].body = EventBody::MessagePosted {
            message: "m1".into(),
            text: "Bora pra Búzios?".into(),
            attaches: None,
        };
        assert_eq!(
            SpaceLog::from_events("sp_paraty", events).unwrap_err(),
            LogError::BadContent { seq: 2 }
        );
    }

    #[test]
    fn rehashed_tamper_fails_signature() {
        let (log, _, _) = sample();
        let mut events = log.events().to_vec();
        events[3] = rewrite(&events[3], |c| {
            c.payload = Some(Payload::Body(content::encode_body(
                &EventBody::MessagePosted {
                    message: "m2".into(),
                    text: "Não vou.".into(),
                    attaches: None,
                },
            )))
        });
        rehash(&mut events[3]);
        assert_eq!(
            SpaceLog::from_events("sp_paraty", events).unwrap_err(),
            LogError::BadSignature { seq: 3 }
        );
    }

    #[test]
    fn forged_author_fails_signature() {
        let (log, enzo, _) = sample();
        let mut events = log.events().to_vec();
        events[2] = rewrite(&events[2], |c| c.author = enzo.id());
        rehash(&mut events[2]);
        assert_eq!(
            SpaceLog::from_events("sp_paraty", events).unwrap_err(),
            LogError::BadSignature { seq: 2 }
        );
    }

    #[test]
    fn fields_and_kinds_from_newer_clients_survive_verbatim() {
        let author = Signer::generate();
        let mut log = SpaceLog::new("sp_new");
        log.append(
            &author,
            1,
            EventBody::SpaceCreated {
                title: "t".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        );
        let mut content = SignedContent {
            v: 3,
            space: "sp_new".into(),
            client_id: "c_future".into(),
            author: author.id(),
            device: None,
            at_ms: 2,
            seen: log.head().as_ref().map(Into::into),
            payload: Some(Payload::Body(
                br#"{"PollOpened":{"question":"Praia?"}}"#.to_vec(),
            )),
        }
        .encode();
        content.extend_from_slice(&[0xA2, 0x06, 0x03, b'n', b'e', b'w']);
        let sig = author.sign(content::content_hash(&content).as_bytes());
        let e = event_from_content(content.clone(), sig, None, 0, String::new(), String::new())
            .unwrap();
        assert_eq!(
            e.body,
            EventBody::Unsupported {
                kind: "PollOpened".into()
            }
        );
        let sequenced = log.sequence(e).clone();
        assert_eq!(sequenced.content, content);
        let json = serde_json::to_string(log.events()).unwrap();
        assert!(SpaceLog::from_events("sp_new", serde_json::from_str(&json).unwrap()).is_ok());
    }

    #[test]
    fn a_sequencer_cannot_move_an_event_before_what_its_author_saw() {
        let (log, _, marina) = sample();
        let mut relay = SpaceLog::new("sp_paraty");
        for e in &log.events()[..2] {
            relay.accept(e.clone()).unwrap();
        }
        let reply = Author::root(marina.clone()).sign_event(
            "sp_paraty",
            "late",
            9,
            relay.head(),
            EventBody::MessagePosted {
                message: "m9".into(),
                text: "Concordo".into(),
                attaches: None,
            },
        );
        let mut forked = SpaceLog::new("sp_paraty");
        forked.accept(log.events()[0].clone()).unwrap();
        let mut moved = reply.clone();
        moved.seq = 1;
        moved.prev = forked.head_hash().to_string();
        rehash(&mut moved);
        assert_eq!(
            forked.accept(moved).unwrap_err(),
            LogError::BrokenCausalLink { seq: 1 }
        );

        let mut placed = reply;
        placed.seq = 2;
        placed.prev = relay.head_hash().to_string();
        rehash(&mut placed);
        assert!(relay.accept(placed).is_ok());
    }

    #[test]
    fn words_signed_on_one_history_do_not_verify_on_another() {
        let enzo = Signer::generate();
        let mut real = SpaceLog::new("sp_fork");
        real.append(
            &enzo,
            1,
            EventBody::SpaceCreated {
                title: "Real".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        );
        real.append(
            &enzo,
            2,
            EventBody::MessagePosted {
                message: "a".into(),
                text: "Vamos sábado".into(),
                attaches: None,
            },
        );
        let mut fake = SpaceLog::new("sp_fork");
        fake.accept(real.events()[0].clone()).unwrap();
        fake.append(
            &enzo,
            3,
            EventBody::MessagePosted {
                message: "b".into(),
                text: "Cancelado".into(),
                attaches: None,
            },
        );
        let answer = Author::root(enzo.clone()).sign_event(
            "sp_fork",
            "ans",
            4,
            real.head(),
            EventBody::MessagePosted {
                message: "c".into(),
                text: "Confirmado".into(),
                attaches: None,
            },
        );
        let mut shown = answer;
        shown.seq = fake.next_seq();
        shown.prev = fake.head_hash().to_string();
        rehash(&mut shown);
        assert_eq!(
            fake.accept(shown).unwrap_err(),
            LogError::BrokenCausalLink { seq: 2 }
        );
    }

    #[test]
    fn only_creation_and_self_join_may_skip_the_causal_link() {
        let (log, enzo, _) = sample();
        let mut client = SpaceLog::new("sp_paraty");
        for e in log.events() {
            client.accept(e.clone()).unwrap();
        }
        let blind = Author::root(enzo).sign_event(
            "sp_paraty",
            "blind",
            9,
            None,
            EventBody::MessagePosted {
                message: "m9".into(),
                text: "?".into(),
                attaches: None,
            },
        );
        let mut e = blind;
        e.seq = client.next_seq();
        e.prev = client.head_hash().to_string();
        rehash(&mut e);
        assert_eq!(
            client.accept(e).unwrap_err(),
            LogError::MissingCausalLink { seq: 4 }
        );

        let newcomer = Signer::generate();
        let join = Author::root(newcomer.clone()).sign_event(
            "sp_paraty",
            "join",
            10,
            None,
            EventBody::MemberAdded {
                identity: newcomer.id(),
                role: Role::Member,
            },
        );
        let mut j = join;
        j.seq = client.next_seq();
        j.prev = client.head_hash().to_string();
        rehash(&mut j);
        assert!(client.accept(j).is_ok());
    }

    #[test]
    fn removed_or_reordered_events_are_detected() {
        let (log, _, _) = sample();
        let mut removed = log.events().to_vec();
        removed.remove(1);
        assert_eq!(
            SpaceLog::from_events("sp_paraty", removed).unwrap_err(),
            LogError::BadSequence {
                expected: 1,
                got: 2
            }
        );

        let mut reordered = log.events().to_vec();
        reordered.swap(2, 3);
        assert!(SpaceLog::from_events("sp_paraty", reordered).is_err());

        // Renumerar depois de remover ainda quebra a cadeia.
        let mut renumbered = log.events().to_vec();
        renumbered.remove(1);
        for (i, e) in renumbered.iter_mut().enumerate() {
            e.seq = i as u64;
        }
        assert!(matches!(
            SpaceLog::from_events("sp_paraty", renumbered).unwrap_err(),
            LogError::BrokenChain { .. } | LogError::BadHash { .. }
        ));
    }

    #[test]
    fn events_from_another_space_are_rejected() {
        let (log, _, _) = sample();
        assert_eq!(
            SpaceLog::from_events("sp_outro", log.events().to_vec()).unwrap_err(),
            LogError::WrongSpace { seq: 0 }
        );
    }

    #[test]
    fn device_keys_sign_for_their_identity() {
        let root = Signer::generate();
        let phone = Author::device(&root, Signer::generate());
        let mut log = SpaceLog::new("sp_dm");
        let e = phone.sign_event(
            "sp_dm",
            "c1",
            1,
            None,
            EventBody::SpaceCreated {
                title: "t".into(),
                kind: SpaceKind::Direct,
                privacy: Privacy::Closed,
            },
        );
        assert_eq!(e.author, root.id());
        log.sequence(e);
        assert!(log.verify().is_ok());

        // A device certified by someone else can't speak for this identity.
        let mallory = Signer::generate();
        let fake = Author::certified(
            root.id(),
            Signer::generate(),
            mallory.sign(b"zoen-device-v1:x"),
        );
        let e = fake.sign_event(
            "sp_dm",
            "c2",
            2,
            log.head(),
            EventBody::MessagePosted {
                message: "m".into(),
                text: "oi".into(),
                attaches: None,
            },
        );
        log.sequence(e);
        assert_eq!(
            log.verify().unwrap_err(),
            LogError::BadDeviceCert { seq: 1 }
        );
    }

    #[test]
    fn relay_sequenced_events_are_accepted_in_order_only() {
        let root = Signer::generate();
        let me = Author::root(root.clone());
        let mut relay = SpaceLog::new("sp_g");
        let a = relay
            .sequence(me.sign_event(
                "sp_g",
                "a",
                1,
                None,
                EventBody::SpaceCreated {
                    title: "g".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::Closed,
                },
            ))
            .clone();
        let b = relay
            .sequence(me.sign_event(
                "sp_g",
                "b",
                2,
                relay.head(),
                EventBody::MessagePosted {
                    message: "m".into(),
                    text: "x".into(),
                    attaches: None,
                },
            ))
            .clone();
        let mut client = SpaceLog::new("sp_g");
        assert_eq!(
            client.accept(b.clone()).unwrap_err(),
            LogError::BadSequence {
                expected: 0,
                got: 1
            }
        );
        client.accept(a).unwrap();
        client.accept(b).unwrap();
        assert_eq!(client.head_hash(), relay.head_hash());
    }

    #[test]
    fn signer_roundtrips_secret() {
        let s = Signer::generate();
        let again = Signer::from_secret(&s.secret());
        assert_eq!(s.id(), again.id());
        assert_eq!(s.id().len(), 64);
    }

    #[test]
    fn json_hash_is_stable_and_sensitive() {
        assert_eq!(content_hash(&("a", 1)), content_hash(&("a", 1)));
        assert_ne!(content_hash(&("a", 1)), content_hash(&("a", 2)));
    }
}

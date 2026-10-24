//! Linking a second device (ADR 0045): the code a new device shows, the boxes an existing
//! device seals to it, and the history it hands over.
//!
//! Everything between the two devices is HPKE (RFC 9180) in PSK mode: DHKEM(X25519,
//! HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305, to the new device's key from its code, with
//! the code's one-time secret as the PSK. Only someone who saw the code can seal to it, and
//! only the device that showed it can open what was sealed. The relay carries bytes.

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use hpke_rs::{
    hpke_types::{AeadAlgorithm, KdfAlgorithm, KemAlgorithm},
    rustcrypto::HpkeRustCrypto,
    Hpke, HpkePrivateKey, HpkePublicKey, Mode,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use types::{Event, EventBody};

const PREFIX: &str = "zoen-link:1:";
const PSK_ID: &[u8] = b"zoen-link-v1";
const ENC_LEN: usize = 32;

fn hpke() -> Hpke<HpkeRustCrypto> {
    Hpke::new(
        Mode::Psk,
        KemAlgorithm::DhKem25519,
        KdfAlgorithm::HkdfSha256,
        AeadAlgorithm::ChaCha20Poly1305,
    )
}

pub fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("system randomness");
    b
}

/// An X25519 key pair for HPKE: (secret, public).
pub fn key_pair() -> (Vec<u8>, Vec<u8>) {
    let kp = hpke()
        .derive_key_pair(&random::<32>())
        .expect("X25519 key pair");
    let (sk, pk) = kp.into_keys();
    (sk.as_slice().to_vec(), pk.as_slice().to_vec())
}

/// What the new device shows (as a QR code): its device key, its HPKE key, and a one-time
/// secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkCode {
    pub device: String,
    pub hpke: Vec<u8>,
    pub secret: [u8; 32],
}

impl LinkCode {
    pub fn encode(&self) -> String {
        format!(
            "{PREFIX}{}{}{}",
            self.device,
            hex::encode(&self.hpke),
            hex::encode(self.secret)
        )
    }

    pub fn parse(code: &str) -> Option<LinkCode> {
        let hexed = code.trim().strip_prefix(PREFIX)?;
        if hexed.len() != 192 || !hexed.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let device = hexed[..64].to_lowercase();
        event_log::parse_device_key(&device)?;
        let hpke = hex::decode(&hexed[64..128]).ok()?;
        let secret = hex::decode(&hexed[128..]).ok()?.try_into().ok()?;
        Some(LinkCode {
            device,
            hpke,
            secret,
        })
    }

    /// Six digits both screens show, so the person can see they scanned the right code.
    pub fn check_digits(&self) -> String {
        let h = Sha256::digest(self.encode().as_bytes());
        let n = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) % 1_000_000;
        format!("{n:06}")
    }
}

/// Where the relay keeps a box for this link: a hash of the secret, never the secret.
pub fn box_id(secret: &[u8; 32], label: &str) -> String {
    hex::encode(
        Sha256::new()
            .chain_update(b"zoen-link-box-v1\0")
            .chain_update(secret)
            .chain_update(label.as_bytes())
            .finalize(),
    )
}

/// Seals `plain` to `to` (an HPKE public key), bound to the link secret and `context`.
pub fn seal(to: &[u8], secret: &[u8; 32], context: &str, plain: &[u8]) -> Option<Vec<u8>> {
    let pk = HpkePublicKey::new(to.to_vec());
    let info = format!("zoen-link-v1:{context}");
    let (enc, ct) = hpke()
        .seal(
            &pk,
            info.as_bytes(),
            b"",
            plain,
            Some(secret),
            Some(PSK_ID),
            None,
        )
        .ok()?;
    let mut out = enc;
    out.extend(ct);
    Some(out)
}

pub fn open(sk: &[u8], secret: &[u8; 32], context: &str, sealed: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < ENC_LEN {
        return None;
    }
    let sk = HpkePrivateKey::new(sk.to_vec());
    let info = format!("zoen-link-v1:{context}");
    hpke()
        .open(
            &sealed[..ENC_LEN],
            &sk,
            info.as_bytes(),
            b"",
            &sealed[ENC_LEN..],
            Some(secret),
            Some(PSK_ID),
            None,
        )
        .ok()
}

/// What the existing device hands the new one: the account (the identity key moves, see
/// ADR 0045), a certificate for the new device, and how to reach the existing one.
#[derive(Clone, Serialize, Deserialize)]
pub struct IdentityBox {
    pub identity_secret: String,
    pub agreement_secret: String,
    pub cert: String,
    pub name: String,
    pub handle: String,
    pub relay: String,
    /// The device that linked this one: it answers requests for older pages.
    pub primary: String,
    pub primary_hpke: String,
    /// The encrypted-profile key (hex), the version it opens from, the newest version seen
    /// and its fields (hex): so the new device keeps the profile others already read.
    pub profile_key: String,
    pub profile_key_version: u64,
    pub profile_seen_version: u64,
    pub profile_fields: String,
}

/// Where the first history bundle is: chunks on the relay, encrypted with `key`.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct HistoryBox {
    pub transfer: String,
    pub key: String,
    pub chunks: Vec<String>,
}

/// The first bundle: the recent window of every chat, as the existing device opened it.
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Bundle {
    pub events: Vec<Event>,
}

/// One chunk of the bundle, sealed with the bundle key; the index is the nonce.
pub fn seal_chunk(key: &[u8; 32], n: u32, plain: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; 12];
    nonce[8..].copy_from_slice(&n.to_be_bytes());
    ChaCha20Poly1305::new(key.into())
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: plain,
                aad: b"zoen-history-v1",
            },
        )
        .expect("chunk sealing")
}

pub fn open_chunk(key: &[u8; 32], n: u32, sealed: &[u8]) -> Option<Vec<u8>> {
    let mut nonce = [0u8; 12];
    nonce[8..].copy_from_slice(&n.to_be_bytes());
    ChaCha20Poly1305::new(key.into())
        .decrypt(
            &Nonce::from(nonce),
            Payload {
                msg: sealed,
                aad: b"zoen-history-v1",
            },
        )
        .ok()
}

/// What two linked devices of one person say to each other through the relay.
#[derive(Clone, Serialize, Deserialize)]
pub enum DeviceMsg {
    /// Content-free hint: a first-history manifest was acknowledged by the relay.
    /// This does not carry or authenticate completion; receivers still fetch and verify.
    HistoryReady,
    /// Up to `limit` opened messages of `space` before `before`.
    PageRequest {
        req: String,
        space: String,
        before: u64,
        limit: u32,
    },
    Page {
        req: String,
        space: String,
        events: Vec<Event>,
        /// Older ones are left.
        more: bool,
    },
}

/// What a device keeps about another device of the same person it linked with.
#[derive(Clone, Serialize, Deserialize)]
pub struct Peer {
    pub device: String,
    pub their_hpke: String,
    pub my_hpke_secret: String,
    pub secret: String,
}

impl Peer {
    pub fn secret(&self) -> Option<[u8; 32]> {
        hex::decode(&self.secret).ok()?.try_into().ok()
    }

    pub fn seal(&self, msg: &DeviceMsg) -> Option<Vec<u8>> {
        let to = hex::decode(&self.their_hpke).ok()?;
        let plain = serde_json::to_vec(msg).ok()?;
        seal(&to, &self.secret()?, "device-message", &plain)
    }

    pub fn open(&self, sealed: &[u8]) -> Option<DeviceMsg> {
        let sk = hex::decode(&self.my_hpke_secret).ok()?;
        let plain = open(&sk, &self.secret()?, "device-message", sealed)?;
        serde_json::from_slice(&plain).ok()
    }
}

pub const VAULT_PENDING: &str = "link.pending";

/// What a device waiting to be linked keeps until its box arrives.
#[derive(Clone, Serialize, Deserialize)]
pub struct Pending {
    pub device_secret: String,
    pub hpke_secret: String,
    pub hpke_public: String,
    pub secret: String,
    pub relay: String,
}

/// Replace an authenticated outer record only with the same signed inner header.
/// Chain placement always comes from the receiver's already verified relay log.
pub fn opened_history_event(local: &Event, mut opened: Event) -> Option<Event> {
    if !matches!(&local.body, EventBody::Sealed { kind } if kind == "Application")
        || local.sealed_wire.is_some()
        || opened.sealed_wire.as_deref() != Some(event_log::content_hash_of(local).as_str())
        || opened.seq != local.seq
        || opened.space != local.space
        || opened.client_id != local.client_id
        || opened.author != local.author
        || opened.device != local.device
        || opened.at_ms != local.at_ms
        || opened.seen != local.seen
        || matches!(
            opened.body,
            EventBody::SpaceCreated { .. }
                | EventBody::MemberAdded { .. }
                | EventBody::MemberRemoved { .. }
                | EventBody::ProfileKeyShared { .. }
                | EventBody::SpaceEncrypted
                | EventBody::Checkpoint { .. }
                | EventBody::DeviceJoining { .. }
                | EventBody::Sealed { .. }
        )
        || event_log::verify_author(&opened).is_err()
    {
        return None;
    }
    opened.prev = local.prev.clone();
    opened.hash = local.hash.clone();
    Some(opened)
}

#[cfg(test)]
mod tests {
    use super::*;
    use event_log::content::{Sealed, SealedKind};
    use event_log::{Author, Signer, SpaceLog};
    use types::{Privacy, Seen, SpaceKind};

    #[test]
    fn hpke_links_bind_secret_recipient_and_context_without_changing_wire() {
        let (sk, pk) = key_pair();
        let (wrong_sk, _) = key_pair();
        let secret = [7; 32];
        let sealed = seal(&pk, &secret, "history", b"private").unwrap();
        assert_eq!(open(&sk, &secret, "history", &sealed).unwrap(), b"private");
        assert!(open(&wrong_sk, &secret, "history", &sealed).is_none());
        assert!(open(&sk, &[8; 32], "history", &sealed).is_none());
        assert!(open(&sk, &secret, "identity", &sealed).is_none());
        let code = LinkCode {
            device: Signer::from_secret(&[2; 32]).id(),
            hpke: pk,
            secret,
        };
        assert_eq!(LinkCode::parse(&code.encode()), Some(code.clone()));
        assert_eq!(code.check_digits().len(), 6);
        assert_ne!(box_id(&secret, "history"), box_id(&secret, "identity"));
    }

    #[test]
    fn history_chunks_bind_each_index_and_refuse_tampered_ciphertext() {
        let key = [9; 32];
        let mut sealed = seal_chunk(&key, 4, b"actual bytes");
        assert_eq!(open_chunk(&key, 4, &sealed).unwrap(), b"actual bytes");
        assert!(open_chunk(&key, 3, &sealed).is_none());
        assert!(open_chunk(&[8; 32], 4, &sealed).is_none());
        *sealed.last_mut().unwrap() ^= 1;
        assert!(open_chunk(&key, 4, &sealed).is_none());
    }

    fn fixture() -> Result<(Author, Event, Event), Box<dyn std::error::Error>> {
        let identity = Signer::from_secret(&[11; 32]);
        let author = Author::device(&identity, Signer::from_secret(&[12; 32]));
        let mut log = SpaceLog::new("sp_history");
        log.append(
            &identity,
            1,
            EventBody::SpaceCreated {
                title: "Private".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        )?;
        let opened = author.sign_event(
            log.space(),
            "msg_history",
            2,
            log.head(),
            EventBody::MessagePosted {
                message: "msg_history".into(),
                text: "Actual history".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )?;
        let (content, sig) = author.sign_sealed(
            log.space(),
            "msg_history",
            2,
            log.head().as_ref(),
            Sealed::new(SealedKind::Application, 1, vec![1, 2, 3]),
        );
        let outer = event_log::event_from_content(
            content,
            sig,
            author.cert.clone(),
            0,
            String::new(),
            String::new(),
        )
        .unwrap();
        let local = log.sequence(outer)?.clone();
        let mut opened = opened;
        opened.seq = local.seq;
        opened.sealed_wire = Some(event_log::content_hash_of(&local));
        Ok((author, local, opened))
    }

    #[test]
    fn opened_history_requires_every_signed_outer_header_and_preserves_local_chain(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (author, local, opened) = fixture()?;
        let actual = opened_history_event(&local, opened.clone()).unwrap();
        assert_eq!(actual.hash, local.hash);
        assert_eq!(actual.prev, local.prev);
        event_log::verify_author(&actual).unwrap();
        for changed in [
            author.sign_event(
                &local.space,
                &local.client_id,
                3,
                local.seen.clone(),
                opened.body.clone(),
            )?,
            author.sign_event(
                &local.space,
                "different",
                2,
                local.seen.clone(),
                opened.body.clone(),
            )?,
            author.sign_event(
                &local.space,
                &local.client_id,
                2,
                Some(Seen {
                    seq: 0,
                    hash: "00".repeat(32),
                }),
                opened.body.clone(),
            )?,
            author.sign_event(
                &local.space,
                &local.client_id,
                2,
                local.seen.clone(),
                EventBody::SpaceEncrypted,
            )?,
        ] {
            let mut changed = changed;
            changed.seq = local.seq;
            changed.sealed_wire = opened.sealed_wire.clone();
            assert!(opened_history_event(&local, changed).is_none());
        }
        let mut wrong_seq = opened.clone();
        wrong_seq.seq += 1;
        assert!(opened_history_event(&local, wrong_seq).is_none());
        let mut wrong_device = opened;
        wrong_device.device = None;
        assert!(opened_history_event(&local, wrong_device).is_none());
        Ok(())
    }
}

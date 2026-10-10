//! Encrypted public MLS context for recovery after all device group states are lost.

use std::collections::BTreeMap;

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use hkdf::Hkdf;
use prost::Message;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::profile::AgreementKey;

pub const MAX_CONTEXT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RECIPIENTS: usize = 5000;
const NONCE_BYTES: usize = 24;
// Identity, ephemeral public key, nonce, authenticated 32-byte content key.
const BOX_BYTES: usize = 32 + 32 + NONCE_BYTES + 48;
pub const MAX_RECOVERY_BLOB_BYTES: usize = MAX_CONTEXT_BYTES + MAX_RECIPIENTS * BOX_BYTES + 128;
const DOMAIN: &[u8] = b"zoen-mls-recovery-context-v1\0";

#[derive(Clone, PartialEq, Message)]
struct EncryptedContext {
    #[prost(uint32, tag = "1")]
    version: u32,
    #[prost(bytes = "vec", tag = "2")]
    nonce: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    ciphertext: Vec<u8>,
    // Fixed-size sorted records bound decoder allocations independently of member count.
    #[prost(bytes = "vec", tag = "4")]
    boxes: Vec<u8>,
}

/// A signed commit points to the complete content-addressed encrypted context blob.
#[derive(Clone, PartialEq, Message)]
pub struct RecoveryRef {
    #[prost(uint32, tag = "1")]
    pub version: u32,
    #[prost(uint64, tag = "2")]
    pub epoch: u64,
    #[prost(string, tag = "3")]
    pub blob: String,
}

impl RecoveryRef {
    pub fn encode(&self) -> Vec<u8> {
        self.encode_to_vec()
    }

    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() > 128 {
            return None;
        }
        let r = Self::decode(data).ok()?;
        (r.version == 1
            && r.epoch > 0
            && hex32(&r.blob).is_some()
            && r.blob
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        .then_some(r)
    }
}

fn hex32(s: &str) -> Option<[u8; 32]> {
    (s.len() == 64).then_some(())?;
    hex::decode(s).ok()?.try_into().ok()
}

fn random<const N: usize>() -> Option<[u8; N]> {
    let mut b = [0; N];
    getrandom::getrandom(&mut b).ok()?;
    Some(b)
}

fn context(space: &str, epoch: u64, device: &str) -> Option<Vec<u8>> {
    let device = hex32(device)?;
    if space.is_empty() || space.len() > 128 || epoch == 0 {
        return None;
    }
    let mut aad = DOMAIN.to_vec();
    aad.extend_from_slice(&(space.len() as u32).to_be_bytes());
    aad.extend_from_slice(space.as_bytes());
    aad.extend_from_slice(&epoch.to_be_bytes());
    aad.extend_from_slice(&device);
    Some(aad)
}

fn wrapping_key(
    shared: &[u8; 32],
    ephemeral: &[u8; 32],
    recipient: &[u8; 32],
    aad: &[u8],
) -> Option<XChaCha20Poly1305> {
    let mut salt = [0; 64];
    salt[..32].copy_from_slice(ephemeral);
    salt[32..].copy_from_slice(recipient);
    let mut key = [0; 32];
    Hkdf::<Sha256>::new(Some(&salt), shared)
        .expand_multi_info(&[DOMAIN, b"key-box\0", aad], &mut key)
        .ok()?;
    XChaCha20Poly1305::new_from_slice(&key).ok()
}

/// Recipient identity IDs and their verified recoverable X25519 public keys.
pub fn seal(
    space: &str,
    epoch: u64,
    publishing_device: &str,
    public_context: &[u8],
    recipients: &[(&str, &str)],
) -> Option<Vec<u8>> {
    if public_context.is_empty()
        || public_context.len() > MAX_CONTEXT_BYTES
        || recipients.is_empty()
        || recipients.len() > MAX_RECIPIENTS
    {
        return None;
    }
    let aad = context(space, epoch, publishing_device)?;
    let mut ordered = BTreeMap::new();
    for (identity, public) in recipients {
        if ordered.insert(hex32(identity)?, hex32(public)?).is_some() {
            return None;
        }
    }
    let key = random::<32>()?;
    let nonce = random::<NONCE_BYTES>()?;
    let ciphertext = XChaCha20Poly1305::new_from_slice(&key)
        .ok()?
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: public_context,
                aad: &aad,
            },
        )
        .ok()?;
    let mut boxes = Vec::with_capacity(ordered.len() * BOX_BYTES);
    for (identity, recipient) in ordered {
        let ephemeral = StaticSecret::from(random::<32>()?);
        let public = PublicKey::from(&ephemeral).to_bytes();
        let shared = ephemeral.diffie_hellman(&PublicKey::from(recipient));
        if !shared.was_contributory() {
            return None;
        }
        let mut box_aad = aad.clone();
        box_aad.extend_from_slice(&identity);
        let nonce = random::<NONCE_BYTES>()?;
        let wrapped = wrapping_key(shared.as_bytes(), &public, &recipient, &box_aad)?
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &key,
                    aad: &box_aad,
                },
            )
            .ok()?;
        boxes.extend_from_slice(&identity);
        boxes.extend_from_slice(&public);
        boxes.extend_from_slice(&nonce);
        boxes.extend_from_slice(&wrapped);
    }
    Some(
        EncryptedContext {
            version: 1,
            nonce: nonce.to_vec(),
            ciphertext,
            boxes,
        }
        .encode_to_vec(),
    )
}

pub fn open(
    space: &str,
    epoch: u64,
    publishing_device: &str,
    identity: &str,
    agreement: &AgreementKey,
    blob: &[u8],
) -> Option<Vec<u8>> {
    if blob.len() > MAX_RECOVERY_BLOB_BYTES {
        return None;
    }
    let aad = context(space, epoch, publishing_device)?;
    let identity = hex32(identity)?;
    let blob = EncryptedContext::decode(blob).ok()?;
    if blob.version != 1
        || blob.nonce.len() != NONCE_BYTES
        || blob.ciphertext.len() <= 16
        || blob.ciphertext.len() > MAX_CONTEXT_BYTES + 16
        || blob.boxes.is_empty()
        || blob.boxes.len() % BOX_BYTES != 0
        || blob.boxes.len() / BOX_BYTES > MAX_RECIPIENTS
    {
        return None;
    }
    let mut previous: Option<&[u8]> = None;
    let mut mine = None;
    for b in blob.boxes.as_chunks::<BOX_BYTES>().0 {
        if previous.is_some_and(|p| p >= &b[..32]) {
            return None;
        }
        previous = Some(&b[..32]);
        if b[..32] == identity {
            mine = Some(b);
        }
    }
    let b = mine?;
    let public: [u8; 32] = b[32..64].try_into().ok()?;
    let nonce: [u8; NONCE_BYTES] = b[64..88].try_into().ok()?;
    let secret = StaticSecret::from(agreement.secret());
    let recipient = PublicKey::from(&secret).to_bytes();
    let shared = secret.diffie_hellman(&PublicKey::from(public));
    if !shared.was_contributory() {
        return None;
    }
    let mut box_aad = aad.clone();
    box_aad.extend_from_slice(&identity);
    let key = wrapping_key(shared.as_bytes(), &public, &recipient, &box_aad)?
        .decrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &b[88..],
                aad: &box_aad,
            },
        )
        .ok()?;
    let nonce: [u8; NONCE_BYTES] = blob.nonce.try_into().ok()?;
    XChaCha20Poly1305::new_from_slice(&key)
        .ok()?
        .decrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &blob.ciphertext,
                aad: &aad,
            },
        )
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_named_recoverable_keys_open_the_bound_context() {
        let ana = AgreementKey::generate();
        let bruno = AgreementKey::generate();
        let a = "aa".repeat(32);
        let b = "bb".repeat(32);
        let device = "dd".repeat(32);
        let raw = b"public MLS tree whose device metadata stays encrypted";
        let blob = seal(
            "sp_room",
            3,
            &device,
            raw,
            &[(&a, &ana.public_hex()), (&b, &bruno.public_hex())],
        )
        .unwrap();
        assert!(!blob.windows(raw.len()).any(|w| w == raw));
        for (id, key) in [(&a, &ana), (&b, &bruno)] {
            assert_eq!(
                open("sp_room", 3, &device, id, key, &blob).as_deref(),
                Some(raw.as_slice())
            );
        }
        assert!(open("sp_else", 3, &device, &a, &ana, &blob).is_none());
        assert!(open("sp_room", 4, &device, &a, &ana, &blob).is_none());
        assert!(open("sp_room", 3, &a, &a, &ana, &blob).is_none());
        assert!(open("sp_room", 3, &device, &a, &bruno, &blob).is_none());
        assert!(open("sp_room", 3, &device, &device, &ana, &blob).is_none());
        let mut changed = EncryptedContext::decode(blob.as_slice()).unwrap();
        changed.ciphertext[0] ^= 1;
        assert!(open("sp_room", 3, &device, &a, &ana, &changed.encode_to_vec()).is_none());
    }

    #[test]
    fn invalid_keys_duplicate_recipients_and_malformed_records_are_refused() {
        let key = AgreementKey::generate();
        let id = "ab".repeat(32);
        let public = key.public_hex();
        assert!(seal("sp_room", 1, &id, b"tree", &[(&id, &"00".repeat(32))]).is_none());
        assert!(seal(
            "sp_room",
            1,
            &id,
            b"tree",
            &[(&id, &public), (&id, &public)]
        )
        .is_none());
        let blob = seal("sp_room", 1, &id, b"tree", &[(&id, &public)]).unwrap();
        let mut parsed = EncryptedContext::decode(blob.as_slice()).unwrap();
        parsed.boxes.pop();
        assert!(open("sp_room", 1, &id, &id, &key, &parsed.encode_to_vec()).is_none());
        parsed.boxes = vec![0; (MAX_RECIPIENTS + 1) * BOX_BYTES];
        assert!(open("sp_room", 1, &id, &id, &key, &parsed.encode_to_vec()).is_none());
        assert!(RecoveryRef::parse(
            &RecoveryRef {
                version: 1,
                epoch: 1,
                blob: id
            }
            .encode_to_vec()
        )
        .is_some());
    }
}

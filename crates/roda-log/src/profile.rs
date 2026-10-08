//! Encrypted profiles (ADR 0016).
//!
//! Each person has a random 32-byte **profile key**. Their name, bio and photo reference
//! are encrypted under it and the relay stores only that ciphertext, a version and the
//! author's signature. The key reaches contacts and Space members as **shares**: sealed to
//! each recipient's X25519 **agreement key** inside a signed event of a Space they share
//! (an MLS application message once M2 lands). Whoever holds the key reads the profile;
//! everyone else sees the @handle the directory already has. Blocking someone rotates the key.

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use hkdf::Hkdf;
use prost::Message;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::{device_cert_message, verify_sig, Author};

const PROFILE_INFO: &[u8] = b"zoen-profile-v1";
const SHARE_INFO: &[u8] = b"zoen-profile-key-share-v1";
const AGREEMENT_DOMAIN: &[u8] = b"zoen-agreement-key-v1\0";
const UPLOAD_DOMAIN: &[u8] = b"zoen-profile-upload-v1\0";
const NONCE: usize = 24;
/// Biggest profile ciphertext a relay accepts (fields are short; the photo is a blob).
pub const MAX_CIPHERTEXT: usize = 8 * 1024;

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("system randomness");
    b
}

/// The fields a profile carries, as the protobuf the author encrypts. Unknown fields
/// from newer clients are dropped on re-encode, so only the owner ever re-encodes.
#[derive(Clone, PartialEq, Message)]
pub struct ProfileFields {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub bio: String,
    #[prost(message, optional, tag = "3")]
    pub photo: Option<ProfilePhoto>,
}

/// An encrypted media blob (ADR 0007): what to download, how to open it, what it must be.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct ProfilePhoto {
    /// sha256 of the plaintext image.
    #[prost(string, tag = "1")]
    pub sha256: String,
    /// sha256 of the ciphertext on the relay's blob store.
    #[prost(string, tag = "2")]
    pub blob: String,
    /// The blob's own XChaCha20-Poly1305 key, hex.
    #[prost(string, tag = "3")]
    pub key: String,
    #[prost(string, tag = "4")]
    pub mime: String,
}

impl ProfileFields {
    pub fn to_bytes(&self) -> Vec<u8> {
        self.encode_to_vec()
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        Self::decode(bytes).ok()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProfileKey(pub [u8; 32]);

impl std::fmt::Debug for ProfileKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProfileKey(…)")
    }
}

impl ProfileKey {
    pub fn generate() -> Self {
        Self(random())
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        hex::decode(s).ok()?.try_into().ok().map(Self)
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        let mut k = [0u8; 32];
        Hkdf::<Sha256>::new(None, &self.0)
            .expand(PROFILE_INFO, &mut k)
            .expect("32 bytes");
        XChaCha20Poly1305::new_from_slice(&k).expect("32-byte key")
    }
}

/// Binds a ciphertext to whose profile it is and which version, so a relay can't swap
/// profiles between people or serve version 3's bytes as version 9.
fn profile_aad(identity: &str, version: u64) -> Vec<u8> {
    let mut aad = PROFILE_INFO.to_vec();
    aad.push(0);
    aad.extend_from_slice(identity.as_bytes());
    aad.extend_from_slice(&version.to_be_bytes());
    aad
}

/// `nonce ‖ ciphertext+tag`.
pub fn seal_profile(
    key: &ProfileKey,
    identity: &str,
    version: u64,
    fields: &ProfileFields,
) -> Vec<u8> {
    let nonce = random::<NONCE>();
    let ct = key
        .cipher()
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &fields.encode_to_vec(),
                aad: &profile_aad(identity, version),
            },
        )
        .expect("encrypt");
    [nonce.as_slice(), &ct].concat()
}

pub fn open_profile(
    key: &ProfileKey,
    identity: &str,
    version: u64,
    sealed: &[u8],
) -> Option<ProfileFields> {
    if sealed.len() < NONCE + 16 {
        return None;
    }
    let nonce: [u8; NONCE] = sealed[..NONCE].try_into().ok()?;
    let plain = key
        .cipher()
        .decrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &sealed[NONCE..],
                aad: &profile_aad(identity, version),
            },
        )
        .ok()?;
    ProfileFields::decode(plain.as_slice()).ok()
}

/// A statement signed by one of an identity's devices: the device key, its signature and
/// the identity's certificate over the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSigned {
    pub device: String,
    pub sig: String,
    pub cert: String,
}

fn sign_as(author: &Author, msg: &[u8]) -> Option<DeviceSigned> {
    Some(DeviceSigned {
        device: author.device.clone()?,
        sig: author.key.sign(msg),
        cert: author.cert.clone()?,
    })
}

fn verify_as(identity: &str, msg: &[u8], s: &DeviceSigned) -> bool {
    verify_sig(identity, &device_cert_message(&s.device), &s.cert)
        && verify_sig(&s.device, msg, &s.sig)
}

fn upload_message(identity: &str, version: u64, ciphertext: &[u8]) -> Vec<u8> {
    let mut m = UPLOAD_DOMAIN.to_vec();
    m.extend_from_slice(identity.as_bytes());
    m.push(0);
    m.extend_from_slice(&version.to_be_bytes());
    m.extend_from_slice(&Sha256::digest(ciphertext));
    m
}

/// Signs a profile upload; `None` for an author without a certified device.
pub fn sign_upload(author: &Author, version: u64, ciphertext: &[u8]) -> Option<DeviceSigned> {
    sign_as(
        author,
        &upload_message(&author.identity, version, ciphertext),
    )
}

pub fn verify_upload(identity: &str, version: u64, ciphertext: &[u8], s: &DeviceSigned) -> bool {
    verify_as(identity, &upload_message(identity, version, ciphertext), s)
}

/// An identity's X25519 key for receiving sealed profile keys.
#[derive(Clone)]
pub struct AgreementKey(StaticSecret);

impl AgreementKey {
    pub fn generate() -> Self {
        Self(StaticSecret::from(random::<32>()))
    }

    pub fn from_secret(secret: [u8; 32]) -> Self {
        Self(StaticSecret::from(secret))
    }

    pub fn secret(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    pub fn public_hex(&self) -> String {
        hex::encode(PublicKey::from(&self.0).as_bytes())
    }
}

fn agreement_message(identity: &str, public: &str) -> Vec<u8> {
    [
        AGREEMENT_DOMAIN,
        identity.as_bytes(),
        b"\0",
        public.as_bytes(),
    ]
    .concat()
}

/// Publishes `public` as `author`'s agreement key.
pub fn sign_agreement(author: &Author, public: &str) -> Option<DeviceSigned> {
    sign_as(author, &agreement_message(&author.identity, public))
}

pub fn verify_agreement(identity: &str, public: &str, s: &DeviceSigned) -> bool {
    public.len() == 64 && verify_as(identity, &agreement_message(identity, public), s)
}

/// A profile key sealed to one recipient: an ephemeral X25519 key and `nonce ‖ ciphertext`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedShare {
    pub ephemeral: String,
    pub sealed: String,
}

fn share_key(
    shared: &[u8; 32],
    ephemeral: &[u8; 32],
    recipient: &[u8; 32],
    context: &[u8],
) -> XChaCha20Poly1305 {
    let salt = [ephemeral.as_slice(), recipient].concat();
    let mut k = [0u8; 32];
    Hkdf::<Sha256>::new(Some(&salt), shared)
        .expand_multi_info(&[SHARE_INFO, b"\0", context], &mut k)
        .expect("32 bytes");
    XChaCha20Poly1305::new_from_slice(&k).expect("32-byte key")
}

/// `context` names sender, recipient and Space, so a share can't be replayed elsewhere.
pub fn share_context(space: &str, sender: &str, recipient: &str) -> Vec<u8> {
    [
        space.as_bytes(),
        b"\0",
        sender.as_bytes(),
        b"\0",
        recipient.as_bytes(),
    ]
    .concat()
}

/// Seals `(key, version)` to `recipient_public` (hex X25519).
pub fn seal_share(
    key: &ProfileKey,
    version: u64,
    recipient_public: &str,
    context: &[u8],
) -> Option<SealedShare> {
    let recipient: [u8; 32] = hex::decode(recipient_public).ok()?.try_into().ok()?;
    let eph = StaticSecret::from(random::<32>());
    let eph_pub = PublicKey::from(&eph).to_bytes();
    let shared = eph.diffie_hellman(&PublicKey::from(recipient));
    if !shared.was_contributory() {
        return None;
    }
    let nonce = random::<NONCE>();
    let msg = [key.0.as_slice(), &version.to_be_bytes()].concat();
    let ct = share_key(shared.as_bytes(), &eph_pub, &recipient, context)
        .encrypt(&XNonce::from(nonce), msg.as_slice())
        .ok()?;
    Some(SealedShare {
        ephemeral: hex::encode(eph_pub),
        sealed: hex::encode([nonce.as_slice(), &ct].concat()),
    })
}

/// Opens a share sealed to `me`: the sender's profile key and the version it unlocks.
pub fn open_share(
    me: &AgreementKey,
    share: &SealedShare,
    context: &[u8],
) -> Option<(ProfileKey, u64)> {
    let eph: [u8; 32] = hex::decode(&share.ephemeral).ok()?.try_into().ok()?;
    let sealed = hex::decode(&share.sealed).ok()?;
    if sealed.len() < NONCE + 16 {
        return None;
    }
    let shared = me.0.diffie_hellman(&PublicKey::from(eph));
    let recipient = PublicKey::from(&me.0).to_bytes();
    let nonce: [u8; NONCE] = sealed[..NONCE].try_into().ok()?;
    let plain = share_key(shared.as_bytes(), &eph, &recipient, context)
        .decrypt(&XNonce::from(nonce), &sealed[NONCE..])
        .ok()?;
    if plain.len() != 40 {
        return None;
    }
    let key = ProfileKey(plain[..32].try_into().ok()?);
    let version = u64::from_be_bytes(plain[32..].try_into().ok()?);
    Some((key, version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Signer;

    fn fields() -> ProfileFields {
        ProfileFields {
            name: "Ana".into(),
            bio: "trilhas e café".into(),
            photo: Some(ProfilePhoto {
                sha256: "aa".into(),
                blob: "bb".into(),
                key: "cc".into(),
                mime: "image/jpeg".into(),
            }),
        }
    }

    #[test]
    fn a_profile_opens_only_with_its_key_identity_and_version() {
        let key = ProfileKey::generate();
        let sealed = seal_profile(&key, "ana", 3, &fields());
        assert_eq!(open_profile(&key, "ana", 3, &sealed), Some(fields()));
        assert!(open_profile(&ProfileKey::generate(), "ana", 3, &sealed).is_none());
        assert!(
            open_profile(&key, "bruno", 3, &sealed).is_none(),
            "swapped between people"
        );
        assert!(
            open_profile(&key, "ana", 9, &sealed).is_none(),
            "served as another version"
        );
        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(open_profile(&key, "ana", 3, &tampered).is_none());
        assert!(!String::from_utf8_lossy(&sealed).contains("trilhas"));
    }

    #[test]
    fn a_share_opens_only_for_its_recipient_and_context() {
        let key = ProfileKey::generate();
        let bruno = AgreementKey::generate();
        let carla = AgreementKey::generate();
        let ctx = share_context("sp1", "ana", "bruno");
        let share = seal_share(&key, 7, &bruno.public_hex(), &ctx).unwrap();
        assert_eq!(open_share(&bruno, &share, &ctx), Some((key, 7)));
        assert!(open_share(&carla, &share, &ctx).is_none());
        assert!(
            open_share(&bruno, &share, &share_context("sp2", "ana", "bruno")).is_none(),
            "replayed in another Space"
        );
        assert!(
            seal_share(&ProfileKey::generate(), 1, &"00".repeat(32), &ctx).is_none(),
            "low-order key"
        );
    }

    #[test]
    fn uploads_and_agreement_keys_are_signed_by_a_certified_device() {
        let root = Signer::generate();
        let ana = Author::device(&root, Signer::generate());
        let ct = seal_profile(&ProfileKey::generate(), &ana.identity, 1, &fields());
        let s = sign_upload(&ana, 1, &ct).unwrap();
        assert!(verify_upload(&ana.identity, 1, &ct, &s));
        assert!(!verify_upload(&ana.identity, 2, &ct, &s));
        let mallory = Author::device(&Signer::generate(), Signer::generate());
        let forged = DeviceSigned {
            cert: mallory.cert.clone().unwrap(),
            ..sign_upload(&mallory, 1, &ct).unwrap()
        };
        assert!(!verify_upload(&ana.identity, 1, &ct, &forged));

        let ak = AgreementKey::generate();
        let s = sign_agreement(&ana, &ak.public_hex()).unwrap();
        assert!(verify_agreement(&ana.identity, &ak.public_hex(), &s));
        assert!(!verify_agreement(
            &ana.identity,
            &AgreementKey::generate().public_hex(),
            &s
        ));
    }
}

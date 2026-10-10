//! # roda-mls: end-to-end Spaces on OpenMLS (ADR 0026)
//!
//! One [`Device`] is one MLS leaf. Its signature key is the device's Ed25519 key, and its
//! credential is a [`Leaf`]: identity, device and the identity's certificate over it.
//! The group id is the Space id. The relay orders every MLS message in the Space's log,
//! so log order is epoch order; this crate turns those bytes into plaintext and back.
//!
//! - Membership is member-checked: [`Device::open`] applies a commit only if everyone it
//!   leaves in the group is listed in the log, so a relay can't slip a reader in.
//! - A sender can't decrypt its own application messages ([`Opened::Own`]). It keeps what it
//!   sent, as every client already does.
//! - State lives in the device's SQLite database, every value sealed with a state key
//!   derived from the device secret ([`sealed`]).
//! - The relay reads one thing: the epoch a commit was made at ([`commit_epoch`]), from the
//!   clear MLS framing, so it can refuse a commit that lost the race for its epoch.
//! - [`Device::checkpoint`] gives the epoch and the digest members sign into `Checkpoint`
//!   events, so members can tell when they were shown different groups.

#[cfg(feature = "device")]
mod device;
mod leaf;
pub mod recovery;
#[cfg(feature = "device")]
pub mod sealed;

#[cfg(feature = "device")]
pub use device::{
    clear_recovery_markers, migrate, Commit, CommittedGroup, Device, GroupState, LeafKeys, Opened,
};
use std::collections::BTreeSet;

pub use leaf::Leaf;
use openmls::prelude::{tls_codec::Deserialize as _, *};
use openmls_rust_crypto::RustCrypto;

/// `MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519`, the value in `Sealed.suite`.
pub const SUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519;
/// [`SUITE`]'s id in the RFC 9420 registry, as `Sealed.suite` carries it.
pub const SUITE_ID: u32 = 0x0003;

#[derive(Debug, thiserror::Error)]
pub enum MlsError {
    #[error("MLS: {0}")]
    Mls(String),
    #[error("MLS state storage: {0}")]
    Storage(String),
    #[error("not valid MLS bytes: {0}")]
    Decode(String),
    #[error("a leaf whose credential doesn't verify")]
    Credential,
    #[error("no group for this space on this device")]
    NoGroup,
    #[error("the group would hold {0:?}, whom the log doesn't list")]
    Unlisted(BTreeSet<String>),
}

pub(crate) fn mls(e: impl std::fmt::Debug) -> MlsError {
    MlsError::Mls(format!("{e:?}"))
}

/// The verified leaf of a member, or `Credential` when it doesn't verify.
/// How a single leaf (one device of an identity) is named in a commit's removals.
pub fn leaf_name(identity: &str, device: &str) -> String {
    format!("{identity}/{device}")
}

pub(crate) fn leaf_of(credential: &Credential, signature_key: &[u8]) -> Result<Leaf, MlsError> {
    let basic = BasicCredential::try_from(credential.clone()).map_err(|_| MlsError::Credential)?;
    Leaf::verified(basic.identity(), signature_key).ok_or(MlsError::Credential)
}

/// Checks a key package someone published and returns whose device it is. The relay
/// uses this to refuse packages that don't name the device publishing them.
pub fn key_package_leaf(bytes: &[u8]) -> Result<Leaf, MlsError> {
    let kp = validate_key_package(&RustCrypto::default(), bytes)?;
    leaf_of(
        kp.leaf_node().credential(),
        kp.leaf_node().signature_key().as_slice(),
    )
}

/// A verified publication's device and signed expiry, for replay receipts on the relay.
pub fn key_package_publication(bytes: &[u8]) -> Result<(Leaf, i64), MlsError> {
    let kp = validate_key_package(&RustCrypto::default(), bytes)?;
    if !kp.life_time().has_acceptable_range() {
        return Err(MlsError::Mls("key package lifetime is too long".into()));
    }
    let expires = i64::try_from(kp.life_time().not_after())
        .map_err(|_| MlsError::Mls("key package expiry is out of range".into()))?;
    let leaf = leaf_of(
        kp.leaf_node().credential(),
        kp.leaf_node().signature_key().as_slice(),
    )?;
    Ok((leaf, expires))
}

/// The epoch a sealed commit was made at, from the clear framing of its PrivateMessage.
/// `None` for anything that isn't a commit. The relay sequences at most one commit per
/// epoch with this; it never needs a key.
pub fn commit_epoch(data: &[u8]) -> Option<u64> {
    let msg = MlsMessageIn::tls_deserialize_exact(data).ok()?;
    let msg = msg.try_into_protocol_message().ok()?;
    (msg.content_type() == ContentType::Commit).then(|| msg.epoch().as_u64())
}

/// The epoch an application message was sealed at, from its clear framing. `None` for
/// anything else. The relay refuses one sealed at an epoch the group has left: members
/// keep no past epochs, so nobody could open it.
pub fn application_epoch(data: &[u8]) -> Option<u64> {
    let msg = MlsMessageIn::tls_deserialize_exact(data).ok()?;
    let msg = msg.try_into_protocol_message().ok()?;
    (msg.content_type() == ContentType::Application).then(|| msg.epoch().as_u64())
}

pub(crate) fn validate_key_package(
    crypto: &RustCrypto,
    bytes: &[u8],
) -> Result<KeyPackage, MlsError> {
    let kp =
        KeyPackageIn::tls_deserialize_exact(bytes).map_err(|e| MlsError::Decode(e.to_string()))?;
    let kp = kp.validate(crypto, ProtocolVersion::Mls10).map_err(mls)?;
    if kp.ciphersuite() != SUITE {
        return Err(MlsError::Mls("key package for another ciphersuite".into()));
    }
    Ok(kp)
}

#[cfg(all(test, feature = "device"))]
mod tests;

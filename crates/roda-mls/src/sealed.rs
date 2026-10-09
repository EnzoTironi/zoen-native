//! MLS state at rest: every key and value OpenMLS stores is sealed with the device's state key.
//!
//! `openmls_sqlite_storage` serializes through a [`Codec`] with no instance, so the key
//! can't live in the codec. Every OpenMLS call is synchronous, so the device puts its keys in
//! a thread-local [`Scope`] for exactly the span of one call, and the codec fails closed
//! outside one. Two devices in one process (tests, agents) never see each other's keys.
//!
//! The same codec encodes the lookup keys (group ids, key package refs), so it must be
//! deterministic: the nonce is synthetic, HMAC-SHA256 of the plaintext under its own subkey
//! (the SIV construction). That reveals only which stored items are equal, which lookups
//! need anyway, and the nonce never repeats for different plaintexts.

use std::cell::RefCell;

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use openmls_sqlite_storage::Codec;
use sha2::Sha256;

const NONCE: usize = 24;

/// The state key of a device, derived from its Ed25519 secret: it lives wherever that
/// secret lives (the Keychain), and needs no vault entry of its own.
pub fn state_key(device_secret: &[u8; 32]) -> [u8; 32] {
    let mut key = [0; 32];
    Hkdf::<Sha256>::new(Some(b"zoen-mls-state-key/1"), device_secret)
        .expand(b"state", &mut key)
        .expect("32 bytes");
    key
}

#[derive(Clone, Copy)]
struct Keys {
    seal: [u8; 32],
    siv: [u8; 32],
}

impl Keys {
    fn derive(state_key: [u8; 32]) -> Self {
        let hk = Hkdf::<Sha256>::new(Some(b"zoen-mls-state/1"), &state_key);
        let mut keys = Self {
            seal: [0; 32],
            siv: [0; 32],
        };
        hk.expand(b"seal", &mut keys.seal).expect("32 bytes");
        hk.expand(b"siv", &mut keys.siv).expect("32 bytes");
        keys
    }
}

thread_local! {
    static KEYS: RefCell<Option<Keys>> = const { RefCell::new(None) };
}

/// The state key is in effect while this lives; dropping it (also on panic) restores the
/// scope it replaced.
pub(crate) struct Scope {
    previous: Option<Keys>,
}

impl Scope {
    pub(crate) fn enter(state_key: [u8; 32]) -> Self {
        Self {
            previous: KEYS.with(|k| k.replace(Some(Keys::derive(state_key)))),
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        KEYS.with(|k| *k.borrow_mut() = self.previous.take());
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SealError {
    #[error("MLS state touched outside a device scope")]
    NoKey,
    #[error("MLS state doesn't open with this device's key")]
    Open,
    #[error("MLS state encoding: {0}")]
    Encoding(#[from] serde_json::Error),
}

fn keys() -> Result<Keys, SealError> {
    KEYS.with(|k| *k.borrow()).ok_or(SealError::NoKey)
}

/// JSON (what OpenMLS's own codec uses), then XChaCha20-Poly1305 under a synthetic nonce.
#[derive(Default)]
pub struct SealedCodec;

impl Codec for SealedCodec {
    type Error = SealError;

    fn to_vec<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, SealError> {
        let keys = keys()?;
        let plain = serde_json::to_vec(value)?;
        let mut mac = Hmac::<Sha256>::new_from_slice(&keys.siv).expect("any key length");
        mac.update(&plain);
        let mut nonce = [0u8; NONCE];
        nonce.copy_from_slice(&mac.finalize().into_bytes()[..NONCE]);
        let sealed = XChaCha20Poly1305::new(&keys.seal.into())
            .encrypt(&XNonce::from(nonce), plain.as_slice())
            .map_err(|_| SealError::Open)?;
        Ok([nonce.as_slice(), &sealed].concat())
    }

    fn from_slice<T: serde::de::DeserializeOwned>(slice: &[u8]) -> Result<T, SealError> {
        let keys = keys()?;
        if slice.len() < NONCE {
            return Err(SealError::Open);
        }
        let (nonce, sealed) = slice.split_at(NONCE);
        let nonce = XNonce::try_from(nonce).map_err(|_| SealError::Open)?;
        let plain = XChaCha20Poly1305::new(&keys.seal.into())
            .decrypt(&nonce, sealed)
            .map_err(|_| SealError::Open)?;
        Ok(serde_json::from_slice(&plain)?)
    }
}

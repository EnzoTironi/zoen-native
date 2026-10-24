//! Shared ADR 0007 media blob codec: nonce24 || XChaCha20-Poly1305 ciphertext.
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};

const NONCE: usize = 24;

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("system randomness");
    b
}

/// `nonce ‖ ciphertext+tag` under a new random key.
#[must_use]
pub fn seal(plain: &[u8]) -> ([u8; 32], Vec<u8>) {
    let key = random::<32>();
    let nonce = random::<NONCE>();
    let cipher = XChaCha20Poly1305::new(&key.into());
    let ct = cipher
        .encrypt(&XNonce::from(nonce), plain)
        .expect("encrypt");
    let mut out = Vec::with_capacity(NONCE + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    (key, out)
}

/// The plaintext, or `None` if the key is wrong or a byte changed.
#[must_use]
pub fn open(key: &[u8], blob: &[u8]) -> Option<Vec<u8>> {
    if blob.len() < NONCE + 16 {
        return None;
    }
    let cipher = XChaCha20Poly1305::new_from_slice(key).ok()?;
    let nonce: [u8; NONCE] = blob[..NONCE].try_into().ok()?;
    cipher.decrypt(&XNonce::from(nonce), &blob[NONCE..]).ok()
}

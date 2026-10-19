//! Device-local authenticated values. Callers own domain separation and size budgets.
use base64::Engine as _;
use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};

pub(crate) fn seal(key: &[u8; 32], aad: &[u8], plain: &[u8]) -> Result<String, ()> {
    let mut nonce = [0u8; 24];
    getrandom::getrandom(&mut nonce).map_err(|_| ())?;
    let sealed = XChaCha20Poly1305::new(key.into())
        .encrypt(&XNonce::from(nonce), Payload { msg: plain, aad })
        .map_err(|_| ())?;
    let mut bytes = nonce.to_vec();
    bytes.extend(sealed);
    Ok(format!(
        "v1:{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

pub(crate) fn open(
    key: &[u8; 32],
    aad: &[u8],
    encoded: &str,
    maximum: usize,
) -> Result<Vec<u8>, ()> {
    if maximum != usize::MAX
        && encoded.len()
            > (maximum.saturating_add(40))
                .div_ceil(3)
                .saturating_mul(4)
                .saturating_add(3)
    {
        return Err(());
    }
    let sealed = base64::engine::general_purpose::STANDARD
        .decode(encoded.strip_prefix("v1:").ok_or(())?)
        .map_err(|_| ())?;
    if sealed.len() < 40 {
        return Err(());
    }
    let nonce: [u8; 24] = sealed[..24].try_into().map_err(|_| ())?;
    let plain = XChaCha20Poly1305::new(key.into())
        .decrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &sealed[24..],
                aad,
            },
        )
        .map_err(|_| ())?;
    if plain.len() > maximum {
        return Err(());
    }
    Ok(plain)
}

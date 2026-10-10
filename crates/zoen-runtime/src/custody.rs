use crate::{hash, RuntimeError};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use serde::{de::DeserializeOwned, Serialize};

pub(crate) const MAX_SEALED: usize = 98_304;
pub(crate) struct Custody {
    cipher: XChaCha20Poly1305,
    namespace: String,
}

impl Custody {
    pub fn new(key: [u8; 32], namespace: String) -> Self {
        Self {
            cipher: XChaCha20Poly1305::new((&key).into()),
            namespace,
        }
    }
    fn aad(&self, subject: &str) -> Vec<u8> {
        format!("zoen-runtime-sealed-v1\0{}\0{subject}", self.namespace).into_bytes()
    }
    pub fn seal<T: Serialize>(
        &self,
        subject: &str,
        value: &T,
    ) -> Result<(String, Vec<u8>), RuntimeError> {
        let plain = serde_json::to_vec(value).map_err(|_| RuntimeError::InvalidBinding)?;
        if plain.len() + 40 > MAX_SEALED {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let digest = hash(&plain);
        let mut nonce = [0; 24];
        getrandom::getrandom(&mut nonce).map_err(|_| RuntimeError::Unavailable)?;
        let mut sealed = nonce.to_vec();
        sealed.extend(
            self.cipher
                .encrypt(
                    &XNonce::from(nonce),
                    Payload {
                        msg: &plain,
                        aad: &self.aad(subject),
                    },
                )
                .map_err(|_| RuntimeError::Unavailable)?,
        );
        Ok((digest, sealed))
    }
    pub fn open<T: DeserializeOwned>(
        &self,
        subject: &str,
        digest: Option<&str>,
        sealed: &[u8],
    ) -> Result<T, RuntimeError> {
        if !(40..=MAX_SEALED).contains(&sealed.len()) {
            return Err(RuntimeError::InvalidBinding);
        }
        let nonce: [u8; 24] = sealed[..24]
            .try_into()
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let plain = self
            .cipher
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &sealed[24..],
                    aad: &self.aad(subject),
                },
            )
            .map_err(|_| RuntimeError::InvalidBinding)?;
        if digest.is_some_and(|d| hash(&plain) != d) {
            return Err(RuntimeError::InvalidBinding);
        }
        serde_json::from_slice(&plain).map_err(|_| RuntimeError::InvalidBinding)
    }
}

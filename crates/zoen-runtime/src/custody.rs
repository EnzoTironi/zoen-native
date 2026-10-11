use crate::{hash, RuntimeError};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use serde::{de::DeserializeOwned, Serialize};
use zeroize::Zeroizing;

pub(crate) const MAX_SEALED: usize = 98_304;
pub(crate) struct Custody {
    cipher: XChaCha20Poly1305,
    namespace: String,
}

impl Custody {
    pub fn new(key: [u8; 32], namespace: String) -> Self {
        let key = Zeroizing::new(key);
        Self {
            cipher: XChaCha20Poly1305::new((&*key).into()),
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
        let plain =
            Zeroizing::new(serde_json::to_vec(value).map_err(|_| RuntimeError::InvalidBinding)?);
        self.seal_bytes(subject, &plain)
    }
    pub fn seal_bytes(
        &self,
        subject: &str,
        plain: &[u8],
    ) -> Result<(String, Vec<u8>), RuntimeError> {
        if plain
            .len()
            .checked_add(40)
            .is_none_or(|size| size > MAX_SEALED)
        {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let digest = hash(plain);
        let mut nonce = [0; 24];
        getrandom::getrandom(&mut nonce).map_err(|_| RuntimeError::Unavailable)?;
        let mut sealed = nonce.to_vec();
        sealed.extend(
            self.cipher
                .encrypt(
                    &XNonce::from(nonce),
                    Payload {
                        msg: plain,
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
        let plain = self.open_bytes(subject, digest, sealed)?;
        serde_json::from_slice(&plain).map_err(|_| RuntimeError::InvalidBinding)
    }
    pub fn open_bytes(
        &self,
        subject: &str,
        digest: Option<&str>,
        sealed: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, RuntimeError> {
        if !(40..=MAX_SEALED).contains(&sealed.len()) {
            return Err(RuntimeError::InvalidBinding);
        }
        let nonce: [u8; 24] = sealed[..24]
            .try_into()
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let plain = Zeroizing::new(
            self.cipher
                .decrypt(
                    &XNonce::from(nonce),
                    Payload {
                        msg: &sealed[24..],
                        aad: &self.aad(subject),
                    },
                )
                .map_err(|_| RuntimeError::InvalidBinding)?,
        );
        if digest.is_some_and(|d| hash(&plain) != d) {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(plain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_chunks_are_bounded_binary_and_bound_to_key_namespace_and_subject() {
        let custody = Custody::new([7; 32], "native-fixture".into());
        let bytes: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
        let (digest, sealed) = custody
            .seal_bytes("capsule/agent/device/generation/chunk/0", &bytes)
            .unwrap();
        assert_eq!(sealed.len(), 65576);
        assert_eq!(
            custody
                .open_bytes(
                    "capsule/agent/device/generation/chunk/0",
                    Some(&digest),
                    &sealed
                )
                .unwrap()
                .as_slice(),
            bytes.as_slice()
        );
        assert!(custody
            .open_bytes(
                "capsule/agent/device/generation/chunk/1",
                Some(&digest),
                &sealed
            )
            .is_err());
        assert!(Custody::new([7; 32], "other-domain".into())
            .open_bytes(
                "capsule/agent/device/generation/chunk/0",
                Some(&digest),
                &sealed
            )
            .is_err());
        assert!(Custody::new([8; 32], "native-fixture".into())
            .open_bytes(
                "capsule/agent/device/generation/chunk/0",
                Some(&digest),
                &sealed
            )
            .is_err());
        let mut corrupt = sealed;
        corrupt[100] ^= 1;
        assert!(custody
            .open_bytes(
                "capsule/agent/device/generation/chunk/0",
                Some(&digest),
                &corrupt
            )
            .is_err());
        assert_eq!(
            custody
                .seal_bytes("large", &vec![0; MAX_SEALED])
                .unwrap_err(),
            RuntimeError::EvidenceTooLarge
        );
    }
}

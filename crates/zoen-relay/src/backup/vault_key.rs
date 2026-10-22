use super::*;

// ───────────────────────────── the HSM boundary ─────────────────────────────

/// The only code that touches a person's OPRF key. Staging seals keys under a master key
/// from the environment ([`EnvVault`]); production swaps in an HSM or enclave behind the
/// same calls, and `k` never exists outside it.
pub trait Vault: Send + Sync {
    /// A fresh OPRF key for `identity`, sealed.
    fn new_key(&self, identity: &str) -> anyhow::Result<Vec<u8>>;
    /// `k·blinded` for the sealed key; `None` if the key or the point is invalid.
    fn evaluate(&self, sealed: &[u8], identity: &str, blinded: &[u8; 32]) -> Option<[u8; 32]>;
    /// Evaluation under a key that exists for no one (no backup, or unknown handle): the
    /// same for the same handle, so a restore can't tell "no backup" from "wrong password".
    fn evaluate_decoy(&self, handle: &str, blinded: &[u8; 32]) -> Option<[u8; 32]>;
    /// Stable decoy fields from a separate secret PRF, independent of public evaluations.
    fn decoy_metadata(&self, handle: &str) -> ([u8; 32], [u8; 32]);
}

pub struct EnvVault {
    master: [u8; 32],
}

impl EnvVault {
    /// `ZOEN_BACKUP_VAULT_KEY` (64 hex chars). `None` disables password backups.
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let Ok(hex_key) = std::env::var("ZOEN_BACKUP_VAULT_KEY") else {
            return Ok(None);
        };
        let bytes = hex::decode(hex_key.trim())
            .map_err(|_| anyhow::anyhow!("ZOEN_BACKUP_VAULT_KEY must be 64 hex characters"))?;
        let master: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("ZOEN_BACKUP_VAULT_KEY must be 32 bytes"))?;
        Ok(Some(Self { master }))
    }

    fn sub_key(&self, info: &[u8]) -> [u8; 32] {
        let hk = hkdf::Hkdf::<Sha256>::new(Some(b"zoen-backup-vault-v1"), &self.master);
        let mut out = [0u8; 32];
        hk.expand(info, &mut out).expect("32 bytes");
        out
    }

    fn aad(identity: &str) -> Vec<u8> {
        [b"zoen-backup-oprf-key-v1\0".as_slice(), identity.as_bytes()].concat()
    }
}

fn point_mul(k: &Scalar, blinded: &[u8; 32]) -> Option<[u8; 32]> {
    let p = CompressedRistretto(*blinded).decompress()?;
    // The identity point would make the output independent of k.
    if p == curve25519_dalek::RistrettoPoint::default() {
        return None;
    }
    Some((k * p).compress().to_bytes())
}

impl Vault for EnvVault {
    fn new_key(&self, identity: &str) -> anyhow::Result<Vec<u8>> {
        let mut wide = [0u8; 64];
        getrandom::getrandom(&mut wide)?;
        let k = Scalar::from_bytes_mod_order_wide(&wide);
        let mut nonce = [0u8; 24];
        getrandom::getrandom(&mut nonce)?;
        let aead = XChaCha20Poly1305::new((&self.sub_key(b"seal")).into());
        let ct = aead
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: k.as_bytes(),
                    aad: &Self::aad(identity),
                },
            )
            .map_err(|_| anyhow::anyhow!("seal failed"))?;
        Ok([nonce.as_slice(), &ct].concat())
    }

    fn evaluate(&self, sealed: &[u8], identity: &str, blinded: &[u8; 32]) -> Option<[u8; 32]> {
        if sealed.len() < 24 {
            return None;
        }
        let aead = XChaCha20Poly1305::new((&self.sub_key(b"seal")).into());
        let nonce: [u8; 24] = sealed[..24].try_into().ok()?;
        let k = aead
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &sealed[24..],
                    aad: &Self::aad(identity),
                },
            )
            .ok()?;
        let k: [u8; 32] = k.try_into().ok()?;
        let k = Option::<Scalar>::from(Scalar::from_canonical_bytes(k))?;
        point_mul(&k, blinded)
    }

    fn evaluate_decoy(&self, handle: &str, blinded: &[u8; 32]) -> Option<[u8; 32]> {
        let mut wide = [0u8; 64];
        let hk = hkdf::Hkdf::<Sha256>::new(Some(b"zoen-backup-decoy-v1"), &self.master);
        hk.expand(handle.as_bytes(), &mut wide).expect("64 bytes");
        point_mul(&Scalar::from_bytes_mod_order_wide(&wide), blinded)
    }

    fn decoy_metadata(&self, handle: &str) -> ([u8; 32], [u8; 32]) {
        let hk = hkdf::Hkdf::<Sha256>::new(Some(b"zoen-backup-decoy-metadata-v1"), &self.master);
        let mut out = [0u8; 64];
        hk.expand(handle.as_bytes(), &mut out).expect("64 bytes");
        (
            out[..32].try_into().expect("32 bytes"),
            out[32..].try_into().expect("32 bytes"),
        )
    }
}

// ───────────────────────────── helpers ─────────────────────────────

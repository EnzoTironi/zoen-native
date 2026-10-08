//! Identifiers in logs and traces are keyed pseudonyms, never raw ids or prefixes of them:
//! a raw prefix still matches the public directory. The key comes from
//! `ZOEN_LOG_PSEUDONYM_KEY` (shared by replicas so their logs correlate, rotated with the
//! secret) or is random per process.

use std::{fmt, sync::OnceLock};

use sha2::{Digest, Sha256};

fn key() -> &'static [u8; 32] {
    static KEY: OnceLock<[u8; 32]> = OnceLock::new();
    KEY.get_or_init(|| match std::env::var("ZOEN_LOG_PSEUDONYM_KEY") {
        Ok(k) if !k.is_empty() => Sha256::digest(k.as_bytes()).into(),
        _ => {
            let mut k = [0u8; 32];
            getrandom::getrandom(&mut k).expect("os randomness");
            k
        }
    })
}

/// `p:` plus 12 hex characters of SHA-256(key ‖ id).
pub struct Pseudonym([u8; 6]);

pub fn pseudo(id: &str) -> Pseudonym {
    let digest = Sha256::new()
        .chain_update(key())
        .chain_update(id.as_bytes())
        .finalize();
    let mut out = [0u8; 6];
    out.copy_from_slice(&digest[..6]);
    Pseudonym(out)
}

impl fmt::Display for Pseudonym {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "p:{}", hex::encode(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pseudonyms_are_stable_and_never_a_prefix_of_the_id() {
        let id = "a3f9c2e17b4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7";
        let p = pseudo(id).to_string();
        assert_eq!(p, pseudo(id).to_string());
        assert_ne!(p, pseudo("sp_other").to_string());
        assert_eq!(p.len(), 14);
        assert!(!p.contains(&id[..12]));
    }
}

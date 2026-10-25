//! Explicit failure before publishing link keys or encrypted history chunks.
use std::fmt;

#[derive(Debug)]
pub enum LinkError {
    Entropy(getrandom::Error),
    KeyDerivation(hpke_rs::HpkeError),
    ChunkEncryption,
}

impl fmt::Display for LinkError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entropy(error) => write!(output, "link entropy unavailable: {error}"),
            Self::KeyDerivation(error) => write!(output, "link key derivation failed: {error}"),
            Self::ChunkEncryption => output.write_str("history chunk encryption failed"),
        }
    }
}
impl std::error::Error for LinkError {}

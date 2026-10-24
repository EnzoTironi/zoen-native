//! Failure boundaries for key material and profile/media encryption.

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CryptoError {
    #[error("cryptographic entropy unavailable: {0}")]
    Entropy(getrandom::Error),
    #[error("cryptographic key derivation failed")]
    KeyDerivation,
    #[error("authenticated encryption failed")]
    Encryption,
}

impl From<getrandom::Error> for CryptoError {
    fn from(error: getrandom::Error) -> Self {
        Self::Entropy(error)
    }
}

pub(crate) fn random<const N: usize>() -> Result<[u8; N], CryptoError> {
    random_with(getrandom::getrandom)
}

fn random_with<const N: usize>(
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<[u8; N], CryptoError> {
    let mut bytes = [0; N];
    fill(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entropy_failure_is_explicit_and_never_returns_partial_key_bytes() {
        let result = random_with::<32>(|bytes| {
            bytes[..16].fill(7);
            Err(getrandom::Error::UNSUPPORTED)
        });
        assert!(matches!(result, Err(CryptoError::Entropy(_))));
        assert_eq!(
            random_with::<32>(|bytes| {
                bytes.fill(3);
                Ok(())
            })
            .unwrap(),
            [3; 32]
        );
    }
}

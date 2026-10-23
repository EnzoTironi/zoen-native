//! Recoverable local boundaries must refuse work, never manufacture capability bytes.
use super::{custom, FdbBindingError, Mutex, StoreError};

pub(super) fn invite_code(
    entropy: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<String, StoreError> {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut bytes = [0u8; 10];
    entropy(&mut bytes).map_err(|_| StoreError("invite entropy unavailable".into()))?;
    Ok(bytes
        .iter()
        .map(|byte| char::from(ALPHABET[usize::from(*byte & 31)]))
        .collect())
}

pub(super) fn take_cache<T>(cache: &Mutex<Option<T>>) -> Result<Option<T>, FdbBindingError> {
    cache
        .lock()
        .map(|mut held| held.take())
        .map_err(|_| custom("sequencer cache unavailable"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn entropy_failure_does_not_return_partial_or_zero_capability() {
        let result = invite_code(|bytes| {
            bytes[..3].fill(27);
            Err(getrandom::Error::UNSUPPORTED)
        });
        assert_eq!(result.unwrap_err().0, "invite entropy unavailable");
        let code = invite_code(|bytes| {
            bytes.copy_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 255]);
            Ok(())
        })
        .unwrap();
        assert_eq!(code, "012345678Z");
        assert!(protocol::valid_invite_code(&code));
    }

    #[test]
    fn poisoned_cache_refuses_instead_of_reloading_or_using_untrusted_state() {
        let cache = Arc::new(Mutex::new(Some("retained state")));
        let worker = cache.clone();
        assert!(std::thread::spawn(move || {
            let _held = worker.lock().unwrap();
            panic!("controlled cache poison");
        })
        .join()
        .is_err());
        assert!(take_cache(&cache).is_err());
        assert!(cache.is_poisoned());
    }
}

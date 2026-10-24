//! Stable identifier generation and aliases.
use std::{fmt, sync::Mutex, time::SystemTime};

#[cfg(test)]
mod tests;

/// Chave pública Ed25519 em hex (32 bytes → 64 caracteres).
pub type IdentityId = String;
pub type SpaceId = String;
pub type ItemId = String;
pub type GrantId = String;
pub type RequestId = String;

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
static LAST: Mutex<u128> = Mutex::new(0);

#[derive(Debug)]
pub enum IdError {
    Clock(std::time::SystemTimeError),
    ClockRange(std::num::TryFromIntError),
    Entropy(getrandom::Error),
    SequencePoisoned,
    SequenceExhausted,
}

impl fmt::Display for IdError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clock(error) => write!(output, "identifier clock is before the epoch: {error}"),
            Self::ClockRange(error) => write!(output, "identifier clock is out of range: {error}"),
            Self::Entropy(error) => write!(output, "identifier entropy unavailable: {error}"),
            Self::SequencePoisoned => output.write_str("identifier sequence state is poisoned"),
            Self::SequenceExhausted => output.write_str("identifier sequence is exhausted"),
        }
    }
}
impl std::error::Error for IdError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Clock(error) => Some(error),
            Self::ClockRange(error) => Some(error),
            _ => None,
        }
    }
}

/// A readable prefix plus a lowercase ULID: `sp_01jq3…` (ADR 0017). Ids made later sort
/// later, across devices to the millisecond and on one device strictly.
///
/// # Errors
/// Refuses unavailable/out-of-range clock or entropy and invalid sequence state.
pub fn new_id(prefix: &str) -> Result<String, IdError> {
    let now = clock_ms(SystemTime::now())?;
    Ok(format!("{prefix}_{}", new_ulid(now)?.to_ascii_lowercase()))
}

fn clock_ms(now: SystemTime) -> Result<i64, IdError> {
    let elapsed = now
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(IdError::Clock)?;
    i64::try_from(elapsed.as_millis()).map_err(IdError::ClockRange)
}

/// 128 random bits in hex, for ids that double as a capability (invite links): never a
/// ULID, whose leading characters are a guessable timestamp.
///
/// # Errors
/// Refuses the identifier when system entropy is unavailable.
pub fn new_secret_id(prefix: &str) -> Result<String, IdError> {
    secret_id_with(prefix, getrandom::getrandom)
}

fn secret_id_with(
    prefix: &str,
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<String, IdError> {
    let mut bytes = [0u8; 16];
    fill(&mut bytes).map_err(IdError::Entropy)?;
    Ok(format!("{prefix}_{}", hex::encode(bytes)))
}

/// A ULID (26 chars, Crockford base32): 48 bits of milliseconds + 80 random bits.
/// Monotonic within this process, including clock rollback. Negative times retain
/// the historical epoch-zero encoding. Used as every event's idempotency key.
///
/// # Errors
/// Refuses unavailable entropy, poisoned sequence state or exhausted sequence.
pub fn new_ulid(now_ms: i64) -> Result<String, IdError> {
    ulid_with(&LAST, now_ms, getrandom::getrandom)
}

fn ulid_with(
    sequence: &Mutex<u128>,
    now_ms: i64,
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<String, IdError> {
    let mut random = [0u8; 16];
    fill(&mut random[6..]).map_err(IdError::Entropy)?;
    let timestamp = u128::from(now_ms.max(0).unsigned_abs()) & 0xFFFF_FFFF_FFFF;
    let mut value = (timestamp << 80) | u128::from_be_bytes(random);
    let mut last = sequence.lock().map_err(|_| IdError::SequencePoisoned)?;
    if value >> 80 <= *last >> 80 {
        value = last.checked_add(1).ok_or(IdError::SequenceExhausted)?;
    }
    *last = value;
    drop(last);
    let mut out = [0u8; 26];
    for i in (0..26).rev() {
        out[i] = CROCKFORD[usize::from(value.to_le_bytes()[0] & 31)];
        value >>= 5;
    }
    Ok(out.into_iter().map(char::from).collect())
}

/// When a ULID-based id was made (ms since the epoch), or `None` for ids from before
/// ADR 0017 (random hex), out-of-range base32 and anything else.
#[must_use]
pub fn id_time_ms(id: &str) -> Option<i64> {
    let ulid = id.rsplit('_').next()?;
    if ulid.len() != 26 {
        return None;
    }
    let mut value: u128 = 0;
    for character in ulid.bytes() {
        let digit = CROCKFORD
            .iter()
            .position(|&a| a == character.to_ascii_uppercase())?;
        value = value
            .checked_mul(32)?
            .checked_add(u128::try_from(digit).ok()?)?;
    }
    i64::try_from(value >> 80).ok()
}

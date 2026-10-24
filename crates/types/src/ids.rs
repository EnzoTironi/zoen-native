//! Stable identifier generation and aliases.

/// Chave pública Ed25519 em hex (32 bytes → 64 caracteres).
pub type IdentityId = String;
pub type SpaceId = String;
pub type ItemId = String;
pub type GrantId = String;
pub type RequestId = String;

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A readable prefix plus a lowercase ULID: `sp_01jq3…` (ADR 0017). Ids made later sort
/// later, across devices to the millisecond and on one device strictly.
#[must_use]
pub fn new_id(prefix: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    format!("{prefix}_{}", new_ulid(now).to_ascii_lowercase())
}

/// 128 random bits in hex, for ids that double as a capability (invite links): never a
/// ULID, whose leading characters are a guessable timestamp.
#[must_use]
pub fn new_secret_id(prefix: &str) -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("fonte de entropia do sistema");
    format!("{prefix}_{}", hex::encode(bytes))
}

/// A ULID (26 chars, Crockford base32): 48 bits of milliseconds + 80 random bits.
/// Monotonic within this process: a second id in the same millisecond (or after the clock
/// steps back) increments the previous one instead of drawing fresh bits, so ids made on one
/// device always sort in creation order. Used as every event's idempotency key.
pub fn new_ulid(now_ms: i64) -> String {
    static LAST: std::sync::Mutex<u128> = std::sync::Mutex::new(0);
    let mut rnd = [0u8; 10];
    getrandom::getrandom(&mut rnd).expect("fonte de entropia do sistema");
    let mut v: u128 = (now_ms.max(0) as u128 & 0xFFFF_FFFF_FFFF) << 80;
    for (i, b) in rnd.iter().enumerate() {
        v |= (*b as u128) << (8 * (9 - i));
    }
    let mut last = LAST.lock().unwrap_or_else(|p| p.into_inner());
    if v >> 80 <= *last >> 80 {
        v = last.wrapping_add(1);
    }
    *last = v;
    drop(last);
    let mut out = [0u8; 26];
    for i in (0..26).rev() {
        out[i] = CROCKFORD[(v & 31) as usize];
        v >>= 5;
    }
    String::from_utf8(out.to_vec()).expect("ascii")
}

/// When a ULID-based id was made (ms since the epoch), or `None` for ids from before
/// ADR 0017 (random hex) and anything else.
#[must_use]
pub fn id_time_ms(id: &str) -> Option<i64> {
    let ulid = id.rsplit('_').next()?;
    if ulid.len() != 26 {
        return None;
    }
    let mut v: u128 = 0;
    for c in ulid.bytes() {
        let d = CROCKFORD
            .iter()
            .position(|&a| a == c.to_ascii_uppercase())?;
        v = (v << 5) | d as u128;
    }
    Some((v >> 80) as i64)
}

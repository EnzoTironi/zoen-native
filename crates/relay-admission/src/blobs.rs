//! Native media admission stages. Device lookup, quota persistence and durable
//! create-if-absent storage remain host I/O, in that order between these stages.

use ring::digest::{Context, SHA256};
use serde::Serialize;

pub const MAX_BLOB_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_SKEW_MS: u64 = 5 * 60 * 1000;
pub const MAX_DIGEST_CHUNK: usize = 64 * 1024;
pub const DEFAULT_INTERVAL_NS: u64 = 60_000_000_000 / (100 * 1024);
pub const DEFAULT_BURST: u32 = 200 * 1024;

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Failure {
    pub status: u16,
    pub body: &'static str,
}

impl Failure {
    fn new(status: u16, body: &'static str) -> Self {
        Self { status, body }
    }
    fn internal() -> Self {
        Self::new(500, "storage unavailable")
    }
}

#[derive(Debug, Serialize)]
pub struct PutAdmission {
    pub device: String,
    pub key: String,
}

pub fn valid_sha(sha: &str) -> bool {
    sha.len() == 64
        && sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// GET is an unauthenticated ciphertext capability. No device or Space facts.
pub fn get_key(sha: &str) -> Result<String, Failure> {
    if !valid_sha(sha) {
        return Err(Failure::new(400, "bad hash"));
    }
    Ok(format!("blobs/{}/{sha}", &sha[..2]))
}

/// Only the signed prefix. The host must next check the current active device.
pub fn put_policy(
    sha: &str,
    device: &str,
    timestamp: &str,
    signature: &str,
    relay: &str,
    now_ms: i64,
) -> Result<PutAdmission, Failure> {
    put_signed(sha, device, timestamp, signature, now_ms, |ts| {
        Ok(protocol::blob_put_message(sha, ts, relay))
    })
}

pub fn put_bound_policy(
    sha: &str,
    device: &str,
    timestamp: &str,
    signature: &str,
    relay: &str,
    now_ms: i64,
    binding: &protocol::runtime_binding::BlobBinding,
) -> Result<PutAdmission, Failure> {
    put_signed(sha, device, timestamp, signature, now_ms, |ts| {
        binding
            .message(sha, ts, relay)
            .ok_or_else(|| Failure::new(401, "invalid runtime binding"))
    })
}

fn put_signed(
    sha: &str,
    device: &str,
    timestamp: &str,
    signature: &str,
    now_ms: i64,
    message: impl FnOnce(i64) -> Result<Vec<u8>, Failure>,
) -> Result<PutAdmission, Failure> {
    let key = get_key(sha)?;
    let ts = timestamp
        .parse::<i64>()
        .map_err(|_| Failure::new(401, "missing signature"))?;
    // abs_diff handles the complete signed range without subtraction/abs panic.
    if now_ms.abs_diff(ts) > MAX_SKEW_MS {
        return Err(Failure::new(401, "stale signature"));
    }
    if !event_log::verify_sig(device, &message(ts)?, signature) {
        return Err(Failure::new(401, "bad signature"));
    }
    Ok(PutAdmission {
        device: device.into(),
        key,
    })
}

/// Called only after the current-device SQL gate and before rate charging.
pub fn body_cost(length: u64) -> Result<u32, Failure> {
    if length == 0 || length > MAX_BLOB_BYTES as u64 {
        return Err(Failure::new(413, "empty or too large"));
    }
    Ok(length.div_ceil(1024) as u32)
}

/// Incremental ciphertext SHA-256; no plaintext or decryption keys enter policy.
pub struct BlobDigest(Context);

impl Default for BlobDigest {
    fn default() -> Self {
        Self::new()
    }
}

impl BlobDigest {
    pub fn new() -> Self {
        Self(Context::new(&SHA256))
    }
    pub fn update(&mut self, bytes: &[u8]) -> Result<(), Failure> {
        if bytes.len() > MAX_DIGEST_CHUNK {
            return Err(Failure::internal());
        }
        self.0.update(bytes);
        Ok(())
    }
    pub fn finish(self, expected_sha: &str) -> Result<String, Failure> {
        let actual = self.finish_hash();
        if actual != expected_sha {
            return Err(Failure::new(400, "hash mismatch"));
        }
        Ok(actual)
    }
    /// Transfer signatures bind the computed hash rather than a hash in the URL.
    pub fn finish_hash(self) -> String {
        self.0
            .finish()
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

pub fn verify_body(bytes: &[u8], sha: &str) -> Result<(), Failure> {
    let mut digest = BlobDigest::new();
    for chunk in bytes.chunks(MAX_DIGEST_CHUNK) {
        digest.update(chunk)?;
    }
    digest.finish(sha).map(|_| ())
}

//! Encrypted device-history transport admission. Hosts own the current device
//! lookup, shared media quota and atomic object storage; no plaintext enters here.

use serde::Serialize;

pub const MAX_CHUNK: usize = 4 * 1024 * 1024;
pub const MAX_CHUNKS: u32 = 4096;
const MAX_SKEW_MS: u64 = 5 * 60 * 1000;

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Failure {
    pub status: u16,
    pub body: &'static str,
}

impl Failure {
    fn new(status: u16, body: &'static str) -> Self {
        Self { status, body }
    }
}

#[derive(Debug, Serialize)]
pub struct Admission {
    pub device: String,
    pub key: String,
    pub cost_kib: u32,
}

pub fn get_key(id: &str, chunk: u32) -> Result<String, Failure> {
    if !crate::blobs::valid_sha(id) || chunk >= MAX_CHUNKS {
        return Err(Failure::new(400, "bad transfer"));
    }
    Ok(format!("transfers/{id}/{chunk}"))
}

pub fn delete_prefix(id: &str) -> Result<String, Failure> {
    if !crate::blobs::valid_sha(id) {
        return Err(Failure::new(400, "bad transfer"));
    }
    // The slash matters for object stores whose list prefix is byte based.
    Ok(format!("transfers/{id}/"))
}

pub fn body_cost(length: u64) -> Result<u32, Failure> {
    if length == 0 || length > MAX_CHUNK as u64 {
        return Err(Failure::new(413, "empty or too large"));
    }
    Ok(length.div_ceil(1024) as u32)
}

fn signature(
    device: &str,
    timestamp: &str,
    signature: &str,
    now_ms: i64,
    message: impl FnOnce(i64) -> Vec<u8>,
) -> Result<(), Failure> {
    let ts = timestamp
        .parse::<i64>()
        .map_err(|_| Failure::new(401, "missing signature"))?;
    if now_ms.abs_diff(ts) > MAX_SKEW_MS {
        return Err(Failure::new(401, "stale signature"));
    }
    if !event_log::verify_sig(device, &message(ts), signature) {
        return Err(Failure::new(401, "bad signature"));
    }
    Ok(())
}

/// Native precedence is path, body bound, signature, current device, then quota.
/// `sha` must be computed from the complete actual ciphertext by the host.
#[allow(clippy::too_many_arguments)]
pub fn put_policy(
    id: &str,
    chunk: u32,
    length: u64,
    sha: &str,
    device: &str,
    timestamp: &str,
    signed: &str,
    now_ms: i64,
) -> Result<Admission, Failure> {
    let key = get_key(id, chunk)?;
    let cost_kib = body_cost(length)?;
    signature(device, timestamp, signed, now_ms, |ts| {
        protocol::transfer_message("put", id, &chunk.to_string(), sha, ts)
    })?;
    Ok(Admission {
        device: device.into(),
        key,
        cost_kib,
    })
}

pub fn delete_policy(
    id: &str,
    device: &str,
    timestamp: &str,
    signed: &str,
    now_ms: i64,
) -> Result<String, Failure> {
    let prefix = delete_prefix(id)?;
    signature(device, timestamp, signed, now_ms, |ts| {
        protocol::transfer_message("delete", id, "", "", ts)
    })?;
    Ok(prefix)
}

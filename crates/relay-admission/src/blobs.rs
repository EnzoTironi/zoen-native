//! Native media admission stages. Device lookup, quota persistence and durable
//! create-if-absent storage remain host I/O, in that order between these stages.

use ring::digest::{Context, SHA256};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::time::Duration;

use crate::gcra;

pub const MAX_BLOB_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_SKEW_MS: u64 = 5 * 60 * 1000;
pub const MAX_DIGEST_CHUNK: usize = 64 * 1024;
const MAX_CONTEXT_BYTES: usize = 16 * 1024;
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
    let key = get_key(sha)?;
    let ts = timestamp
        .parse::<i64>()
        .map_err(|_| Failure::new(401, "missing signature"))?;
    // abs_diff handles the complete signed range without subtraction/abs panic.
    if now_ms.abs_diff(ts) > MAX_SKEW_MS {
        return Err(Failure::new(401, "stale signature"));
    }
    if !event_log::verify_sig(
        device,
        &protocol::blob_put_message(sha, ts, relay),
        signature,
    ) {
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

fn input<T: DeserializeOwned>(text: &str) -> Result<T, Failure> {
    if text.len() > MAX_CONTEXT_BYTES {
        return Err(Failure::internal());
    }
    serde_json::from_str(text).map_err(|_| Failure::internal())
}
fn output(value: impl Serialize) -> Result<String, Failure> {
    serde_json::to_string(&value).map_err(|_| Failure::internal())
}
fn decimal(value: &str) -> Result<u64, Failure> {
    crate::portable::parse_u64(value).map_err(|_| Failure::internal())
}

/// Raw public fields must reach native validation without a JSON/context limit
/// shadowing the native hash, timestamp and signature rejection precedence.
pub fn put_policy_wire(
    sha: &str,
    device: &str,
    timestamp: &str,
    signature: &str,
    relay: &str,
    now_ms: &str,
) -> Result<String, Failure> {
    let now = now_ms.parse::<i64>().map_err(|_| Failure::internal())?;
    output(put_policy(sha, device, timestamp, signature, relay, now)?)
}

pub fn body_policy_json(text: &str) -> Result<String, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Body {
        length: String,
    }
    let body: Body = input(text)?;
    output(serde_json::json!({ "cost_kib": body_cost(decimal(&body.length)?)? }))
}

pub fn get_policy_json(sha: &str) -> Result<String, Failure> {
    output(serde_json::json!({ "key": get_key(sha)? }))
}

pub fn rate_config_json() -> String {
    serde_json::json!({"interval_ns": DEFAULT_INTERVAL_NS.to_string(), "burst": DEFAULT_BURST})
        .to_string()
}

pub fn rate_decision_json(text: &str) -> Result<String, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Rate {
        now_ns: String,
        tat_ns: Option<String>,
        cost: u32,
        interval_ns: String,
        burst: u32,
    }
    let r: Rate = input(text)?;
    match gcra::decide(
        decimal(&r.now_ns)?,
        r.tat_ns.as_deref().map(decimal).transpose()?,
        r.cost,
        decimal(&r.interval_ns)?,
        r.burst,
    ) {
        gcra::Decision::Accepted { next_ns } => {
            output(serde_json::json!({"accepted":true,"next_ns":next_ns.to_string()}))
        }
        gcra::Decision::Rejected { wait_ns } => {
            let wait = Duration::from_nanos(wait_ns);
            output(
                serde_json::json!({"accepted":false,"wait_ns":wait_ns.to_string(),
                "retry_after":wait.as_secs().max(1).to_string(),"body":protocol::slow_down(wait)}),
            )
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;
    fn failure(error: Failure) -> String {
        serde_json::to_string(&error).expect("plain media failure")
    }
    #[wasm_bindgen(js_name = blobPutPolicy)]
    pub fn put(
        sha: &str,
        device: &str,
        timestamp: &str,
        signature: &str,
        relay: &str,
        now_ms: &str,
    ) -> Result<String, String> {
        put_policy_wire(sha, device, timestamp, signature, relay, now_ms).map_err(failure)
    }
    #[wasm_bindgen(js_name = blobBodyPolicy)]
    pub fn body(context: &str) -> Result<String, String> {
        body_policy_json(context).map_err(failure)
    }
    #[wasm_bindgen(js_name = blobGetPolicy)]
    pub fn get(sha: &str) -> Result<String, String> {
        get_policy_json(sha).map_err(failure)
    }
    #[wasm_bindgen(js_name = blobRateConfig)]
    pub fn config() -> String {
        rate_config_json()
    }
    #[wasm_bindgen(js_name = blobRateDecision)]
    pub fn rate(context: &str) -> Result<String, String> {
        rate_decision_json(context).map_err(failure)
    }
    #[wasm_bindgen(js_name = BlobDigest)]
    #[derive(Default)]
    pub struct WasmBlobDigest(BlobDigest);
    #[wasm_bindgen(js_class = BlobDigest)]
    impl WasmBlobDigest {
        #[wasm_bindgen(constructor)]
        pub fn new() -> Self {
            Self(BlobDigest::new())
        }
        pub fn update(&mut self, bytes: &[u8]) -> Result<(), String> {
            self.0.update(bytes).map_err(failure)
        }
        pub fn finish(self, sha: &str) -> Result<String, String> {
            self.0.finish(sha).map_err(failure)
        }
        #[wasm_bindgen(js_name = finishHash)]
        pub fn finish_hash(self) -> String {
            self.0.finish_hash()
        }
    }
}

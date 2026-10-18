//! Identity-provider proofs bind an authenticated provider subject to an independently
//! generated Zoen identity. They never mint device certificates or decrypt history.

use roda_types::Identity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A short-lived opaque proof plus a root-certified device's explicit authorization.
/// Deliberately has no Debug implementation: proofs must not enter application logs.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderBindRequest {
    pub proof_token: String,
    pub profile: Identity,
    pub device: String,
    pub certificate: String,
    pub signature: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderBindResponse {
    pub identity_id: String,
    pub handle: String,
}

/// Exact domain-separated bytes signed by the device. Profile encoding is shared
/// between native, WASM and the relay; JSON formatting supplied by a caller is ignored.
pub fn provider_bind_message(
    proof_token: &str,
    profile: &Identity,
    device: &str,
    certificate: &str,
) -> Result<Vec<u8>, serde_json::Error> {
    let profile = serde_json::to_vec(profile)?;
    Ok(format!(
        "zoen.provider.bind.v1\n{}\n{}\n{}\n{}",
        hex::encode(Sha256::digest(proof_token.as_bytes())),
        hex::encode(Sha256::digest(profile)),
        device,
        certificate,
    )
    .into_bytes())
}

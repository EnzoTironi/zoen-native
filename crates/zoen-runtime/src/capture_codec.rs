//! Preserve old JSON exactly when it fits; larger chat captures keep provider
//! evidence as raw binary rather than a JSON byte array. Neither is authority.
#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::perf,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod bounded;
use crate::{custody::Custody, Binding, Capture, RuntimeError};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;
use zoen_models::{
    ModelAttemptResult, OutputBlock, OutputFailure, ProviderEvidence, ProviderReceipt,
};
pub(crate) const MAXIMUM: usize = 34 * 1024 * 1024;
pub(crate) const MAX_RESPONSE: usize = 16 * 1024 * 1024;
pub(crate) const INVENTORY_MAXIMUM: usize = 256 * 1024 * 1024;
const MAGIC: &[u8] = b"ZOEN-CHAT-EVIDENCE/1\0";
const MAX_HEADER: usize = MAX_RESPONSE + 256 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    binding: Binding,
    output: Result<Vec<OutputBlock>, OutputFailure>,
    receipt: ProviderReceipt,
    complete: bool,
    body: u64,
}
fn validate(capture: &Capture) -> Result<(), RuntimeError> {
    if capture.result.context != capture.binding.context
        || capture.result.request_digest != capture.binding.request_digest
        || capture.result.evidence.body.len() > MAX_RESPONSE
        || u64::try_from(capture.result.evidence.body.len())
            .map_err(|_| RuntimeError::EvidenceTooLarge)?
            > capture.binding.price.chat()?.max_response_bytes
    {
        return Err(RuntimeError::InvalidBinding);
    }
    capture.binding.price.chat()?.charge(&capture.result)?;
    Ok(())
}
pub(crate) fn encode(capture: &Capture) -> Result<Zeroizing<Vec<u8>>, RuntimeError> {
    validate(capture)?;
    if let Ok(bytes) = bounded::json(capture, crate::custody::MAX_SEALED - 40) {
        return Ok(Zeroizing::new(bytes));
    }
    let header = Header {
        binding: capture.binding.clone(),
        output: capture.result.output.clone(),
        receipt: capture.result.receipt.clone(),
        complete: capture.result.evidence.complete,
        body: u64::try_from(capture.result.evidence.body.len())
            .map_err(|_| RuntimeError::EvidenceTooLarge)?,
    };
    let header = Zeroizing::new(bounded::json(&header, MAX_HEADER)?);
    let total = MAGIC
        .len()
        .checked_add(4)
        .and_then(|v| v.checked_add(header.len()))
        .and_then(|v| v.checked_add(capture.result.evidence.body.len()))
        .filter(|v| v.checked_add(40).is_some_and(|v| v <= MAXIMUM))
        .ok_or(RuntimeError::EvidenceTooLarge)?;
    let mut bytes = Zeroizing::new(Vec::with_capacity(total));
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(
        &u32::try_from(header.len())
            .map_err(|_| RuntimeError::EvidenceTooLarge)?
            .to_be_bytes(),
    );
    bytes.extend_from_slice(&header);
    bytes.extend_from_slice(&capture.result.evidence.body);
    Ok(bytes)
}
pub(crate) fn decode(bytes: &[u8]) -> Result<Capture, RuntimeError> {
    if bytes.len() > MAXIMUM - 40 {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    if !bytes.starts_with(MAGIC) {
        if bytes.len() > crate::custody::MAX_SEALED - 40 {
            return Err(RuntimeError::InvalidBinding);
        }
        let capture: Capture =
            serde_json::from_slice(bytes).map_err(|_| RuntimeError::InvalidBinding)?;
        validate(&capture)?;
        return Ok(capture);
    }
    let split = MAGIC.len() + 4;
    let length = usize::try_from(u32::from_be_bytes(
        bytes
            .get(MAGIC.len()..split)
            .ok_or(RuntimeError::InvalidBinding)?
            .try_into()
            .map_err(|_| RuntimeError::InvalidBinding)?,
    ))
    .map_err(|_| RuntimeError::InvalidBinding)?;
    if length > MAX_HEADER {
        return Err(RuntimeError::InvalidBinding);
    }
    let end = split
        .checked_add(length)
        .ok_or(RuntimeError::InvalidBinding)?;
    let header: Header =
        serde_json::from_slice(bytes.get(split..end).ok_or(RuntimeError::InvalidBinding)?)
            .map_err(|_| RuntimeError::InvalidBinding)?;
    let body = bytes.get(end..).ok_or(RuntimeError::InvalidBinding)?;
    if u64::try_from(body.len()).map_err(|_| RuntimeError::InvalidBinding)? != header.body {
        return Err(RuntimeError::InvalidBinding);
    }
    let result = ModelAttemptResult {
        context: header.binding.context.clone(),
        request_digest: header.binding.request_digest.clone(),
        output: header.output,
        receipt: header.receipt,
        evidence: ProviderEvidence {
            body: body.to_vec(),
            complete: header.complete,
        },
    };
    let capture = Capture {
        binding: header.binding,
        result,
    };
    validate(&capture)?;
    Ok(capture)
}
pub(crate) fn seal(
    custody: &Custody,
    capture: &Capture,
) -> Result<(String, Vec<u8>), RuntimeError> {
    custody.seal_bounded_bytes(
        &format!("evidence/{}", capture.binding.context.attempt_id),
        &encode(capture)?,
        MAXIMUM,
    )
}
pub(crate) fn open(
    custody: &Custody,
    attempt: &str,
    digest: &str,
    sealed: &[u8],
) -> Result<Capture, RuntimeError> {
    let plain = custody.open_bounded_bytes(
        &format!("evidence/{attempt}"),
        Some(digest),
        sealed,
        MAXIMUM,
    )?;
    let capture = decode(&plain)?;
    if capture.binding.context.attempt_id != attempt {
        return Err(RuntimeError::InvalidBinding);
    }
    Ok(capture)
}
pub(crate) fn digest(capture: &Capture) -> Result<String, RuntimeError> {
    Ok(crate::hash(&encode(capture)?))
}

/// Operation-local authentication proof, never execution or publication rights.
/// Construct before opening a database transaction; matching the exact bytes
/// later does not repeat bulk decryption while holding database locks.
pub(crate) struct VerifiedCapture {
    capture: Capture,
    digest: String,
    sealed_digest: String,
}
impl VerifiedCapture {
    pub(crate) fn capture(&self) -> &Capture {
        &self.capture
    }
    pub(crate) fn matches(&self, digest: &str, sealed_digest: &str) -> bool {
        self.digest == digest && self.sealed_digest == sealed_digest
    }
}
pub(crate) fn authenticate(
    custody: &Custody,
    attempt: &str,
    digest: &str,
    sealed: &[u8],
) -> Result<VerifiedCapture, RuntimeError> {
    Ok(VerifiedCapture {
        capture: open(custody, attempt, digest, sealed)?,
        digest: digest.into(),
        sealed_digest: crate::hash(sealed),
    })
}
#[cfg(test)]
mod tests;

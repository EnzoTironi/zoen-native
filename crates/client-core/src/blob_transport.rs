//! The ordinary signed relay blob PUT. A receipt is an observed HTTP response,
//! never permission to upload, publish or alter a document.
use event_log::Signer;
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum BlobUploadError {
    #[error("invalid encrypted blob upload")]
    Invalid,
    #[error("relay blob upload refused ({0})")]
    Refused(u16),
    #[error("relay blob upload acknowledgement unavailable")]
    Unavailable,
}

/// Only a successful response to the exact signed PUT constructs this value.
#[derive(Debug)]
pub struct BlobUploadReceipt {
    blob: String,
    device: String,
    bytes: u64,
}
impl BlobUploadReceipt {
    #[must_use]
    pub fn blob(&self) -> &str {
        &self.blob
    }
    #[must_use]
    pub fn device(&self) -> &str {
        &self.device
    }
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

pub(crate) async fn put(
    http: &reqwest::Client,
    base: &str,
    relay: &str,
    key: &Signer,
    sha: &str,
    bytes: Vec<u8>,
    at_ms: i64,
) -> Result<BlobUploadReceipt, BlobUploadError> {
    upload(
        http,
        base,
        relay,
        key,
        Blob {
            sha,
            bytes,
            at_ms,
            binding: None,
        },
    )
    .await
}

pub(crate) struct BoundUpload<'a> {
    pub base: &'a str,
    pub relay: &'a str,
    pub key: &'a Signer,
    pub binding: &'a protocol::runtime_binding::BlobBinding,
}

pub(crate) async fn put_bound(
    http: &reqwest::Client,
    target: BoundUpload<'_>,
    sha: &str,
    bytes: Vec<u8>,
    at_ms: i64,
) -> Result<BlobUploadReceipt, BlobUploadError> {
    upload(
        http,
        target.base,
        target.relay,
        target.key,
        Blob {
            sha,
            bytes,
            at_ms,
            binding: Some(target.binding),
        },
    )
    .await
}

struct Blob<'a> {
    sha: &'a str,
    bytes: Vec<u8>,
    at_ms: i64,
    binding: Option<&'a protocol::runtime_binding::BlobBinding>,
}

async fn upload(
    http: &reqwest::Client,
    base: &str,
    relay: &str,
    key: &Signer,
    blob: Blob<'_>,
) -> Result<BlobUploadReceipt, BlobUploadError> {
    let Blob {
        sha,
        bytes,
        at_ms,
        binding,
    } = blob;
    if relay.is_empty()
        || at_ms < 0
        || bytes.is_empty()
        || bytes.len() > 25 * 1024 * 1024
        || sha != hex::encode(Sha256::digest(&bytes))
    {
        return Err(BlobUploadError::Invalid);
    }
    let length = u64::try_from(bytes.len()).map_err(|_| BlobUploadError::Invalid)?;
    let message = match binding {
        Some(b) => b
            .message(sha, at_ms, relay)
            .ok_or(BlobUploadError::Invalid)?,
        None => protocol::blob_put_message(sha, at_ms, relay),
    };
    let mut request = http
        .put(format!("{base}/v1/blobs/{sha}"))
        .header("x-zoen-device", key.id())
        .header("x-zoen-ts", at_ms.to_string())
        .header("x-zoen-sig", key.sign(&message));
    if let Some(binding) = binding {
        request = request.header(
            protocol::runtime_binding::BLOB_HEADER,
            serde_json::to_string(binding).map_err(|_| BlobUploadError::Invalid)?,
        );
    }
    let response = request
        .body(bytes)
        .send()
        .await
        .map_err(|_| BlobUploadError::Unavailable)?;
    match response.status().as_u16() {
        200 | 201 => Ok(BlobUploadReceipt {
            blob: sha.into(),
            device: key.id(),
            bytes: length,
        }),
        status => Err(BlobUploadError::Refused(status)),
    }
}

#[cfg(test)]
mod tests;

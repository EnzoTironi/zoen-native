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
    pub fn blob(&self) -> &str {
        &self.blob
    }
    pub fn device(&self) -> &str {
        &self.device
    }
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
    if relay.is_empty()
        || at_ms < 0
        || bytes.is_empty()
        || bytes.len() > 25 * 1024 * 1024
        || sha != hex::encode(Sha256::digest(&bytes))
    {
        return Err(BlobUploadError::Invalid);
    }
    let length = u64::try_from(bytes.len()).map_err(|_| BlobUploadError::Invalid)?;
    let response = http
        .put(format!("{base}/v1/blobs/{sha}"))
        .header("x-zoen-device", key.id())
        .header("x-zoen-ts", at_ms.to_string())
        .header(
            "x-zoen-sig",
            key.sign(&protocol::blob_put_message(sha, at_ms, relay)),
        )
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

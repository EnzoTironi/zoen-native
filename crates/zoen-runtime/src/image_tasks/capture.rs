//! Bounded binary evidence. PNG/base64 bodies never expand to JSON byte arrays.
use super::*;
use zoen_models::{
    GeneratedImage, ImageUsageEvidence, OutputFailure, ProviderEvidence, ProviderReceipt,
};

pub(super) const MAGIC: &[u8] = b"ZOEN-IMAGE-EVIDENCE/1\0";
pub(super) const MAXIMUM: usize = 32 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    task: ImageTask,
    receipt: ProviderReceipt,
    usage: ImageUsageEvidence,
    complete: bool,
    nonce: String,
    body: u64,
    output: Result<ImageHeader, OutputFailure>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImageHeader {
    bytes: u64,
    width: u32,
    height: u32,
    sha256: String,
}
pub(crate) struct ImageCapture {
    pub(crate) task: ImageTask,
    pub(crate) result: ImageAttemptResult,
    pub(crate) nonce: String,
}
impl ImageCapture {
    pub(crate) fn encode(&self) -> Result<Vec<u8>, RuntimeError> {
        self.task.validate()?;
        if !valid_digest(&self.nonce) {
            return Err(RuntimeError::InvalidBinding);
        }
        if self.result.context != self.task.binding.context
            || self.result.request_digest != self.task.binding.request_digest
            || self.result.evidence.body.len() as u64 > self.task.dispatch.max_response_bytes
        {
            return Err(RuntimeError::InvalidBinding);
        }
        if let Ok(image) = &self.result.output {
            if image.mime != "image/png"
                || image.bytes.is_empty()
                || image.bytes.len() as u64 > self.task.request.max_image_bytes
                || (image.width, image.height) != self.task.request.size.dimensions()
                || image.sha256 != hash(&image.bytes)
                || !image.bytes.starts_with(b"\x89PNG\r\n\x1a\n")
            {
                return Err(RuntimeError::InvalidBinding);
            }
        }
        let output = self
            .result
            .output
            .as_ref()
            .map(|image| ImageHeader {
                bytes: image.bytes.len() as u64,
                width: image.width,
                height: image.height,
                sha256: image.sha256.clone(),
            })
            .map_err(|error| *error);
        let header = Header {
            task: self.task.clone(),
            receipt: self.result.receipt.clone(),
            usage: self.result.image_usage.clone(),
            complete: self.result.evidence.complete,
            nonce: self.nonce.clone(),
            body: self.result.evidence.body.len() as u64,
            output,
        };
        let header = serde_json::to_vec(&header).map_err(|_| RuntimeError::InvalidBinding)?;
        if header.len() > 256 * 1024 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let image = self
            .result
            .output
            .as_ref()
            .map(|image| image.bytes.as_slice())
            .unwrap_or_default();
        let total = MAGIC
            .len()
            .checked_add(4)
            .and_then(|n| n.checked_add(header.len()))
            .and_then(|n| n.checked_add(self.result.evidence.body.len()))
            .and_then(|n| n.checked_add(image.len()))
            .filter(|n| n.checked_add(40).is_some_and(|n| n <= MAXIMUM))
            .ok_or(RuntimeError::EvidenceTooLarge)?;
        let mut bytes = Vec::with_capacity(total);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&(header.len() as u32).to_be_bytes());
        bytes.extend(header);
        bytes.extend_from_slice(&self.result.evidence.body);
        bytes.extend_from_slice(image);
        Ok(bytes)
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, RuntimeError> {
        if bytes.len() > MAXIMUM - 40 || !bytes.starts_with(MAGIC) {
            return Err(RuntimeError::InvalidBinding);
        }
        let split = MAGIC
            .len()
            .checked_add(4)
            .ok_or(RuntimeError::InvalidBinding)?;
        let len = u32::from_be_bytes(
            bytes
                .get(MAGIC.len()..split)
                .ok_or(RuntimeError::InvalidBinding)?
                .try_into()
                .map_err(|_| RuntimeError::InvalidBinding)?,
        ) as usize;
        if len > 256 * 1024 {
            return Err(RuntimeError::InvalidBinding);
        }
        let end = split.checked_add(len).ok_or(RuntimeError::InvalidBinding)?;
        let header: Header =
            serde_json::from_slice(bytes.get(split..end).ok_or(RuntimeError::InvalidBinding)?)
                .map_err(|_| RuntimeError::InvalidBinding)?;
        header.task.validate()?;
        if !valid_digest(&header.nonce) {
            return Err(RuntimeError::InvalidBinding);
        }
        let body_len = usize::try_from(header.body).map_err(|_| RuntimeError::InvalidBinding)?;
        if header.body > header.task.dispatch.max_response_bytes {
            return Err(RuntimeError::InvalidBinding);
        }
        let body_end = end
            .checked_add(body_len)
            .ok_or(RuntimeError::InvalidBinding)?;
        let body = bytes
            .get(end..body_end)
            .ok_or(RuntimeError::InvalidBinding)?
            .to_vec();
        let image = bytes.get(body_end..).ok_or(RuntimeError::InvalidBinding)?;
        let output = match header.output {
            Ok(meta) => {
                let dimensions = header.task.request.size.dimensions();
                if meta.bytes != image.len() as u64
                    || meta.bytes == 0
                    || meta.bytes > header.task.request.max_image_bytes
                    || meta.sha256 != hash(image)
                    || (meta.width, meta.height) != dimensions
                    || !image.starts_with(b"\x89PNG\r\n\x1a\n")
                {
                    return Err(RuntimeError::InvalidBinding);
                }
                Ok(GeneratedImage {
                    bytes: image.to_vec(),
                    mime: "image/png",
                    width: meta.width,
                    height: meta.height,
                    sha256: meta.sha256,
                })
            }
            Err(error) => {
                if !image.is_empty() {
                    return Err(RuntimeError::InvalidBinding);
                }
                Err(error)
            }
        };
        let result = ImageAttemptResult {
            context: header.task.request.context.clone(),
            request_digest: header.task.binding.request_digest.clone(),
            receipt: header.receipt,
            image_usage: header.usage,
            evidence: ProviderEvidence {
                body,
                complete: header.complete,
            },
            output,
        };
        Ok(Self {
            task: header.task,
            result,
            nonce: header.nonce,
        })
    }
}

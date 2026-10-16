//! Original output associations. Retained metadata grants no live permission.
mod prepare;
mod receipt;
mod storage;
#[cfg(test)]
pub(crate) mod testing;

use foundationdb::Transaction;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use zoen_ffi::runtime_core::{PreparedReply, ReplyReceipt};
use zoen_models::OutputBlock;

use super::*;
use crate::{Capture, ReplyOutputInspection, ReplyReceiptProgress, VerifiedStep};

const METADATA_BYTES: usize = 16 * 1024;

// No production constructor or read reconstruction exists. Only a cfg(test)
// empty-namespace transaction can establish this private fixture capability.
pub(crate) struct FreshOutputNamespace {
    namespace: Vec<u8>,
    deployment: String,
    source: String,
    #[cfg(test)]
    observed: std::sync::Mutex<std::collections::BTreeMap<String, zoen_proto::Envelope>>,
    #[cfg(test)]
    output_expiry_remaining: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl FreshOutputNamespace {
    async fn check_in(&self, execution: &Execution, trx: &Transaction) -> Result<(), RuntimeError> {
        if self.namespace != execution.root.bytes()
            || self.deployment != execution.deployment
            || self.source != execution.reply_source()?.cell()
        {
            return Err(RuntimeError::DeploymentMismatch);
        }
        // The owner transaction independently conflicts on its existing markers.
        let source = trx
            .get(&execution.root.pack(&("relay-source/1",)), false)
            .await?;
        if source.as_deref() != Some(self.source.as_bytes()) {
            return Err(RuntimeError::DeploymentMismatch);
        }
        Ok(())
    }
}

struct SealedRecord<T> {
    value: T,
    digest: String,
    bytes: Vec<u8>,
}

fn seal_metadata<T: Serialize>(
    custody: &Custody,
    subject: &str,
    value: T,
) -> Result<SealedRecord<T>, RuntimeError> {
    let plain = serde_json::to_vec(&value).map_err(|_| RuntimeError::InvalidBinding)?;
    if plain.len() > METADATA_BYTES {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    let (_, bytes) = custody.seal_bytes(subject, &plain)?;
    Ok(SealedRecord {
        value,
        digest: hash(&bytes),
        bytes,
    })
}

fn open_metadata<T: DeserializeOwned + Serialize>(
    custody: &Custody,
    subject: &str,
    bytes: &[u8],
) -> Result<SealedRecord<T>, RuntimeError> {
    if bytes.len() > METADATA_BYTES + 40 {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    let plain = custody.open_bytes(subject, None, bytes)?;
    let value = serde_json::from_slice(&plain).map_err(|_| RuntimeError::InvalidBinding)?;
    if serde_json::to_vec(&value)
        .map_err(|_| RuntimeError::InvalidBinding)?
        .as_slice()
        != plain.as_slice()
    {
        return Err(RuntimeError::InvalidBinding);
    }
    Ok(SealedRecord {
        value,
        digest: hash(bytes),
        bytes: bytes.to_vec(),
    })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeCaptureRecord {
    format: u8,
    source: String,
    run: String,
    attempt: String,
    original: String,
    binding: String,
    original_capsule: String,
    admitted_nonce: String,
    evidence: String,
}

impl NativeCaptureRecord {
    fn validate(&self, record: &RunRecord, original: &str) -> Result<(), RuntimeError> {
        if self.format != 1
            || self.source != record.source
            || self.run != record.locator.run
            || self.attempt != record.locator.attempt
            || self.original != original
            || self.binding != record.binding.digest()?
            || self.original_capsule != record.original.manifest_digest
            || !id(&self.admitted_nonce)
            || !digest(&self.evidence)
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
}

struct RetainedCapture {
    association: SealedRecord<NativeCaptureRecord>,
    capture: Capture,
}

impl RetainedCapture {
    fn text(&self, record: &RunRecord) -> Result<&str, RuntimeError> {
        let result = &self.capture.result;
        let Ok(blocks) = &result.output else {
            return Err(RuntimeError::Denied);
        };
        let [OutputBlock::Text { text }] = blocks.as_slice() else {
            return Err(RuntimeError::Denied);
        };
        if !result.evidence.complete
            || !result
                .receipt
                .http_status
                .is_some_and(|s| (200..300).contains(&s))
            || result.receipt.reported_model.as_deref() != Some(record.binding.price.model.as_str())
            || text.trim().is_empty()
            || text.len() > 32768
        {
            return Err(RuntimeError::Denied);
        }
        Ok(text)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicationRecord {
    format: u8,
    run: String,
    original: String,
    binding: String,
    capture: String,
    intent: ReplyIntent,
    client_id: String,
    message: String,
    inner: String,
    reference: String,
    publish: String,
    root: Root,
}

impl PublicationRecord {
    fn validate(
        &self,
        record: &RunRecord,
        original: &str,
        capture: &str,
    ) -> Result<(), RuntimeError> {
        let principal = &record.original.context.principal;
        self.root
            .context
            .validate(principal, record.original.context.key_version)?;
        if self.format != 1
            || self.run != record.locator.run
            || self.original != original
            || self.binding != record.binding.digest()?
            || self.capture != capture
            || self.intent != record.intent
            || !id(&self.client_id)
            || !id(&self.message)
            || ![
                &self.inner,
                &self.reference,
                &self.publish,
                &self.root.manifest_digest,
            ]
            .into_iter()
            .all(|v| digest(v))
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }

    fn inspection(&self) -> ReplyOutputInspection {
        ReplyOutputInspection {
            client_id: self.client_id.clone(),
            message: self.message.clone(),
            generation: self.root.context.generation,
        }
    }

    fn matches_receipt(&self, proof: &ReplyReceipt) -> bool {
        let (intent, cid, message, inner, reference, publish) = match proof {
            ReplyReceipt::Accepted(p) => (
                p.original(),
                p.client_id(),
                p.message(),
                p.event_digest(),
                p.reference_digest(),
                p.publish_digest(),
            ),
            ReplyReceipt::Prefix(p) => (
                p.original(),
                p.client_id(),
                p.message(),
                p.event_digest(),
                p.reference_digest(),
                p.publish_digest(),
            ),
        };
        intent == &self.intent
            && cid == self.client_id
            && message == self.message
            && inner == self.inner
            && reference == self.reference
            && publish == self.publish
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptRecord {
    format: u8,
    original: String,
    capture: String,
    publication: String,
    reference: String,
    publish: String,
    accepted: Seen,
    source: Seen,
    root: Root,
}

impl ReceiptRecord {
    fn validate(&self, publication: &SealedRecord<PublicationRecord>) -> Result<(), RuntimeError> {
        let p = &publication.value;
        self.root
            .context
            .validate(&p.root.context.principal, p.root.context.key_version)?;
        if self.format != 1
            || self.original != p.original
            || self.capture != p.capture
            || self.publication != publication.digest
            || self.reference != p.reference
            || self.publish != p.publish
            || self.source.seq < self.accepted.seq
            || !digest(&self.accepted.hash)
            || !digest(&self.source.hash)
            || !digest(&self.root.manifest_digest)
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ReplyOutputScope {
    image: ActivationScope,
    source: String,
    run: String,
    original: String,
    binding: String,
    capture: String,
    run_fence: Fence,
    intent: ReplyIntent,
    frontier: Seen,
    epoch: u64,
    expires_at_ms: Option<i64>,
    publication: String,
}

impl ReplyOutputScope {
    pub(crate) fn matches_directory(
        &self,
        owner: &str,
        agent: &str,
        device: &str,
        certificate: &str,
    ) -> bool {
        self.image
            .matches_directory(owner, agent, device, certificate)
    }
    pub(crate) fn expires_at_ms(&self) -> Option<i64> {
        self.expires_at_ms
    }
}

pub(crate) async fn associate_capture(
    runtime: &RuntimeAuthority,
    step: &VerifiedStep,
    capture: &Capture,
    nonce: &str,
    evidence: &str,
    sealed: &[u8],
) -> Result<(), RuntimeError> {
    #[cfg(not(test))]
    let VerifiedStep::Native(step) = step;
    #[cfg(test)]
    let step = match step {
        VerifiedStep::Native(step) => step,
        #[cfg(test)]
        VerifiedStep::Fixture(_) => return Err(RuntimeError::Denied),
    };
    #[cfg(test)]
    let capability = runtime
        .output_fixture
        .as_ref()
        .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
    #[cfg(not(test))]
    let capability = runtime.production_output_scope()?;
    #[cfg(test)]
    if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 40 {
        return Err(RuntimeError::Unavailable);
    }
    runtime
        .execution
        .associate_native_capture(
            capability,
            &step.loaded,
            capture,
            nonce,
            evidence,
            sealed,
            &runtime.custody,
        )
        .await
}

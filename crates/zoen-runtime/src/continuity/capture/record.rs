//! References to original encrypted evidence; no provider or publication right.
use super::*;
const PREFIX: &str = "capture_v1_";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Pointer {
    pub(super) attempt: String,
    pub(super) digest: String,
    sealed_digest: String,
    root_digest: String,
    binding_digest: String,
    nonce_digest: String,
}
pub(super) struct Original<'a> {
    pub(super) capture: &'a crate::Capture,
    pub(super) sealed: Vec<u8>,
    pub(super) nonce: String,
}
impl Pointer {
    pub(super) fn new(
        capture: &crate::Capture,
        digest: &str,
        sealed: &[u8],
        root: &[u8],
        nonce: &str,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            attempt: capture.binding.context.attempt_id.clone(),
            digest: digest.into(),
            sealed_digest: crate::hash(sealed),
            root_digest: crate::hash(root),
            binding_digest: capture.binding.digest()?,
            nonce_digest: crate::hash(nonce.as_bytes()),
        })
    }
    pub(super) fn key(&self, runtime: &RuntimeAuthority) -> Vec<u8> {
        runtime.execution.root.pack(&(
            "shared-native-capture/1",
            self.attempt.as_str(),
            self.digest.as_str(),
        ))
    }
    pub(super) fn subject(&self) -> Result<String, RuntimeError> {
        Ok(crate::hash(
            &serde_json::to_vec(&("runtime-capture/1", &self.attempt, &self.digest))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
    pub(super) fn bytes(&self) -> Result<Vec<u8>, RuntimeError> {
        serde_json::to_vec(self).map_err(|_| RuntimeError::InvalidBinding)
    }
    pub(super) fn update(&self) -> Result<SubjectUpdate, RuntimeError> {
        Ok(SubjectUpdate {
            subject: self.subject()?,
            previous: None,
            next: crate::hash(&self.bytes()?),
        })
    }
    pub(super) fn evidence_key(&self, runtime: &RuntimeAuthority) -> Vec<u8> {
        runtime
            .execution
            .root
            .pack(&("evidence", self.attempt.as_str(), self.digest.as_str()))
    }
    /// Check the actual snapshot against the exact ciphertext commitment. The
    /// independent subject authenticates this pointer; reading intact bytes
    /// does not require decrypting every historical response under a DB lock.
    pub(super) async fn verify(
        &self,
        runtime: &RuntimeAuthority,
        tx: &foundationdb::Transaction,
        root: &[u8],
    ) -> Result<(Vec<u8>, crate::Binding, String), RuntimeError> {
        if crate::hash(root) != self.root_digest
            || crate::execution::Execution::evidence_payload_size(root)?
                > crate::capture_codec::MAXIMUM
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let sealed = runtime
            .execution
            .read_evidence_payload_in(tx, &self.attempt, &self.digest, root)
            .await?;
        if crate::hash(&sealed) != self.sealed_digest {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let prepared = tx
            .get(&runtime.execution.attempt(&self.attempt, "prepared"), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let binding: crate::Binding =
            runtime
                .custody
                .open(&format!("prepared/{}", self.attempt), None, &prepared)?;
        if binding.context.attempt_id != self.attempt || binding.digest()? != self.binding_digest {
            return Err(RuntimeError::InvalidBinding);
        }
        let admitted = tx
            .get(&runtime.execution.attempt(&self.attempt, "admitted"), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if crate::hash(&admitted) != self.nonce_digest {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let nonce =
            String::from_utf8(admitted.to_vec()).map_err(|_| RuntimeError::InvalidBinding)?;
        Ok((sealed, binding, nonce))
    }
    pub(super) async fn original<'a>(
        &self,
        runtime: &RuntimeAuthority,
        tx: &foundationdb::Transaction,
        root: &[u8],
        verified: &'a crate::capture_codec::VerifiedCapture,
    ) -> Result<Original<'a>, RuntimeError> {
        let (sealed, binding, nonce) = self.verify(runtime, tx, root).await?;
        let capture = verified.capture();
        if capture.binding != binding
            || capture.result.context != binding.context
            || capture.result.request_digest != binding.request_digest
            || !verified.matches(&self.digest, &self.sealed_digest)
        {
            return Err(RuntimeError::InvalidBinding);
        }
        binding.price.chat()?.charge(&capture.result)?;
        Ok(Original {
            capture,
            sealed,
            nonce,
        })
    }
    pub(super) async fn stage(
        &self,
        runtime: &RuntimeAuthority,
        tx: &foundationdb::Transaction,
        root: &[u8],
    ) -> Result<(), RuntimeError> {
        for (key, bytes) in [
            (self.key(runtime), self.bytes()?),
            (self.evidence_key(runtime), root.to_vec()),
        ] {
            if let Some(prior) = tx.get(&key, false).await? {
                if prior.as_ref() != bytes.as_slice() {
                    return Err(RuntimeError::RestoreUnreconciled);
                }
            } else {
                tx.set(&key, &bytes);
            }
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Intent {
    pub(super) pointer: Pointer,
    pub(super) original: String,
    pub(super) financial: SubjectUpdate,
}
impl Intent {
    pub(super) fn updates(&self) -> Result<[SubjectUpdate; 2], RuntimeError> {
        Ok([self.pointer.update()?, self.financial.clone()])
    }
    pub(super) fn effect(&self) -> Result<String, RuntimeError> {
        Ok(crate::hash(
            &serde_json::to_vec(&("runtime-capture/1", self))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
    pub(super) fn seal(&self, runtime: &RuntimeAuthority) -> Result<String, RuntimeError> {
        let (_, bytes) = runtime.custody.seal("runtime-capture/1", self)?;
        let encoded = format!("{PREFIX}{}", STANDARD.encode(bytes));
        if encoded.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        Ok(encoded)
    }
    pub(super) fn decode(
        runtime: &RuntimeAuthority,
        prepared: &Prepared,
    ) -> Result<Self, RuntimeError> {
        let encoded = prepared
            .intent
            .as_deref()
            .and_then(|v| v.strip_prefix(PREFIX))
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if encoded.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let intent: Self = runtime.custody.open("runtime-capture/1", None, &bytes)?;
        if prepared.retirement
            || prepared.effect_digest != intent.effect()?
            || prepared.subject_updates != intent.updates()?
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(intent)
    }
}
pub(in crate::continuity) fn capture_intent(prepared: &Prepared) -> bool {
    prepared
        .intent
        .as_deref()
        .is_some_and(|v| v.starts_with(PREFIX))
}

mod source;
pub(super) use source::{Source, SOURCE_MAXIMUM};

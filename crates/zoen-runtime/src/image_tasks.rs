mod association;
pub(crate) mod capture;
mod execution;
#[cfg(test)]
pub(crate) use execution::ImageAttemptAdapter;
// Image tasks reuse the existing immutable binding, finance and admission stores.
// No public constructor turns a task record into authority.
use crate::*;
use zoen_models::{DispatchDescriptor, ImageAttemptResult, ImageRequest};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImageTask {
    pub(crate) schema: u8,
    pub(crate) request: ImageRequest,
    pub(crate) dispatch: DispatchDescriptor,
    pub(crate) binding: Binding,
    /// The signed native source and exact reviewed operation, not a host label.
    pub(crate) source_digest: String,
    pub(crate) task_digest: String,
    pub(crate) request_id: String,
    pub(crate) approval_hash: String,
}

impl ImageTask {
    pub(crate) fn validate(&self) -> Result<(), RuntimeError> {
        let b = &self.binding;
        let p = b.price.image()?;
        let dispatch = DispatchRequest {
            context: self.request.context.clone(),
            request_digest: b.request_digest.clone(),
            profile: zoen_models::IMAGE_PROFILE,
            descriptor: self.dispatch.clone(),
        };
        let quote = p.quote(&dispatch)?;
        if self.schema != 1
            || b.context != self.request.context
            || b.requested_output_tokens != 0
            || b.hold_units != quote.reservation_units()
            || self.request.prompt.trim().is_empty()
            || self.request.prompt.contains('\0')
            || self.request.prompt.chars().count() > 32_000
            || self.request.size != p.image.size
            || self.request.quality != p.image.quality
            || self.request.background != p.image.background
            || self.request.max_image_bytes != p.image.max_image_bytes
            || !valid_digest(&self.source_digest)
            || !id(&self.request_id)
            || !valid_digest(&self.approval_hash)
            || self.task_digest != self.expected_digest()?
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
    pub(crate) fn expected_digest(&self) -> Result<String, RuntimeError> {
        Ok(hash(
            &serde_json::to_vec(&(
                "native-image-task/1",
                &self.request,
                &self.dispatch,
                &self.binding,
                &self.source_digest,
                &self.request_id,
                &self.approval_hash,
            ))
            .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
    pub(crate) fn charge(&self, result: &ImageAttemptResult) -> Result<Option<i64>, RuntimeError> {
        self.validate()?;
        self.binding
            .price
            .image()?
            .quote(&DispatchRequest {
                context: self.request.context.clone(),
                request_digest: self.binding.request_digest.clone(),
                profile: zoen_models::IMAGE_PROFILE,
                descriptor: self.dispatch.clone(),
            })?
            .charge(result)
    }
    /// An amount outside ledger integer range is an unresolved obligation, not
    /// zero and never a reason to drop the original incurred evidence.
    pub(crate) fn retained_charge(
        &self,
        result: &ImageAttemptResult,
    ) -> Result<Option<i64>, RuntimeError> {
        self.validate()?;
        match self.charge(result) {
            Err(RuntimeError::InvalidProfile) => Ok(None),
            result => result,
        }
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}

impl RuntimeAuthority {
    pub(super) async fn capture_retained_evidence(
        &self,
        attempt: &str,
        digest: &str,
        sealed: &[u8],
    ) -> Result<FinancialState, RuntimeError> {
        let plain = self.custody.open_bounded_bytes(
            &format!("evidence/{attempt}"),
            Some(digest),
            sealed,
            capture::MAXIMUM,
        )?;
        if plain.starts_with(capture::MAGIC) {
            let capture = capture::ImageCapture::decode(&plain)?;
            if capture.task.binding.context.attempt_id != attempt {
                return Err(RuntimeError::InvalidBinding);
            }
            let charge = capture.task.retained_charge(&capture.result)?;
            self.finance
                .settle_capture(&capture.task.binding, charge, digest, sealed)
                .await
        } else {
            // Legacy chat stays bounded by the original limit and JSON format.
            if sealed.len() > custody::MAX_SEALED {
                return Err(RuntimeError::InvalidBinding);
            }
            let capture: Capture =
                serde_json::from_slice(&plain).map_err(|_| RuntimeError::InvalidBinding)?;
            if capture.binding.context.attempt_id != attempt {
                return Err(RuntimeError::InvalidBinding);
            }
            self.finance.capture(&capture, digest, sealed).await
        }
    }
}

//! Inert image pricing only. This grants no task, SQL hold or dispatch authority.
mod endpoint;
mod evidence;
pub use endpoint::ImageEndpointPolicy;
#[cfg(test)]
mod tests;
use crate::{hash, RuntimeError};
use serde::{Deserialize, Serialize};
use zoen_models::{
    image_response_budget, AttemptContext, DispatchDescriptor, DispatchRequest, ImageAttemptResult,
    ImageDispatch, Operation, IMAGE_PROFILE, MAX_IMAGE_BYTES,
};

/// Host-configured obligation tariffs in USD micro-units per million tokens.
/// Reservation is a configured hold, never an enforced provider cost ceiling.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImagePriceProfile {
    pub version: String,
    pub endpoint: String,
    /// Omitted for existing HTTPS profiles so their signed digests stay stable.
    #[serde(default, skip_serializing_if = "ImageEndpointPolicy::is_https")]
    pub endpoint_policy: ImageEndpointPolicy,
    pub model: String,
    pub credential_ref: String,
    pub currency: String,
    pub scale: u8,
    pub image: ImageDispatch,
    pub input_text_units_per_million: u64,
    pub input_image_units_per_million: u64,
    pub output_text_units_per_million: u64,
    pub output_image_units_per_million: u64,
    pub reservation_units: i64,
    pub max_request_bytes: u64,
    pub max_response_bytes: u64,
}

/// Immutable, local quote, deliberately without Deserialize or an admission impl.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct ImagePriceQuote {
    profile: ImagePriceProfile,
    context: AttemptContext,
    request_digest: String,
    dispatch: DispatchDescriptor,
    profile_digest: String,
    digest: String,
}
impl std::fmt::Debug for ImagePriceQuote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ImagePriceQuote { redacted }")
    }
}
impl ImagePriceProfile {
    pub fn digest(&self) -> Result<String, RuntimeError> {
        self.validate()?;
        Ok(hash(
            &serde_json::to_vec(&("zoen-image-price-v1", IMAGE_PROFILE, self))
                .map_err(|_| RuntimeError::InvalidProfile)?,
        ))
    }
    fn validate(&self) -> Result<(), RuntimeError> {
        let endpoint = url::Url::parse(&self.endpoint).map_err(|_| RuntimeError::InvalidProfile)?;
        if !crate::id(&self.version)
            || !crate::id(&self.model)
            || !self.model.starts_with("gpt-image-")
            || !crate::id(&self.credential_ref)
            || self.endpoint.len() > 2048
            || !self.endpoint_policy.permits(&endpoint)
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.host().is_none()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || endpoint.path() != "/v1/images/generations"
            || self.currency != "USD"
            || self.scale != 6
            || self.image.count != 1
            || self.image.format != "png"
            || !(1..=MAX_IMAGE_BYTES).contains(&self.image.max_image_bytes)
            || !(1..=1_048_576).contains(&self.max_request_bytes)
            || !(1..=16_777_216).contains(&self.max_response_bytes)
            || image_response_budget(self.image.max_image_bytes)
                .is_none_or(|n| n > self.max_response_bytes)
            || self.reservation_units <= 0
            || [
                self.input_text_units_per_million,
                self.input_image_units_per_million,
                self.output_text_units_per_million,
                self.output_image_units_per_million,
            ]
            .iter()
            .all(|v| *v == 0)
        {
            return Err(RuntimeError::InvalidProfile);
        }
        Ok(())
    }
    pub fn quote(&self, request: &DispatchRequest) -> Result<ImagePriceQuote, RuntimeError> {
        self.validate()?;
        let d = &request.descriptor;
        if request.profile != IMAGE_PROFILE
            || d.operation != Operation::ImageGeneration
            || request.context.price_version != self.version
            || d.endpoint != self.endpoint
            || d.model != self.model
            || d.credential_ref != self.credential_ref
            || d.image.as_ref() != Some(&self.image)
            || d.requested_output_tokens != 0
            || !(1..=self.max_request_bytes).contains(&d.max_request_bytes)
            || !(1..=d.max_request_bytes).contains(&d.encoded_request_bytes)
            || !(1..=self.max_response_bytes).contains(&d.max_response_bytes)
            || image_response_budget(self.image.max_image_bytes)
                .is_none_or(|n| n > d.max_response_bytes)
            || request.request_digest.len() != 64
            || !request
                .request_digest
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || [
                &request.context.attempt_id,
                &request.context.run_id,
                &request.context.owner,
                &request.context.agent,
                &request.context.device,
                &request.context.space,
                &request.context.authority_version,
                &request.context.definition_version,
            ]
            .into_iter()
            .any(|v| !crate::id(v))
            || request.context.owner == request.context.agent
        {
            return Err(RuntimeError::InvalidProfile);
        }
        let profile_digest = self.digest()?;
        let digest = hash(
            &serde_json::to_vec(&(
                "zoen-image-quote-v1",
                &profile_digest,
                &request.context,
                &request.request_digest,
                d,
            ))
            .map_err(|_| RuntimeError::InvalidBinding)?,
        );
        Ok(ImagePriceQuote {
            profile: self.clone(),
            context: request.context.clone(),
            request_digest: request.request_digest.clone(),
            dispatch: d.clone(),
            profile_digest,
            digest,
        })
    }
}
impl ImagePriceQuote {
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn profile_digest(&self) -> &str {
        &self.profile_digest
    }
    pub fn context(&self) -> &AttemptContext {
        &self.context
    }
    pub fn request_digest(&self) -> &str {
        &self.request_digest
    }
    pub fn reservation_units(&self) -> i64 {
        self.profile.reservation_units
    }

    /// None is unknown incurred cost, never permission to release the hold.
    /// A known charge can exceed the reservation and is never capped to it.
    /// The optional model echo is checked, not fabricated when Images omits it.
    pub fn charge(&self, result: &ImageAttemptResult) -> Result<Option<i64>, RuntimeError> {
        if result.context != self.context || result.request_digest != self.request_digest {
            return Err(RuntimeError::InvalidBinding);
        }
        evidence::charge(&self.profile, &self.dispatch, result)
    }
}

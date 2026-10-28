//! `OpenAI` Images through the same bounded Rig transport and admission authority.
mod decoding;
mod types;
use crate::{
    receipt, transport, validation, Admission, DispatchAuthority, DispatchDescriptor,
    DispatchRequest, GatewayError, ModelGateway, Operation, OutputFailure, ProviderEvidence,
};
use rig_core::image_generation::ImageGenerationRequest as RigImage;
use rig_core::providers::openai::OpenAIConfig;
use rig_core::wire::{Body, Mode, Wire};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use tracing::instrument::WithSubscriber;
pub use types::*;

impl ModelGateway {
    /// Capability of this configured wire, not account access or a price grant.
    #[must_use]
    pub fn supports_images(&self) -> bool {
        self.config.model.starts_with("gpt-image-")
            && url::Url::parse(&self.config.base_url)
                .is_ok_and(|url| url.path().trim_end_matches('/') == "/v1")
    }
    /// # Errors
    /// Refuses unsupported image wires or invalid/budget-exceeding inputs and encoding failures, without admitting or sending.
    pub fn preflight_image(&self, request: &ImageRequest) -> Result<DispatchRequest, GatewayError> {
        self.prepare_image(request).map(|(_, admission)| admission)
    }
    fn prepare_image(
        &self,
        request: &ImageRequest,
    ) -> Result<(RigImage, DispatchRequest), GatewayError> {
        if !self.supports_images() {
            return Err(GatewayError::UnsupportedOperation);
        }
        let limits = self.config.limits;
        if !validation::valid_context(&request.context)
            || request.prompt.trim().is_empty()
            || request.prompt.contains('\0')
            || request.prompt.chars().count() > 32_000
            || request.max_image_bytes == 0
            || request.max_image_bytes > MAX_IMAGE_BYTES
            || image_response_budget(request.max_image_bytes)
                .is_none_or(|n| n > limits.max_response_bytes as u64)
        {
            return Err(GatewayError::InvalidRequest);
        }
        validation::bounded_json(request, limits.max_request_bytes)?;
        let (width, height) = request.size.dimensions();
        let make = || RigImage {
            prompt: request.prompt.clone(),
            width,
            height,
            additional_params: Some(json!({"n":1,"output_format":"png", "stream":false,
                "quality":request.quality,"background":request.background,
                    "safety_identifier":hex::encode(Sha256::digest(format!("zoen-image-user-v1\0{}",request.context.owner)))})),
        };
        // No content telemetry during wire construction/encoding either.
        let bytes = tracing::subscriber::with_default(
            tracing::subscriber::NoSubscriber::default(),
            || {
                let wire = rig_core::providers::openai::wire::Images::new(
                    OpenAIConfig::new(self.config.api_key.clone())
                        .with_base_url(&self.config.base_url),
                    &self.config.model,
                );
                let prepared =
                    <rig_core::operation::ImageGeneration as rig_core::wire::Operation>::prepare(
                        make(),
                        &wire.describe(),
                    )
                    .map_err(|_| GatewayError::InvalidRequest)?;
                let encoded = wire
                    .encode(prepared, Mode::Unary)
                    .map_err(|_| GatewayError::InvalidRequest)?;
                let Body::Bytes(bytes) = encoded.request.body() else {
                    return Err(GatewayError::UnsupportedOperation);
                };
                if bytes.len() > limits.max_request_bytes {
                    return Err(GatewayError::InvalidRequest);
                }
                Ok::<_, GatewayError>(bytes.clone())
            },
        )?;
        let image = ImageDispatch {
            count: 1,
            size: request.size,
            quality: request.quality,
            background: request.background,
            format: "png".into(),
            max_image_bytes: request.max_image_bytes,
        };
        let profile = json!({"profile":IMAGE_PROFILE,"base_url":self.config.base_url,
            "model":self.config.model,"credential_ref":self.config.credential_ref,
            "wire_digest":validation::wire_digest(&bytes, limits.max_request_bytes)?,
            "image":image,"request_bytes":limits.max_request_bytes,"response_bytes":limits.max_response_bytes,
            "timeout_ms":limits.timeout.as_millis(),"connect_timeout_ms":limits.connect_timeout.as_millis()});
        let admission = DispatchRequest {
            context: request.context.clone(),
            request_digest: validation::digest(request, &profile, limits.max_request_bytes)?,
            profile: IMAGE_PROFILE,
            descriptor: DispatchDescriptor {
                operation: Operation::ImageGeneration,
                endpoint: format!("{}/images/generations", self.config.base_url),
                model: self.config.model.clone(),
                credential_ref: self.config.credential_ref.clone(),
                encoded_request_bytes: bytes.len() as u64,
                max_request_bytes: limits.max_request_bytes as u64,
                max_response_bytes: limits.max_response_bytes as u64,
                requested_output_tokens: 0,
                image: Some(image),
            },
        };
        Ok((make(), admission))
    }
    /// One admitted attempt. Cancellation/errors never release or reconstruct a
    /// financial hold; only the existing authority can reconcile that evidence.
    /// # Errors
    /// Refuses invalid/unsupported requests or non-fresh/failed admission. Provider output failures remain typed in the returned attempt.
    pub async fn generate_image(
        &self,
        request: ImageRequest,
        authority: &dyn DispatchAuthority,
    ) -> Result<ImageAttemptResult, GatewayError> {
        let (prepared, admission) = self.prepare_image(&request)?;
        if authority.admit(&admission).await? != Admission::Fresh {
            return Err(GatewayError::NotFresh);
        }
        let capture = Arc::new(Mutex::new(transport::Capture::default()));
        let transport = transport::BoundedHttp {
            client: self.client.clone(),
            endpoint: admission.descriptor.endpoint.clone(),
            max_request_bytes: self.config.limits.max_request_bytes,
            max_response_bytes: self.config.limits.max_response_bytes,
            capture: capture.clone(),
        };
        let model =
            tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
                OpenAIConfig::new(self.config.api_key.clone())
                    .with_base_url(&self.config.base_url)
                    .connect(transport)
                    .image_generation(&self.config.model)
            });
        let response = model
            .call(prepared)
            .with_subscriber(tracing::Dispatch::new(
                tracing::subscriber::NoSubscriber::default(),
            ))
            .await;
        let captured = match transport::snapshot(&capture) {
            Ok(captured) => captured,
            // This remains a typed Transport output failure with incomplete
            // evidence/unknown usage, not another dispatch or a zero charge.
            Err(incomplete) => incomplete.into_incomplete(),
        };
        let (mut receipt, raw) = receipt::receipt(&captured);
        let (usage, image_usage) = decoding::usage(raw.as_ref().and_then(|raw| raw.get("usage")));
        receipt.usage = usage;
        let output = match (response, raw.as_ref()) {
            (Ok(response), Some(raw)) => decoding::image(response.image, raw, &request),
            _ => Err(captured.failure.unwrap_or(OutputFailure::MalformedResponse)),
        };
        Ok(ImageAttemptResult {
            context: request.context,
            request_digest: admission.request_digest,
            output,
            receipt,
            image_usage,
            evidence: ProviderEvidence {
                body: captured.body,
                complete: captured.complete,
            },
        })
    }
}

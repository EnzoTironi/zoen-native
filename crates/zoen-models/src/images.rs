//! OpenAI Images through the same bounded Rig transport and admission authority.
mod decoding;
mod types;
use crate::*;
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
    pub fn supports_images(&self) -> bool {
        self.config.model.starts_with("gpt-image-")
            && url::Url::parse(&self.config.base_url)
                .is_ok_and(|url| url.path().trim_end_matches('/') == "/v1")
    }
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
                let wire = OpenAIConfig::new(self.config.api_key.clone())
                    .with_base_url(&self.config.base_url)
                    .image_generation(&self.config.model);
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
        let captured = capture.lock().expect("capture lock");
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
                body: captured.body.clone(),
                complete: captured.complete,
            },
        })
    }
}

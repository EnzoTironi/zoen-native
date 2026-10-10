//! The Zoen-owned boundary for one admitted provider attempt. Rig types and
//! HTTP implementations stay private. This crate does not implement the
//! durable SQL/FDB authority, MLS worker, billing or tool execution.

mod adapter;
mod contract;
mod receipt;
mod transport;
mod validation;
pub use contract::*;

use rig_core::providers::openai::OpenAIConfig;
use rig_core::wire::{Body, Mode, Wire};
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tracing::instrument::WithSubscriber;
use transport::{BoundedHttp, Capture};

pub const CHAT_PROFILE: &str = "zoen-openai-chat-v1/rig-0.44.0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointPolicy {
    Https,
    LoopbackFixture,
}

#[derive(Clone, Copy, Debug)]
pub struct ModelLimits {
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub max_output_tokens: u64,
    pub timeout: Duration,
    pub connect_timeout: Duration,
}

/// Secret-bearing configuration is neither serializable nor content-debuggable.
pub struct GatewayConfig {
    pub base_url: String,
    pub endpoint_policy: EndpointPolicy,
    pub model: String,
    pub credential_ref: String,
    pub api_key: String,
    pub limits: ModelLimits,
}

impl std::fmt::Debug for GatewayConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GatewayConfig { redacted }")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GatewayError {
    #[error("invalid model gateway configuration")]
    InvalidConfiguration,
    #[error("invalid or oversized model request")]
    InvalidRequest,
    #[error("unsupported model operation")]
    UnsupportedOperation,
    #[error("model admission was not fresh")]
    NotFresh,
    #[error(transparent)]
    Admission(#[from] AdmissionError),
}

pub struct ModelGateway {
    config: GatewayConfig,
    client: reqwest::Client,
}

impl std::fmt::Debug for ModelGateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ModelGateway { redacted }")
    }
}

impl ModelGateway {
    pub fn new(mut config: GatewayConfig) -> Result<Self, GatewayError> {
        if config.base_url.len() > 2048 {
            return Err(GatewayError::InvalidConfiguration);
        }
        let url = reqwest::Url::parse(&config.base_url)
            .map_err(|_| GatewayError::InvalidConfiguration)?;
        let endpoint_allowed = match config.endpoint_policy {
            EndpointPolicy::Https => url.scheme() == "https",
            EndpointPolicy::LoopbackFixture => {
                url.scheme() == "http"
                    && matches!(url.host(),
                Some(url::Host::Ipv4(ip)) if ip.is_loopback())
                    || url.scheme() == "http"
                        && matches!(url.host(), Some(url::Host::Ipv6(ip)) if ip.is_loopback())
            }
        };
        let limits = config.limits;
        if !endpoint_allowed
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path().trim_end_matches('/') != "/v1"
            || !validation::identifier(&config.model)
            || !validation::identifier(&config.credential_ref)
            || config.api_key.is_empty()
            || config.api_key.len() > 4096
            || config.api_key.chars().any(char::is_control)
            || limits.max_request_bytes == 0
            || limits.max_request_bytes > 1_048_576
            || limits.max_response_bytes == 0
            || limits.max_response_bytes > 16_777_216
            || limits.max_output_tokens == 0
            || limits.max_output_tokens > 1_048_576
            || limits.timeout.is_zero()
            || limits.timeout > Duration::from_secs(120)
            || limits.connect_timeout.is_zero()
            || limits.connect_timeout > limits.timeout
        {
            return Err(GatewayError::InvalidConfiguration);
        }
        config.base_url = url.as_str().trim_end_matches('/').to_owned();
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .tls_backend_rustls()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .connect_timeout(limits.connect_timeout)
            .read_timeout(limits.timeout)
            .timeout(limits.timeout)
            .build()
            .map_err(|_| GatewayError::InvalidConfiguration)?;
        Ok(Self { config, client })
    }

    pub fn supports(&self, operation: Operation) -> bool {
        operation == Operation::ChatCompletion
    }

    pub async fn complete(
        &self,
        request: ModelRequest,
        authority: &dyn DispatchAuthority,
    ) -> Result<ModelAttemptResult, GatewayError> {
        let limits = self.config.limits;
        let (prepared, provider, digest) = tracing::subscriber::with_default(
            tracing::subscriber::NoSubscriber::default(),
            || {
                let prepared = validation::prepare(
                    &request,
                    limits.max_output_tokens,
                    limits.max_request_bytes,
                )?;
                let provider = OpenAIConfig::new(self.config.api_key.clone())
                    .with_base_url(&self.config.base_url);
                let wire = provider.chat(&self.config.model);
                let checked =
                    <rig_core::operation::Completion as rig_core::wire::Operation>::prepare(
                        prepared.clone(),
                        &wire.describe(),
                    )
                    .map_err(|_| GatewayError::InvalidRequest)?;
                let encoded = wire
                    .encode(checked, Mode::Unary)
                    .map_err(|_| GatewayError::InvalidRequest)?;
                let Body::Bytes(bytes) = encoded.request.body() else {
                    return Err(GatewayError::UnsupportedOperation);
                };
                if bytes.len() > limits.max_request_bytes {
                    return Err(GatewayError::InvalidRequest);
                }
                let wire_digest = validation::wire_digest(bytes, limits.max_request_bytes)?;
                let profile = json!({
                    "profile": CHAT_PROFILE,
                    "base_url": self.config.base_url,
                    "model": self.config.model,
                    "credential_ref": self.config.credential_ref,
                    "wire_digest": wire_digest,
                    "request_bytes": limits.max_request_bytes,
                    "response_bytes": limits.max_response_bytes,
                    "output_tokens": limits.max_output_tokens,
                    "timeout_ms": limits.timeout.as_millis(),
                    "connect_timeout_ms": limits.connect_timeout.as_millis(),
                });
                let digest = validation::digest(&request, &profile, limits.max_request_bytes)?;
                Ok::<_, GatewayError>((prepared, provider, digest))
            },
        )?;
        let admission = DispatchRequest {
            context: request.context.clone(),
            request_digest: digest.clone(),
            profile: CHAT_PROFILE,
        };
        if authority.admit(&admission).await? != Admission::Fresh {
            return Err(GatewayError::NotFresh);
        }
        let capture = Arc::new(Mutex::new(Capture::default()));
        let transport = BoundedHttp {
            client: self.client.clone(),
            endpoint: format!("{}/chat/completions", self.config.base_url),
            max_request_bytes: limits.max_request_bytes,
            max_response_bytes: limits.max_response_bytes,
            capture: capture.clone(),
        };
        let model =
            tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
                provider.connect(transport).chat(&self.config.model)
            });
        // Rig 0.44 trace_json logs raw requests/replies even with content
        // telemetry disabled. Guard construction, every poll and future drop,
        // without muting the caller's authority or structural observability.
        // There is one call and no fallback/retry. Cancellation cannot release a hold.
        let response = model
            .call(prepared)
            .with_subscriber(tracing::Dispatch::new(
                tracing::subscriber::NoSubscriber::default(),
            ))
            .await;
        let captured = capture.lock().expect("capture lock");
        let (receipt, raw) = receipt::receipt(&captured);
        let output = match (response, raw.as_ref()) {
            (Ok(response), Some(raw)) => adapter::output(response, raw, &request),
            _ => Err(captured.failure.unwrap_or(OutputFailure::MalformedResponse)),
        };
        Ok(ModelAttemptResult {
            context: request.context,
            request_digest: digest,
            output,
            receipt,
            evidence: ProviderEvidence {
                body: captured.body.clone(),
                complete: captured.complete,
            },
        })
    }
}

#[cfg(test)]
mod tests;

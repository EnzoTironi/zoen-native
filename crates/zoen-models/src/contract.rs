use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    ChatCompletion,
    StreamingCompletion,
    StructuredOutput,
    Embedding,
    Reranking,
    Transcription,
    AudioGeneration,
    ImageGeneration,
}

/// Authenticated scope/version binding supplied by the runtime, not by model output.
/// The admission implementation must independently verify every field.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptContext {
    pub attempt_id: String,
    pub run_id: String,
    pub owner: String,
    pub agent: String,
    pub device: String,
    pub space: String,
    pub authority_version: String,
    pub definition_version: String,
    pub price_version: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum InputMessage {
    System {
        text: String,
    },
    User {
        text: String,
    },
    Assistant {
        blocks: Vec<OutputBlock>,
    },
    ToolResult {
        call_id: String,
        name: String,
        text: String,
    },
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutputBlock {
    Text {
        text: String,
    },
    /// A proposal only. The gateway never executes it or creates a grant.
    ToolCall {
        call_id: String,
        name: String,
        arguments: Value,
    },
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRequest {
    pub context: AttemptContext,
    pub operation: Operation,
    pub messages: Vec<InputMessage>,
    pub tools: Vec<ToolDefinition>,
    pub max_output_tokens: u64,
}

/// No content or credential is passed to the authority. The digest binds the
/// complete canonical request and gateway configuration, including limits,
/// profile, credential reference and pinned price version.
#[derive(Clone, PartialEq, Eq)]
pub struct DispatchRequest {
    pub context: AttemptContext,
    pub request_digest: String,
    pub profile: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// Returned only for a known, fresh committed admission. Never on a read,
    /// replay or commit-unknown response. This value is deliberately not serializable.
    Fresh,
    AlreadyKnown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AdmissionError {
    #[error("model dispatch denied")]
    Denied,
    #[error("model admission unavailable or uncertain")]
    Unavailable,
}

/// Trusted runtime boundary. Implementations must reserve and claim in
/// Postgres, then perform current fenced FDB admission. No default or permissive
/// implementation exists. A fake implementation is not durable billing proof.
#[async_trait]
pub trait DispatchAuthority: Send + Sync {
    async fn admit(&self, request: &DispatchRequest) -> Result<Admission, AdmissionError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFailure {
    Transport,
    Timeout,
    ResponseTooLarge,
    MalformedResponse,
    ProviderRejected,
    UnsupportedOutput,
    InvalidToolCall,
    IncompleteOutput,
}

/// Directly reported counters only. Missing is distinct from reported zero.
/// Rig's inferred counters and catalog cost are never copied into this type.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportedUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "counters", rename_all = "snake_case")]
pub enum UsageEvidence {
    Missing,
    Reported(ReportedUsage),
    Invalid,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReceipt {
    pub http_status: Option<u16>,
    pub request_id: Option<String>,
    pub response_id: Option<String>,
    pub reported_model: Option<String>,
    pub usage: UsageEvidence,
}

/// Bounded sensitive bytes for the runtime to seal. Incomplete bodies cannot
/// prove a final bill. This is evidence, never output/publication authority.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderEvidence {
    pub body: Vec<u8>,
    pub complete: bool,
}

/// Returned when an admitted attempt completes, including malformed/error/lost replies.
/// Cancellation may return no result; admission must already be durable. The
/// caller must durably retain receipt/evidence independently of output.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelAttemptResult {
    pub context: AttemptContext,
    pub request_digest: String,
    pub output: Result<Vec<OutputBlock>, OutputFailure>,
    pub receipt: ProviderReceipt,
    pub evidence: ProviderEvidence,
}

macro_rules! private_debug {
    ($($ty:ty),* $(,)?) => {$ (
        impl std::fmt::Debug for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(concat!(stringify!($ty), " { redacted }"))
            }
        }
    )*};
}

private_debug!(
    AttemptContext,
    InputMessage,
    OutputBlock,
    ToolDefinition,
    ModelRequest,
    DispatchRequest,
    ReportedUsage,
    UsageEvidence,
    ProviderReceipt,
    ProviderEvidence,
    ModelAttemptResult
);

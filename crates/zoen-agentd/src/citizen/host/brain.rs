//! Only a trusted adapter can attest that its invocation never dispatched.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BrainError {
    #[error("{0}")]
    NotDispatched(String),
    #[error("{0}")]
    OutcomeUnknown(String),
}

// Historical/opaque errors never establish that inference did not happen.
impl From<String> for BrainError {
    fn from(message: String) -> Self {
        Self::OutcomeUnknown(message)
    }
}
impl From<&str> for BrainError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

#[async_trait]
pub trait Brain: Send + Sync {
    async fn think(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
    ) -> Result<BrainOut, BrainError>;

    async fn think_with_tools(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
        tools: &[ToolDef],
        steps: &[ToolStep],
    ) -> Result<BrainOut, BrainError> {
        let _ = (tools, steps);
        self.think(cx, history, inbound).await
    }
}

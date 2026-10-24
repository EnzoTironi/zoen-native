//! Existing public DTO declarations, grouped by domain.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum RequestStatus {
    Cancelled,
    Pending,
    Approved,
    Denied,
    /// O conteúdo mudou depois do pedido: a aprovação não vale mais.
    Stale,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ToolApprovalDto {
    pub content_hash: String,
    pub artifact_id: String,
    pub artifact_title: String,
    pub version: u32,
    pub operation: String,
    pub expires_ms: i64,
    /// Native read-only preview kind; never an execution permit.
    #[uniffi(default = None)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub preview_kind: Option<String>,
    /// Presentation of the signed review lineage, never a permit or effect receipt.
    #[uniffi(default = None)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub review: Option<ToolReviewDto>,
    #[uniffi(default = None)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub canonical_tool: Option<String>,
    /// Exact signed generation facts, never a provider capability or a cost estimate.
    #[uniffi(default = None)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub image: Option<ImageToolReview>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ImageToolReview {
    pub prompt: String,
    pub size: String,
    pub quality: String,
    pub background: String,
    pub definition_version: String,
    pub price_digest: String,
    pub max_image_bytes: u64,
    pub source_id: String,
    pub source_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum ToolReviewState {
    Unavailable,
    Available,
    Requested,
    Superseded,
    Reconcile,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ToolReviewDto {
    pub state: ToolReviewState,
    pub root_request_id: String,
    pub predecessor_request_id: Option<String>,
    pub successor_request_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum ToolRequestDecision {
    ApproveOnce,
    Deny,
    Cancel,
    /// Preparation only. A successor needs a separate normal ApproveOnce.
    RequestReview,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AgentRequestDto {
    pub tool_approval: Option<ToolApprovalDto>,
    pub id: String,
    pub agent: Persona,
    pub space_id: String,
    pub space_title: String,
    pub title: String,
    pub detail: String,
    pub audience: String,
    pub action_label: String,
    pub cost_cents: Option<i64>,
    pub reason: String,
    pub status: RequestStatus,
    pub opened_ms: i64,
    pub resolved_ms: Option<i64>,
    pub item_id: Option<String>,
    pub line_id: Option<String>,
    /// What a standing decision on this request would cover (`grants::standing_key`).
    pub action_key: String,
    /// False for red lines and irreversible actions: those always ask, so the stack
    /// doesn't offer "always approve".
    pub can_always_approve: bool,
    /// Resolved by a standing decision ("always approve/deny") rather than one by one.
    pub by_standing: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AgentSpaceTrust {
    pub space_id: String,
    pub space_title: String,
    pub level: TrustLevelDto,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AgentProfile {
    pub persona: Persona,
    /// `None` quando o orçamento é de outra pessoa (quem é dono paga).
    pub budget_limit_cents: Option<i64>,
    pub budget_spent_cents: Option<i64>,
    pub near_limit: bool,
    pub spaces: Vec<AgentSpaceTrust>,
    pub pending_requests: u32,
    pub approvals_streak: u32,
    pub runs_on: String,
    /// Your own agent's notes asking for you (`ask_owner`, `agent_proposal`) in your
    /// chat with it since you last wrote there. Always 0 for other people's agents.
    pub owner_notes_pending: u32,
}

/// An agent's status, as its owner sees it on the relay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum AgentStatus {
    Active,
    Paused,
    /// Final: the agent no longer answers anyone.
    Revoked,
}

/// What an owner can do to one of their agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum AgentControl {
    Pause,
    Resume,
    /// Final, like `AgentStatus::Revoked`.
    Revoke,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum DecisionKind {
    Act,
    ActWithUndo,
    Request,
    Block,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct DecisionPreview {
    pub action: String,
    pub example: String,
    pub kind: DecisionKind,
    pub explanation: String,
    pub red_line: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct PlanOutcome {
    pub kind: DecisionKind,
    pub item: Option<ItemDetail>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ApproveOutcome {
    pub request: AgentRequestDto,
    pub message: String,
}

/// The four swipes on the approvals stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum RequestDecision {
    /// Right: approve this one.
    Approve,
    /// Left: deny this one.
    Deny,
    /// Up: approve and keep approving this kind of action from this agent here.
    AlwaysApprove,
    /// Down: deny and keep denying this kind of action from this agent here.
    AlwaysDeny,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct DecideOutcome {
    pub request: AgentRequestDto,
    pub message: String,
    /// The standing Grant a "Sempre" swipe issued (revocable from Permissions).
    pub standing_grant_id: Option<String>,
    /// Other pending requests the standing decision resolved right away.
    pub also_resolved: u32,
}

/// A standing "always approve / always deny" an owner gave one of their agents.
#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct StandingDecisionDto {
    pub grant_id: String,
    pub agent: Persona,
    pub space_id: String,
    pub space_title: String,
    pub action_key: String,
    pub action_label: String,
    pub allow: bool,
    pub at_ms: i64,
}

#[cfg(test)]
mod tests;

//! Shared typed owner-control responses; unavailable is not a refusal.
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum PolicyProgress {
    /// Only a precise missing-group rejection from the configured native core.
    /// Not a quote/refusal, and never evidence that a Welcome has arrived.
    NeedsWelcome {
        inquiry_digest: String,
        device: String,
    },
    /// A diagnostic outcome only: never a refusal, receipt or retry permission.
    Unavailable {
        operation: PolicyOperation,
        code: PolicyUnavailable,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_stage: Option<zoen_runtime::OwnerPolicyContextStage>,
    },
    Installed {
        policy_digest: String,
    },
    Refused {
        policy_digest: String,
    },
    Options {
        options: Box<types::owner_policy_context::OwnerPolicyOptions>,
    },
    OptionsRefused {
        inquiry_digest: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PolicyOperation {
    Context,
    Import,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PolicyUnavailable {
    InvalidPolicy,
    InvalidProfile,
    DeploymentMismatch,
    InvalidBinding,
    Denied,
    OverBudget,
    CoreAuthorityUnavailable,
    RestoreUnreconciled,
    Unavailable,
    EvidenceTooLarge,
}

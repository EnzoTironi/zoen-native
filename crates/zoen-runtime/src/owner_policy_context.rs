//! Operational classification only; never an authenticated policy refusal.
use crate::RuntimeError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerPolicyContextStage {
    Credential,
    Inquiry,
    Source,
    Capsule,
    Clock,
    Admission,
    Directory,
    Finance,
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("owner policy context {stage:?}: {error}")]
pub struct OwnerPolicyContextFailure {
    pub stage: OwnerPolicyContextStage,
    pub error: RuntimeError,
}

impl OwnerPolicyContextStage {
    pub(crate) fn failed(self, error: RuntimeError) -> OwnerPolicyContextFailure {
        OwnerPolicyContextFailure { stage: self, error }
    }
}

impl From<OwnerPolicyContextFailure> for RuntimeError {
    fn from(failure: OwnerPolicyContextFailure) -> Self {
        failure.error
    }
}

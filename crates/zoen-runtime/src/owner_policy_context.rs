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
    AdmissionUnusable,
    AdmissionInquiry,
    AdmissionPrincipal,
    AdmissionSignature,
    AdmissionFreshness,
    AdmissionProfiles,
    AdmissionConversation,
    AdmissionIntegrity,
    AdmissionMembership,
    AdmissionWriter,
    AdmissionImageReference,
    AdmissionMlsCredential,
    AdmissionMlsGroup,
    AdmissionMlsPending,
    AdmissionMlsReconciliation,
    AdmissionMlsStorage,
    AdmissionMlsWelcome,
    AdmissionMlsMembership,
    AdmissionMlsBehind,
    AdmissionMlsAdding,
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

impl From<client_core::owner_policy::OwnerPolicyContextRejection> for OwnerPolicyContextStage {
    fn from(reason: client_core::owner_policy::OwnerPolicyContextRejection) -> Self {
        use client_core::owner_policy::OwnerPolicyContextRejection as E;
        match reason {
            E::Unusable => Self::AdmissionUnusable,
            E::Inquiry => Self::AdmissionInquiry,
            E::Principal => Self::AdmissionPrincipal,
            E::Signature => Self::AdmissionSignature,
            E::Freshness => Self::AdmissionFreshness,
            E::Profiles => Self::AdmissionProfiles,
            E::Conversation => Self::AdmissionConversation,
            E::Integrity => Self::AdmissionIntegrity,
            E::Membership => Self::AdmissionMembership,
            E::Writer => Self::AdmissionWriter,
            E::ImageReference => Self::AdmissionImageReference,
            E::MlsCredential => Self::AdmissionMlsCredential,
            E::MlsGroup => Self::AdmissionMlsGroup,
            E::MlsPending => Self::AdmissionMlsPending,
            E::MlsReconciliation => Self::AdmissionMlsReconciliation,
            E::MlsStorage => Self::AdmissionMlsStorage,
            E::MlsWelcome => Self::AdmissionMlsWelcome,
            E::MlsMembership => Self::AdmissionMlsMembership,
            E::MlsBehind => Self::AdmissionMlsBehind,
            E::MlsAdding => Self::AdmissionMlsAdding,
        }
    }
}

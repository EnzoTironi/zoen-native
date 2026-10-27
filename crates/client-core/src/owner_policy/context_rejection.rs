//! Closed operational facts from the actual admission gates, never authority.
use super::{denied, CoreError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerPolicyContextRejection {
    Unusable,
    Inquiry,
    Principal,
    Signature,
    Freshness,
    Profiles,
    Conversation,
    Integrity,
    Membership,
    Writer,
    ImageReference,
    MlsCredential,
    MlsGroup,
    MlsPending,
    MlsReconciliation,
    MlsStorage,
    MlsWelcome,
    MlsMembership,
    MlsBehind,
    MlsAdding,
}

pub(crate) struct ContextAdmissionFailure {
    pub(crate) reason: OwnerPolicyContextRejection,
    pub(crate) cause: CoreError,
}
impl OwnerPolicyContextRejection {
    pub(super) fn denied(self) -> ContextAdmissionFailure {
        self.failed(denied())
    }
    pub(super) fn failed(self, cause: CoreError) -> ContextAdmissionFailure {
        ContextAdmissionFailure {
            reason: self,
            cause,
        }
    }
}
impl From<crate::mls::ReadinessRejection> for OwnerPolicyContextRejection {
    fn from(error: crate::mls::ReadinessRejection) -> Self {
        use crate::mls::ReadinessRejection as E;
        match error {
            E::Credential => Self::MlsCredential,
            E::Group => Self::MlsGroup,
            E::Pending => Self::MlsPending,
            E::Reconciliation => Self::MlsReconciliation,
            E::Storage => Self::MlsStorage,
            E::Welcome => Self::MlsWelcome,
            E::Membership => Self::MlsMembership,
            E::Behind => Self::MlsBehind,
            E::Adding => Self::MlsAdding,
        }
    }
}

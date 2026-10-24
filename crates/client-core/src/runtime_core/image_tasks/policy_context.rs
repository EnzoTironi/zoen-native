//! Read-only confirmation against this retained native capsule, not approval.
use super::*;
use types::owner_policy_context::SignedOwnerPolicyInquiry;
impl DeviceCore {
    pub fn owner_policy_account_context(
        &self,
        signed: &SignedOwnerPolicyInquiry,
        at_ms: i64,
    ) -> Result<()> {
        self.owner_policy_account_admission(signed, at_ms)
            .map_err(|_| NativeError::Permission)
    }

    /// Same read-only admission as the public context check, with a closed cause.
    /// A rejection is operational evidence, never an authenticated refusal.
    pub fn owner_policy_account_admission(
        &self,
        signed: &SignedOwnerPolicyInquiry,
        at_ms: i64,
    ) -> std::result::Result<(), crate::owner_policy::OwnerPolicyContextRejection> {
        use crate::owner_policy::OwnerPolicyContextRejection as E;
        if !self.usable {
            return Err(E::Unusable);
        }
        if signed.inquiry.purpose.is_none() {
            return Err(E::Inquiry);
        }
        if signed.inquiry.owner != self.owner || signed.inquiry.agent != self.agent {
            return Err(E::Principal);
        }
        if !event_log::owner_policy_inquiry::verify(signed) {
            return Err(E::Signature);
        }
        self.engine
            .owner_policy_context_admission(&signed.inquiry, at_ms)
            .map(|_| ())
            .map_err(|failure| failure.reason)
    }

    pub fn owner_policy_image_context(
        &self,
        signed: &SignedOwnerPolicyInquiry,
        at_ms: i64,
    ) -> Result<ImageTaskInput> {
        if !self.usable
            || signed.inquiry.owner != self.owner
            || signed.inquiry.agent != self.agent
            || !event_log::owner_policy_inquiry::verify(signed)
        {
            return Err(NativeError::Permission);
        }
        self.engine
            .owner_policy_image_input(&signed.inquiry, at_ms)
            .map_err(|_| NativeError::Permission)
    }
}

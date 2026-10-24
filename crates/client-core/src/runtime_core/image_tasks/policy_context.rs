//! Read-only confirmation against this retained native capsule, not approval.
use super::*;
use types::owner_policy_context::SignedOwnerPolicyInquiry;
impl DeviceCore {
    pub fn owner_policy_account_context(
        &self,
        signed: &SignedOwnerPolicyInquiry,
        at_ms: i64,
    ) -> Result<()> {
        if !self.usable
            || signed.inquiry.purpose.is_none()
            || signed.inquiry.owner != self.owner
            || signed.inquiry.agent != self.agent
            || !event_log::owner_policy_inquiry::verify(signed)
        {
            return Err(NativeError::Permission);
        }
        self.engine
            .owner_policy_context_input(&signed.inquiry, at_ms)
            .map(|_| ())
            .map_err(|_| NativeError::Permission)
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

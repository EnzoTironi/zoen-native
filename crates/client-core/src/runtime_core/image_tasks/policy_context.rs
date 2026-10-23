//! Read-only confirmation against this retained native capsule, not approval.
use super::*;
use types::owner_policy_context::SignedOwnerPolicyInquiry;
impl DeviceCore {
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

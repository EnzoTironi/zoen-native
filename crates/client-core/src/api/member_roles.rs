//! Public CLI/native entry points over one reviewed membership implementation.
use crate::{Client, CoreError, SpaceMemberContext};

#[uniffi::export]
impl Client {
    pub fn space_member_context(
        &self,
        space_id: String,
        identity_id: String,
        expected_identity_id: String,
        expected_device_id: String,
    ) -> Result<SpaceMemberContext, CoreError> {
        self.lock().space_member_context(
            &space_id,
            &identity_id,
            &expected_identity_id,
            &expected_device_id,
        )
    }

    pub fn set_member_role_reviewed(
        &self,
        space_id: String,
        identity_id: String,
        expected_identity_id: String,
        expected_device_id: String,
        observed_revision: String,
        role: String,
    ) -> Result<SpaceMemberContext, CoreError> {
        self.lock().set_member_role_reviewed(
            &space_id,
            &identity_id,
            &expected_identity_id,
            &expected_device_id,
            &observed_revision,
            &role,
        )
    }
}

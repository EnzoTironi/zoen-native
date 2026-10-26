//! One public membership path; native UI and CLI use the same reviewed Core.
use super::{encode, invalid, Client, Command, CoreError};

impl Client {
    pub(super) async fn dispatch_member_command(
        &self,
        command: Command,
    ) -> Result<String, CoreError> {
        use Command::{
            AddAdmin, AddMember, AddMemberReviewed, FindMemberCandidates, RemoveMember,
            RemoveMemberReviewed, SetMemberRoleReviewed, SpaceMemberContext,
        };
        match command {
            AddMember {
                space_id,
                identity_id,
            } => encode(&self.add_member(space_id, identity_id)?),
            FindMemberCandidates {
                space_id,
                query,
                expected_identity_id,
                expected_device_id,
                owned_agents_only,
            } => encode(
                &self
                    .find_member_candidates(
                        &space_id,
                        &query,
                        &expected_identity_id,
                        &expected_device_id,
                        owned_agents_only.unwrap_or(false),
                    )
                    .await?,
            ),
            AddMemberReviewed {
                space_id,
                identity_id,
                expected_identity_id,
                expected_device_id,
                observed_revision,
            } => encode(&self.lock().add_member_reviewed(
                &space_id,
                &identity_id,
                &expected_identity_id,
                &expected_device_id,
                &observed_revision,
            )?),
            AddAdmin {
                space_id,
                identity_id,
            } => encode(&self.add_admin(space_id, identity_id)?),
            SpaceMemberContext {
                space_id,
                identity_id,
                expected_identity_id,
                expected_device_id,
            } => encode(&self.space_member_context(
                space_id,
                identity_id,
                expected_identity_id,
                expected_device_id,
            )?),
            SetMemberRoleReviewed {
                space_id,
                identity_id,
                expected_identity_id,
                expected_device_id,
                observed_revision,
                role,
            } => encode(&self.set_member_role_reviewed(
                space_id,
                identity_id,
                expected_identity_id,
                expected_device_id,
                observed_revision,
                role,
            )?),
            RemoveMemberReviewed {
                space_id,
                identity_id,
                expected_identity_id,
                expected_device_id,
                observed_revision,
            } => encode(&self.lock().remove_member_reviewed(
                &space_id,
                &identity_id,
                &expected_identity_id,
                &expected_device_id,
                &observed_revision,
            )?),
            RemoveMember {
                space_id,
                identity_id,
            } => encode(&self.remove_member(space_id, identity_id)?),
            _ => Err(invalid("Not a membership command")),
        }
    }
}

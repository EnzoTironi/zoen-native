//! Space invitations reuse native membership review and the relay's atomic limits.
use super::{encode, Client, CoreError};
use serde::Deserialize;
use types::Role;

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub(crate) struct SpaceInviteInput {
    pub space_id: String,
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub observed_revision: String,
    pub max_uses: u32,
    pub ttl_secs: u32,
}
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum SpaceInviteCommand {
    CreateInvite { space_id: String },
    CreateInviteReviewed { input: SpaceInviteInput },
}

pub(crate) fn check(client: &Client, input: &SpaceInviteInput) -> Result<(), CoreError> {
    if !(1..=10_000).contains(&input.max_uses) || !(60..=2_592_000).contains(&input.ttl_secs) {
        return Err(CoreError::Invalid {
            reason: "invalid invite limits".into(),
        });
    }
    let context = client.space_member_context(
        input.space_id.clone(),
        input.expected_identity_id.clone(),
        input.expected_identity_id.clone(),
        input.expected_device_id.clone(),
    )?;
    if context.revision != input.observed_revision
        || !context.can_manage_members
        || !context.delivery_ready
        || context.state != crate::MemberActionState::Active
    {
        return Err(CoreError::Forbidden {
            reason: "invite review changed".into(),
        });
    }
    Ok(())
}
impl SpaceInviteCommand {
    pub(super) async fn execute(self, client: &Client) -> Result<String, CoreError> {
        match self {
            Self::CreateInvite { space_id } => encode(&client.create_invite(space_id).await?),
            Self::CreateInviteReviewed { input } => {
                check(client, &input)?;
                let protocol::Reply::Invite(invite) = client
                    .request(protocol::Op::CreateInvite {
                        space: input.space_id.clone(),
                        agent: None,
                        role: Role::Member,
                        max_uses: input.max_uses,
                        ttl_secs: u64::from(input.ttl_secs),
                    })
                    .await?
                else {
                    return Err(CoreError::Invalid {
                        reason: "unexpected invite response".into(),
                    });
                };
                check(client, &input)?;
                if !protocol::valid_invite_code(&invite.code) || invite.expires_at_ms <= 0 {
                    return Err(CoreError::Invalid {
                        reason: "invalid invite response".into(),
                    });
                }
                encode(&crate::api::InviteDto {
                    link: protocol::links::space(&invite.code),
                    code: invite.code,
                    expires_at_ms: invite.expires_at_ms,
                })
            }
        }
    }
}

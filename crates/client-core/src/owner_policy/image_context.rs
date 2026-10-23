//! Read-only image-review facts; never Act, ApproveOnce or paid dispatch authority.
use super::*;
use crate::image_tasks::ImageTaskInput;
use types::owner_policy_context::OwnerPolicyInquiry;

impl Engine {
    pub(crate) fn owner_policy_inquiry_scope(
        &self,
        inquiry: &OwnerPolicyInquiry,
        at_ms: i64,
    ) -> R<()> {
        let agent = self.identities.get(&inquiry.agent).ok_or_else(denied)?;
        let owner = self.identities.get(&inquiry.owner).ok_or_else(denied)?;
        let space = self.space_state(&inquiry.space)?;
        if !inquiry.fresh_at(at_ms)
            || owner.kind != IdentityKind::Person
            || agent.kind != IdentityKind::Agent
            || agent.owner.as_deref() != Some(inquiry.owner.as_str())
            || !event_log::agent_owner::profile_authorized(agent)
            || space.kind != SpaceKind::Direct
            || space.privacy != Privacy::EndToEnd
            || space.members.len() != 2
            || space.integrity_error.is_some()
            || !self.is_member(&inquiry.space, &inquiry.owner)
            || !self.is_member(&inquiry.space, &inquiry.agent)
            || self.ready_epoch(&inquiry.space).is_none()
        {
            return Err(denied());
        }
        Ok(())
    }

    pub(crate) fn owner_policy_context_input(
        &self,
        inquiry: &OwnerPolicyInquiry,
        at_ms: i64,
    ) -> R<Option<ImageTaskInput>> {
        if inquiry.purpose.is_some() {
            self.owner_policy_inquiry_scope(inquiry, at_ms)?;
            if !self.state.writer(&inquiry.space, &inquiry.owner)
                || !self.state.writer(&inquiry.space, &inquiry.agent)
            {
                return Err(denied());
            }
            return Ok(None);
        }
        self.owner_policy_image_input(inquiry, at_ms).map(Some)
    }

    pub(crate) fn owner_policy_image_input(
        &self,
        inquiry: &OwnerPolicyInquiry,
        at_ms: i64,
    ) -> R<ImageTaskInput> {
        if inquiry.purpose.is_some() {
            return Err(denied());
        }
        self.owner_policy_inquiry_scope(inquiry, at_ms)?;
        let record = self
            .state
            .requests
            .get(&inquiry.request)
            .ok_or_else(denied)?;
        let proposal = record.req.tool_proposal.as_ref().ok_or_else(denied)?;
        if record.status != crate::engine::ReqStatus::Pending
            || record.space != inquiry.space
            || record.req.agent != inquiry.agent
            || record.req.content_hash != inquiry.content_hash
            || proposal.owner != inquiry.owner
            || proposal.expires_at_ms <= at_ms
            || !crate::image_tasks::matches(&self.state, &record.req, &inquiry.space)
        {
            return Err(denied());
        }
        let input: ImageTaskInput =
            serde_json::from_str(&proposal.arguments_json).map_err(|_| invalid())?;
        input.binding()?;
        if input.definition.price_digest != inquiry.price_digest {
            return Err(denied());
        }
        let event = self
            .logs
            .get(&inquiry.space)
            .and_then(|log| {
                log.events()
                    .iter()
                    .find(|event| event_log::content_hash_of(event) == record.opened_content_hash)
            })
            .ok_or_else(denied)?;
        if event.seq == 0
            || event.author != inquiry.agent
            || event.device.as_deref() != Some(proposal.device.as_str())
            || event.sealed_wire.is_none()
            || event_log::verify_author(event).is_err()
            || !matches!(&event.body, EventBody::RequestOpened { request } if request == &record.req)
        {
            return Err(denied());
        }
        Ok(input)
    }
}

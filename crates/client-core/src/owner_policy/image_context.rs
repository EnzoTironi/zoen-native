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
        self.owner_policy_scope_checked(inquiry, at_ms)
            .map_err(|failure| failure.cause)
    }

    fn owner_policy_scope_checked(
        &self,
        inquiry: &OwnerPolicyInquiry,
        at_ms: i64,
    ) -> std::result::Result<(), ContextAdmissionFailure> {
        if !inquiry.fresh_at(at_ms) {
            return Err(OwnerPolicyContextRejection::Freshness.denied());
        }
        self.owner_policy_membership_checked(inquiry)
    }

    fn owner_policy_membership_checked(
        &self,
        inquiry: &OwnerPolicyInquiry,
    ) -> std::result::Result<(), ContextAdmissionFailure> {
        use OwnerPolicyContextRejection as E;
        let agent = self
            .identities
            .get(&inquiry.agent)
            .ok_or_else(|| E::Profiles.denied())?;
        let owner = self
            .identities
            .get(&inquiry.owner)
            .ok_or_else(|| E::Profiles.denied())?;
        let space = self
            .space_state(&inquiry.space)
            .map_err(|error| E::Conversation.failed(error))?;
        if !inquiry.valid_shape() {
            return Err(E::Inquiry.denied());
        }
        if owner.kind != IdentityKind::Person
            || agent.kind != IdentityKind::Agent
            || agent.owner.as_deref() != Some(inquiry.owner.as_str())
            || !event_log::agent_owner::profile_authorized(agent)
        {
            return Err(E::Profiles.denied());
        }
        if space.kind != SpaceKind::Direct
            || space.privacy != Privacy::EndToEnd
            || space.members.len() != 2
        {
            return Err(E::Conversation.denied());
        }
        if space.integrity_error.is_some() {
            return Err(E::Integrity.denied());
        }
        if !self.is_member(&inquiry.space, &inquiry.owner)
            || !self.is_member(&inquiry.space, &inquiry.agent)
        {
            return Err(E::Membership.denied());
        }
        self.ready_epoch_checked(&inquiry.space, &self.state)
            .map_err(|reason| E::from(reason).denied())?;
        Ok(())
    }

    pub(crate) fn owner_policy_context_input(
        &self,
        inquiry: &OwnerPolicyInquiry,
        at_ms: i64,
    ) -> R<Option<ImageTaskInput>> {
        self.owner_policy_context_admission(inquiry, at_ms)
            .map_err(|failure| failure.cause)
    }

    pub(crate) fn owner_policy_context_admission(
        &self,
        inquiry: &OwnerPolicyInquiry,
        at_ms: i64,
    ) -> std::result::Result<Option<ImageTaskInput>, ContextAdmissionFailure> {
        self.owner_policy_scope_checked(inquiry, at_ms)?;
        self.owner_policy_reference_checked(inquiry, at_ms)
    }

    /// Read-only terminal classification. It never admits an expired quote,
    /// review, model turn or financial action.
    pub(crate) fn owner_policy_inquiry_expired(
        &self,
        inquiry: &OwnerPolicyInquiry,
        at_ms: i64,
    ) -> R<bool> {
        if at_ms <= 0 || !inquiry.valid_shape() {
            return Err(denied());
        }
        if at_ms.saturating_sub(inquiry.at_ms) <= types::owner_policy_context::CONTEXT_TTL_MS {
            return Ok(false);
        }
        self.owner_policy_reference_checked(inquiry, at_ms)
            .map_err(|failure| failure.cause)?;
        Ok(true)
    }

    fn owner_policy_reference_checked(
        &self,
        inquiry: &OwnerPolicyInquiry,
        at_ms: i64,
    ) -> std::result::Result<Option<ImageTaskInput>, ContextAdmissionFailure> {
        self.owner_policy_membership_checked(inquiry)?;
        if inquiry.purpose.is_some() {
            if !self.state.writer(&inquiry.space, &inquiry.owner)
                || !self.state.writer(&inquiry.space, &inquiry.agent)
            {
                return Err(OwnerPolicyContextRejection::Writer.denied());
            }
            return Ok(None);
        }
        self.owner_policy_image_reference(inquiry, at_ms)
            .map(Some)
            .map_err(|error| OwnerPolicyContextRejection::ImageReference.failed(error))
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
        self.owner_policy_image_reference(inquiry, at_ms)
    }

    fn owner_policy_image_reference(
        &self,
        inquiry: &OwnerPolicyInquiry,
        at_ms: i64,
    ) -> R<ImageTaskInput> {
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

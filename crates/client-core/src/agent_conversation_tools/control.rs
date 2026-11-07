//! Only canonical review metadata may preserve a native conversation baseline.
use crate::engine::{ReqStatus, State};
use types::{Event, EventBody, ToolDecision};

pub(crate) fn conversation_review_control(
    state: &State,
    event: &Event,
    owner: &str,
    agent: &str,
) -> bool {
    let request = match &event.body {
        EventBody::RequestOpened { request } => request,
        EventBody::RequestResolved { request, .. }
        | EventBody::ToolReviewRequested { request, .. } => {
            let Some(record) = state.requests.get(request) else {
                return false;
            };
            if record.space != event.space {
                return false;
            }
            &record.req
        }
        _ => return false,
    };
    let Some(proposal) = request.tool_proposal.as_ref() else {
        return false;
    };
    if request.agent != agent
        || proposal.owner != owner
        || proposal.schema != super::SCHEMA
        || !state.tool_request_matches(request, &event.space)
    {
        return false;
    }
    match &event.body {
        EventBody::RequestOpened { .. } => {
            event.author == agent && event.device.as_deref() == Some(proposal.device.as_str())
        }
        EventBody::ToolReviewRequested { content_hash, .. } => {
            event.author == owner && content_hash == &request.content_hash
        }
        EventBody::RequestResolved {
            approved,
            content_hash,
            resolution,
            ..
        } => {
            let Some(record) = state.requests.get(&request.id) else {
                return false;
            };
            event.author == owner
                && content_hash == &request.content_hash
                && record.resolution == *resolution
                && matches!(record.status, ReqStatus::Approved) == *approved
                && matches!(
                    (approved, resolution.as_ref().and_then(|r| r.tool_decision)),
                    (true, Some(ToolDecision::ApproveOnce))
                        | (false, Some(ToolDecision::Deny | ToolDecision::Cancel))
                )
        }
        _ => false,
    }
}

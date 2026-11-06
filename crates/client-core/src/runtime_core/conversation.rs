//! Basic conversation references over the original authenticated native history.
use super::*;
use crate::engine::EntryBody;
use types::ReplyRef;

pub(super) fn thread(event: &Event) -> Option<&str> {
    match &event.body {
        EventBody::MessagePosted {
            reply: Some(reply), ..
        } if reply.thread => Some(&reply.to),
        _ => None,
    }
}

pub(super) fn in_context(event: &Event, root: Option<&str>) -> bool {
    match root {
        Some(root) => event.client_id == root || thread(event) == Some(root),
        None => thread(event).is_none(),
    }
}

pub(super) fn supported(state: &State, event: &Event) -> bool {
    let EventBody::MessagePosted {
        attaches: None,
        reply,
        ..
    } = &event.body
    else {
        return false;
    };
    let Some(reply) = reply else { return true };
    if reply.shared.is_some()
        || reply.artifact.is_some()
        || reply.quote.is_some()
        || reply.to == event.client_id
        || reply.to.is_empty()
        || reply.to.len() > 256
        || reply.to.chars().any(char::is_control)
        || !crate::replies::thread_source::received_valid(state, event)
    {
        return false;
    }
    let Some(target) = state
        .spaces
        .get(&event.space)
        .and_then(|space| space.entries.iter().find(|entry| entry.id() == reply.to))
    else {
        return false;
    };
    matches!(&target.body, EntryBody::Message { marks, reply: parent, .. }
        if !marks.deleted && (!reply.thread || parent.as_ref().is_none_or(|parent| !parent.thread)))
}

impl DeviceCore {
    /// Historical reference only. Current spending/sending authorization is checked separately.
    pub(super) fn expected_reply(&self, intent: &ReplyIntent) -> Result<Option<ReplyRef>> {
        if intent.profile == LEGACY_PROFILE {
            return Ok(None);
        }
        if intent.profile != PROFILE {
            return Err(NativeError::History);
        }
        let source = self
            .engine
            .logs
            .get(intent.space())
            .and_then(|log| {
                log.events()
                    .iter()
                    .find(|event| event.hash == intent.trigger())
            })
            .ok_or(NativeError::History)?;
        let mut reply = match thread(source) {
            Some(root) => ReplyRef::thread(root),
            None => ReplyRef::inline(&source.client_id),
        };
        if reply.to != source.client_id {
            reply.source = Some(source.client_id.clone());
        }
        Ok(Some(reply))
    }
}

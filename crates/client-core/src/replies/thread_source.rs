//! Exact reply source and thread root stay distinct, without another history store.
use crate::engine::{Entry, EntryBody, State};
use types::{Event, EventBody, ReplyRef};

pub(crate) fn received_valid(state: &State, event: &Event) -> bool {
    match &event.body {
        EventBody::MessagePosted {
            reply: Some(reply), ..
        } => {
            valid(state, &event.space, &event.author, reply)
                && reply.source.as_deref() != Some(event.client_id.as_str())
        }
        EventBody::AppEditRequested { reply, .. } => reply.source.is_none(),
        _ => true,
    }
}

pub(crate) fn valid(state: &State, space: &str, author: &str, reply: &ReplyRef) -> bool {
    let Some(source) = &reply.source else {
        return true;
    };
    if !reply.thread
        || reply.shared.is_some()
        || reply.artifact.is_some()
        || source == &reply.to
        || source.is_empty()
        || source.len() > 256
        || source.chars().any(char::is_control)
        || reply
            .quote
            .as_ref()
            .is_some_and(|quote| quote.source_id != *source)
        || !state.writer(space, author)
    {
        return false;
    }
    // Late joiners can receive a reply without its earlier source. The locator
    // carries no excerpt or permission; the projection leaves that quote absent.
    let Some(entry) = state
        .spaces
        .get(space)
        .and_then(|state| state.entries.iter().find(|entry| entry.id() == source))
    else {
        return true;
    };
    matches_root(entry, reply)
}

pub(crate) fn matches_root(entry: &Entry, reply: &ReplyRef) -> bool {
    matches!(&entry.body, EntryBody::Message { reply: Some(parent), .. }
        if parent.thread && parent.to == reply.to)
}

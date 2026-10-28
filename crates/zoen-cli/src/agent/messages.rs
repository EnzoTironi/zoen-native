//! Existing chat replies and owner-note delivery; the same Client/authority.
use super::*;

pub(super) fn turn_note(
    selection: Option<&source_mode::Bound>,
    code: &str,
    message: impl std::fmt::Display,
) {
    if selection.is_some() {
        say(code);
    } else {
        say(message);
    }
}

pub(super) fn selected_reply(
    e: &Arc<Client>,
    space: &str,
    text: &str,
    source: &str,
    selection: Option<&source_mode::Bound>,
) -> bool {
    let Some(selection) = selection else {
        return reply(e, space, text, source);
    };
    if selection.check(e).is_err() {
        return false;
    }
    fault("before-send");
    let result = e.send_reply(space.into(), text.into(), source.into(), false);
    trace::delivery(space, source, trace::Delivery::Reply, result.is_ok());
    say(if result.is_ok() {
        "agent_host_source_reply_queued"
    } else {
        "agent_host_source_reply_failed"
    });
    result.is_ok() && selection.check(e).is_ok()
}

pub(super) fn selected_owner_note(
    e: &Arc<Client>,
    cx: &Ctx,
    body: serde_json::Value,
    space: &str,
    source: &str,
    selection: Option<&source_mode::Bound>,
) -> bool {
    let Some(selection) = selection else {
        return tell_owner(e, cx, body, space, source);
    };
    if selection.check(e).is_err() {
        return false;
    }
    let Ok(dm) = e.start_direct(cx.owner.clone()) else {
        trace::delivery(space, source, trace::Delivery::OwnerNote, false);
        return false;
    };
    if selection.check(e).is_err() {
        return false;
    }
    let result = e.send_message(dm, format!("```json\n{body}\n```"));
    trace::delivery(space, source, trace::Delivery::OwnerNote, result.is_ok());
    say(if result.is_ok() {
        "agent_host_source_owner_note_queued"
    } else {
        "agent_host_source_owner_note_failed"
    });
    result.is_ok() && selection.check(e).is_ok()
}

pub(super) fn selected_proposal(
    e: &Arc<Client>,
    cx: &Ctx,
    space: &str,
    t: &TimelineEntry,
    spec: &AgentSpec,
    confirmation: Option<&str>,
    selection: Option<&source_mode::Bound>,
) -> bool {
    if selection.is_none() {
        return propose(e, cx, space, t, spec, confirmation);
    }
    if !selected_owner_note(
        e,
        cx,
        json!({"agent_proposal": {"source":t.id,"space":space,"spec":spec,"confirmation":confirmation}}),
        space,
        &t.id,
        selection,
    ) {
        return false;
    }
    say("agent_host_source_proposal_queued");
    match confirmation {
        Some(text) => selected_reply(e, space, text, &t.id, selection),
        None => true,
    }
}

pub(super) fn text_of(t: &TimelineEntry) -> Option<&str> {
    match &t.kind {
        EntryKind::Message { text, .. } => Some(text),
        _ => None,
    }
}

pub(super) fn reply(e: &Arc<Client>, space: &str, text: &str, source: &str) -> bool {
    fault("before-send");
    let result = e.send_reply(space.into(), text.into(), source.into(), false);
    trace::delivery(space, source, trace::Delivery::Reply, result.is_ok());
    match result {
        Ok(_) => {
            say(format_args!(
                "woke in {} -> replied",
                space.chars().take(10).collect::<String>()
            ));
            true
        }
        Err(err) => {
            say(format_args!("reply failed: {err}"));
            false
        }
    }
}

/// The Creator's proposal goes to the owner's DM for approval on the owner's device; its
/// own words go to the chat it was asked in.
pub(super) fn propose(
    e: &Arc<Client>,
    cx: &Ctx,
    space: &str,
    t: &TimelineEntry,
    spec: &AgentSpec,
    confirmation: Option<&str>,
) -> bool {
    if !tell_owner(
        e,
        cx,
        json!({"agent_proposal": {"source": t.id, "space": space, "spec": spec,
                                  "confirmation": confirmation}}),
        space,
        &t.id,
    ) {
        return false;
    }
    say(format_args!("proposed @{}", spec.handle));
    match confirmation {
        Some(c) => reply(e, space, c, &t.id),
        None => true,
    }
}

/// A structured note to the owner (their app renders it), tagged with its source message.
pub(super) fn tell_owner(
    e: &Arc<Client>,
    cx: &Ctx,
    body: serde_json::Value,
    space: &str,
    source: &str,
) -> bool {
    let Ok(dm) = e.start_direct(cx.owner.clone()) else {
        trace::delivery(space, source, trace::Delivery::OwnerNote, false);
        return false;
    };
    let text = format!("```json\n{body}\n```");
    let result = e.send_message(dm, text);
    trace::delivery(space, source, trace::Delivery::OwnerNote, result.is_ok());
    match result {
        Ok(_) => {
            say("told the owner");
            true
        }
        Err(err) => {
            say(format_args!("owner note failed: {err}"));
            false
        }
    }
}

/// Recover only our exact structured note from the owner's signed timeline.
/// Unavailable history is not evidence that delivery never happened.
pub(super) fn owner_note(
    e: &Arc<Client>,
    cx: &Ctx,
    space: &str,
    source: &TimelineEntry,
) -> Result<Option<serde_json::Value>, client_core::CoreError> {
    let dm = e.start_direct(cx.owner.clone())?;
    Ok(e.timeline(dm)?.iter().find_map(|message| {
        if message.author.id != cx.me {
            return None;
        }
        owner_note_body(text_of(message)?, space, source)
    }))
}

fn owner_note_body(text: &str, space: &str, source: &TimelineEntry) -> Option<serde_json::Value> {
    let body = text.trim().strip_prefix("```json")?.strip_suffix("```")?;
    let note: serde_json::Value = serde_json::from_str(body.trim()).ok()?;
    let object = note.as_object()?;
    if object.len() != 1 {
        return None;
    }
    let (kind, payload) = object.iter().next()?;
    if payload.get("space")?.as_str()? != space || payload.get("source")?.as_str()? != source.id {
        return None;
    }
    match kind.as_str() {
        "agent_proposal" if matches!(source.kind, EntryKind::Message { .. }) => {
            serde_json::from_value::<AgentSpec>(payload.get("spec")?.clone()).ok()?;
            if payload
                .get("confirmation")
                .is_some_and(|value| !value.is_null() && !value.is_string())
            {
                return None;
            }
        }
        "ask_owner" if matches!(source.kind, EntryKind::Message { .. }) => {
            if payload.get("from")?.as_str()? != source.author.id {
                return None;
            }
            for key in ["draft", "rule"] {
                payload.get(key)?.as_str()?;
            }
        }
        "booking" if matches!(source.kind, EntryKind::ItemEdited { .. }) => {}
        _ => return None,
    }
    Some(note)
}

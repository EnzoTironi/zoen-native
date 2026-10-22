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

/// Our earlier note to the owner about `source`, if one went out.
pub(super) fn owner_note(e: &Arc<Client>, cx: &Ctx, source: &str) -> Option<serde_json::Value> {
    let dm = e.start_direct(cx.owner.clone()).ok()?;
    let needle = format!("\"source\":\"{source}\"");
    e.timeline(dm).unwrap_or_default().iter().find_map(|m| {
        let x = text_of(m).filter(|x| m.author.id == cx.me && x.contains(&needle))?;
        let body = x
            .trim()
            .trim_start_matches("```json")
            .trim_end_matches("```");
        Some(serde_json::from_str(body.trim()).unwrap_or(serde_json::Value::Null))
    })
}

//! Actor-aware app operations and concurrent schedule reconciliation.

use super::{apply, create, schedule, t, view, Actor, AppDoc, Applied, ItemContent, Value};

/// Creation with who is creating it: apps with roles (the schedule) record the owner and
/// the agent; the rest only need the name.
/// # Errors
/// Refuses invalid initialization arguments or actor/client scope requirements for the requested app.
pub fn create_as(
    app: &str,
    args: &Value,
    actor: &Actor,
    now: i64,
) -> Result<(String, Value), String> {
    match app {
        "schedule" => schedule::create(args, actor, now),
        _ => create(app, args, actor.name, now),
    }
}

/// A tool call with who is calling it (member id, not display name).
/// An agent's call may name a client (`forId`/`forName`) only as the member it is serving:
/// the author of the message it is answering. A missing `forId` is filled with them; any
/// other `forId`, or naming a client with nobody being served, is refused.
/// # Errors
/// Refuses a caller-supplied client that differs from the serving client.
pub fn pin_client(args: &mut Value, serving: Option<&str>) -> Result<(), String> {
    let Some(a) = args.as_object_mut() else {
        return Ok(());
    };
    let named = |k: &str| a.get(k).is_some_and(|v| !v.is_null());
    if !named("forId") && !named("forName") {
        return Ok(());
    }
    match (serving, a.get("forId").filter(|v| !v.is_null())) {
        (Some(s), None) => {
            a.insert("forId".into(), s.into());
            Ok(())
        }
        (Some(s), Some(f)) if f.as_str() == Some(s) => Ok(()),
        _ => Err(t(
            "o agente só marca para quem está atendendo",
            "the agent only books for the member it's serving",
        )),
    }
}

/// # Errors
/// Refuses invalid calls or actor/client roles that do not authorize the requested domain operation.
pub fn apply_as(
    app: &str,
    tool_name: &str,
    args: &Value,
    state: &Value,
    actor: &Actor,
    now: i64,
) -> Result<Applied, String> {
    match app {
        "schedule" => schedule::apply(tool_name, args, view(app, state, now), actor, now),
        _ => apply(app, tool_name, args, state, actor.name, now),
    }
}

/// An agent call that already landed for the same `ref` (its source message), as the
/// result to report again. Only the schedule books, so only it replays.
#[must_use]
pub fn replayed(app: &str, tool_name: &str, args: &Value, state: &Value) -> Option<String> {
    (app == "schedule")
        .then(|| schedule::replayed(tool_name, args, state))
        .flatten()
}

/// An exact accepted effect owned by the same agent, never a request to execute it.
#[must_use]
pub fn recover_tool_result(
    app: &str,
    tool: &str,
    args: &Value,
    state: &Value,
    actor: &Actor<'_>,
) -> Option<String> {
    if app != "schedule" || tool != "schedule_book" || actor.owner.is_none() {
        return None;
    }
    let reference = args["ref"].as_str()?;
    let service = args["serviceId"].as_str()?;
    if !state["bookings"].as_array()?.iter().any(|b| {
        b["ref"].as_str() == Some(reference)
            && b["byId"].as_str() == Some(actor.id)
            && b["serviceId"].as_str() == Some(service)
            && b["startMs"] == args["startMs"]
            && args
                .get("endMs")
                .filter(|value| !value.is_null())
                .is_none_or(|end| b["endMs"] == *end)
            && b["forId"] == args["forId"]
            && b["note"].as_str().unwrap_or_default() == args["note"].as_str().unwrap_or_default()
            && matches!(b["status"].as_str(), Some("pending" | "confirmed"))
    }) {
        return None;
    }
    replayed(app, tool, args, state)
}

/// The owner's note for what a client just did in a mini-app (only the schedule speaks).
#[must_use]
pub fn owner_note_for(prev: &AppDoc, next: &AppDoc, by: &str) -> Option<Value> {
    if next.app != "schedule" {
        return None;
    }
    let p: Value = serde_json::from_str(&prev.state_json).ok()?;
    let n: Value = serde_json::from_str(&next.state_json).ok()?;
    schedule::booking_change(&p, &n, by)
}

/// Folding a new version of a shared mini-app on top of the current one: apps whose
/// state two devices may change at the same moment (the schedule) merge here, the same
/// way on every device. `None` keeps the incoming version as it is.
#[must_use]
pub fn reconcile_concurrent(
    current: &ItemContent,
    incoming: &ItemContent,
    at: i64,
) -> Option<ItemContent> {
    let (ItemContent::App(cur), ItemContent::App(inc)) = (current, incoming) else {
        return None;
    };
    if cur.app != "schedule" || inc.app != "schedule" {
        return None;
    }
    let c: Value = serde_json::from_str(&cur.state_json).ok()?;
    let i: Value = serde_json::from_str(&inc.state_json).ok()?;
    let merged = schedule::reconcile(&c, &i, at)?;
    Some(ItemContent::App(AppDoc {
        state_json: merged.to_string(),
        title: cur.title.clone(),
        ..inc.clone()
    }))
}

/// The view a given member receives. Apps with private parts (the schedule's client
/// names and notes) filter here; the rest are the same for everyone.
#[must_use]
pub fn view_for(app: &str, state: &Value, now: i64, viewer: &str) -> Value {
    match app {
        "schedule" => schedule::view_for(&view(app, state, now), viewer, now),
        _ => view(app, state, now),
    }
}

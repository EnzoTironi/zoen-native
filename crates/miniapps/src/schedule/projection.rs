//! Agenda projection.

use super::*;

/// What `viewer` may see. The owner, the agent and resolvers get everything; a client
/// gets the business, busy times, and only their own bookings and events.
pub fn view_for(st: &Value, viewer: &str, now: i64) -> Value {
    let r = role(st, viewer);
    let mut v = st.clone();
    v["busy"] = json!(busy(st, now));
    v["viewer"] = json!({ "id": viewer, "role": r.key() });
    if r.staff() {
        return v;
    }
    let mine: Vec<Value> = st["bookings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| is_for(b, viewer))
        .cloned()
        .collect();
    let ids: Vec<&str> = mine.iter().filter_map(|b| b["id"].as_str()).collect();
    let events: Vec<Value> = st["events"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["bookingId"].as_str().is_some_and(|id| ids.contains(&id)))
        .cloned()
        .collect();
    v["bookings"] = json!(mine);
    v["events"] = json!(events);
    v["log"] = json!([]);
    v["resolvers"] = json!([]);
    v
}

pub fn pending(st: &Value) -> usize {
    st["bookings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["status"] == "pending")
        .count()
}

pub fn accent(st: &Value) -> &str {
    st["accentHex"].as_str().unwrap_or("#3D7A28")
}

/// What `by` did to a booking that is theirs ([`is_for`]), between two versions of a
/// schedule, as the owner's `booking` note: `new`, `cancelled` (withdrawn) or
/// `cancel_request`. A client acting for themselves and the agent booking for a client both
/// speak; the owner and resolvers stay silent, and so does staff declining or cancelling
/// (that resolves the request as "cancelled", not "withdrawn").
pub(crate) fn booking_change(prev: &Value, next: &Value, by: &str) -> Option<Value> {
    if !matches!(role(next, by), Role::Client | Role::Agent) {
        return None;
    }
    let event = |st: &Value, id: &Value, kind: &str, status: &str| {
        st["events"].as_array().into_iter().flatten().any(|e| {
            &e["bookingId"] == id
                && e["kind"] == kind
                && (e["status"] == status || e["resolution"] == status)
        })
    };
    let before = prev["bookings"].as_array().cloned().unwrap_or_default();
    next["bookings"]
        .as_array()?
        .iter()
        .filter(|b| is_for(b, by))
        .find_map(|b| {
            let id = &b["id"];
            let kind = match before.iter().find(|o| &o["id"] == id) {
                None if b["status"] == "pending" => "new",
                Some(o)
                    if o["status"] == "pending"
                        && b["status"] == "cancelled"
                        && event(next, id, "book_request", "withdrawn") =>
                {
                    "cancelled"
                }
                Some(_)
                    if event(next, id, "cancel_request", "open")
                        && !event(prev, id, "cancel_request", "open") =>
                {
                    "cancel_request"
                }
                _ => return None,
            };
            let client = b.get("forName").unwrap_or(&b["byName"]);
            Some(json!({"booking": {
                "kind": kind,
                "client": client,
                "service": b["serviceName"],
                "when": when_label(next, b["startMs"].as_i64().unwrap_or(0)),
            }}))
        })
}

//! Agenda bookings.

use super::*;

pub(super) fn booking_index(st: &Value, args: &Value) -> Result<usize, String> {
    let id = s(args, "bookingId").ok_or_else(|| t("qual horário?", "which booking?"))?;
    st["bookings"]
        .as_array()
        .into_iter()
        .flatten()
        .position(|b| b["id"].as_str() == Some(id.as_str()))
        .ok_or_else(|| t("horário não encontrado", "booking not found"))
}

pub(super) fn next_id(st: &mut Value, prefix: &str) -> String {
    let n = st["next"].as_i64().unwrap_or(1);
    st["next"] = json!(n + 1);
    format!("{prefix}{n}")
}

pub(super) fn push_event(
    st: &mut Value,
    kind: &str,
    booking: &str,
    by: &str,
    now: i64,
) -> Result<(), String> {
    let id = next_id(st, "e");
    st["events"]
        .as_array_mut()
        .ok_or_else(|| t("estado inválido", "invalid state"))?
        .push(json!({ "id": id, "kind": kind, "bookingId": booking, "status": "open", "at": now, "by": by }));
    Ok(())
}

/// Closes the open events of a booking (optionally of one kind), keeping them as history.
pub(super) fn resolve_events(
    st: &mut Value,
    booking: &str,
    kind: Option<&str>,
    resolution: &str,
    by: &str,
    now: i64,
) -> usize {
    let mut n = 0;
    for ev in st["events"].as_array_mut().into_iter().flatten() {
        if ev["bookingId"].as_str() == Some(booking)
            && ev["status"] == "open"
            && kind.is_none_or(|k| ev["kind"].as_str() == Some(k))
        {
            ev["status"] = json!("resolved");
            ev["resolution"] = json!(resolution);
            ev["resolvedBy"] = json!(by);
            ev["resolvedAt"] = json!(now);
            n += 1;
        }
    }
    n
}

pub(super) fn has_open(st: &Value, booking: &str, kind: &str) -> bool {
    st["events"].as_array().into_iter().flatten().any(|e| {
        e["bookingId"].as_str() == Some(booking)
            && e["status"] == "open"
            && e["kind"].as_str() == Some(kind)
    })
}

/// Keeps the state bounded: oldest resolved events and resolved bookings go first.
pub(super) fn trim(st: &mut Value, now: i64) {
    if let Some(events) = st["events"].as_array_mut() {
        while events.len() > MAX_EVENTS {
            match events.iter().position(|e| e["status"] != "open") {
                Some(i) => {
                    events.remove(i);
                }
                None => break,
            }
        }
    }
    let done = |b: &Value| {
        b["status"] == "cancelled"
            || (b["status"] == "confirmed" && b["endMs"].as_i64().unwrap_or(0) < now)
    };
    if let Some(bookings) = st["bookings"].as_array_mut() {
        let mut resolved: Vec<(i64, String)> = bookings
            .iter()
            .filter(|b| done(b))
            .map(|b| {
                (
                    b["startMs"].as_i64().unwrap_or(0),
                    b["id"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect();
        if resolved.len() > MAX_RESOLVED_BOOKINGS {
            resolved.sort();
            let drop: Vec<String> = resolved[..resolved.len() - MAX_RESOLVED_BOOKINGS]
                .iter()
                .map(|x| x.1.clone())
                .collect();
            bookings.retain(|b| !drop.iter().any(|d| b["id"].as_str() == Some(d)));
        }
    }
    let live: Vec<String> = st["bookings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| b["id"].as_str().map(String::from))
        .collect();
    if let Some(events) = st["events"].as_array_mut() {
        events.retain(|e| {
            e["status"] == "open"
                || e["bookingId"]
                    .as_str()
                    .is_some_and(|id| live.iter().any(|l| l == id))
        });
    }
}

pub(super) fn note(args: &Value) -> Option<String> {
    args["note"].as_str().map(|n| clean(n, 120))
}

/// Two devices booked from the same state at the same moment: both writes carry the same
/// `rev`, and the log orders them. The fold keeps `current` (the earlier write) and takes
/// from the later one only the bookings it added: each gets a fresh id, and one that now
/// overlaps a booking already there is kept as cancelled with `conflict`, so its client
/// sees the slot was taken. Any other change in the later write is dropped. Every device
/// folds the same log in the same order, so all of them land on the same state.
///
/// `None` when `incoming` was written on top of `current` (the normal case), or when
/// either side predates `rev`.
pub fn reconcile(current: &Value, incoming: &Value, at: i64) -> Option<Value> {
    let (cr, ir) = (current["rev"].as_i64()?, incoming["rev"].as_i64()?);
    if ir > cr {
        return None;
    }
    let mut st = current.clone();
    // A booking keeps its id, author and creation time for life (a reschedule moves the
    // time, a resolution the status), so these three name it across both writes.
    let same =
        |a: &Value, b: &Value| a["id"] == b["id"] && a["byId"] == b["byId"] && a["at"] == b["at"];
    let theirs: Vec<Value> = incoming["bookings"].as_array().cloned().unwrap_or_default();
    let ours: Vec<Value> = current["bookings"].as_array().cloned().unwrap_or_default();
    let mut added = 0;
    for b in theirs.iter().filter(|b| !ours.iter().any(|o| same(o, b))) {
        // Only bookings this write created: they are still pending.
        if b["status"] != "pending" {
            continue;
        }
        let id = next_id(&mut st, "b");
        let (start, end) = (
            b["startMs"].as_i64().unwrap_or(0),
            b["endMs"].as_i64().unwrap_or(0),
        );
        let clash = st["bookings"].as_array().into_iter().flatten().any(|o| {
            occupies(o, at)
                && o["startMs"].as_i64().unwrap_or(0) < end
                && o["endMs"].as_i64().unwrap_or(0) > start
        });
        let mut nb = b.clone();
        nb["id"] = json!(id);
        let by = b["byId"].as_str().unwrap_or_default().to_string();
        if clash {
            nb["status"] = json!("cancelled");
            nb["conflict"] = json!(true);
        }
        st["bookings"].as_array_mut()?.push(nb);
        if clash {
            let svc = b["serviceName"].as_str().unwrap_or_default().to_string();
            push_log(
                &mut st,
                b["byName"].as_str().unwrap_or_default(),
                &tr!(
                    "pediu {svc}, mas o horário tinha acabado de ser marcado",
                    "requested {svc}, but the slot had just been taken"
                ),
                at,
            );
        } else {
            push_event(&mut st, "book_request", &id, &by, at).ok()?;
        }
        added += 1;
    }
    if added == 0 {
        // Nothing new to carry over: the earlier write stands as it is.
        return Some(current.clone());
    }
    trim(&mut st, at);
    st["rev"] = json!(cr + 1);
    Some(st)
}

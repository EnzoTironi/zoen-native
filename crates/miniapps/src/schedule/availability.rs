//! Agenda availability.

use super::{
    has_open, hm, json, parse_tz, push_event, resolve_events, t, tr, Datelike, Timelike, Tz, Value,
    DAY, DEFAULT_HORIZON_DAYS, DEFAULT_TZ, MINUTE,
};

// ── reading ──

pub(super) fn occupies(b: &Value, now: i64) -> bool {
    matches!(b["status"].as_str(), Some("pending" | "confirmed"))
        && b["endMs"].as_i64().unwrap_or(0) > now
}

/// A booking is `id`'s when they made it or it was made for them (the agent or staff
/// booking in their name), so they see and cancel it like one of their own.
pub(super) fn is_for(b: &Value, id: &str) -> bool {
    b["byId"].as_str() == Some(id) || b["forId"].as_str() == Some(id)
}

pub(super) fn busy(st: &Value, now: i64) -> Vec<Value> {
    let mut out: Vec<(i64, i64)> = st["bookings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| occupies(b, now))
        .map(|b| {
            (
                b["startMs"].as_i64().unwrap_or(0),
                b["endMs"].as_i64().unwrap_or(0),
            )
        })
        .collect();
    out.sort_unstable();
    out.into_iter()
        .map(|(a, b)| json!({ "startMs": a, "endMs": b }))
        .collect()
}

// ── the rules ──

pub(super) fn local(tz: Tz, ms: i64) -> Option<chrono::DateTime<Tz>> {
    chrono::DateTime::from_timestamp_millis(ms).map(|d| d.with_timezone(&tz))
}

pub(super) fn when_label(st: &Value, ms: i64) -> String {
    let tz = st["timezone"]
        .as_str()
        .and_then(|z| z.parse::<Tz>().ok())
        .unwrap_or(chrono_tz::UTC);
    local(tz, ms)
        .map(|d| d.format("%d/%m %H:%M").to_string())
        .unwrap_or_default()
}

/// A slot is bookable if it starts on a minute, in the future, within the horizon, on a
/// slot boundary inside one opening window of that local day, and overlaps no pending or
/// confirmed booking other than `ignore`.
pub(super) fn check_slot(
    st: &Value,
    start: i64,
    minutes: i64,
    now: i64,
    ignore: Option<&str>,
) -> Result<i64, String> {
    let end = start + minutes * MINUTE;
    if start % MINUTE != 0 {
        return Err(t("horário inválido", "invalid time"));
    }
    if start <= now {
        return Err(t("esse horário já passou", "that time has passed"));
    }
    let horizon = st["horizonDays"].as_i64().unwrap_or(DEFAULT_HORIZON_DAYS);
    if start > now + horizon * DAY {
        return Err(tr!(
            "só dá para marcar até {horizon} dias à frente",
            "you can only book up to {horizon} days ahead"
        ));
    }
    within_hours(st, start, end)?;
    let clash = st["bookings"].as_array().into_iter().flatten().any(|b| {
        b["id"].as_str() != ignore
            && occupies(b, now)
            && b["startMs"].as_i64().unwrap_or(0) < end
            && b["endMs"].as_i64().unwrap_or(0) > start
    });
    if clash {
        return Err(t(
            "esse horário já está ocupado",
            "that time is already taken",
        ));
    }
    Ok(end)
}

/// `[start, end)` sits on a slot boundary inside one opening window of its local day.
pub(super) fn within_hours(st: &Value, start: i64, end: i64) -> Result<(), String> {
    let tz = parse_tz(st["timezone"].as_str().unwrap_or(DEFAULT_TZ))?;
    let (ls, le) = (local(tz, start), local(tz, end));
    let (Some(ls), Some(le)) = (ls, le) else {
        return Err(t("horário inválido", "invalid time"));
    };
    let day = ls.weekday().num_days_from_sunday().to_string();
    let from = i64::from(ls.hour() * 60 + ls.minute());
    let to = if le.date_naive() == ls.date_naive() {
        i64::from(le.hour() * 60 + le.minute())
    } else if le.date_naive() == ls.date_naive().succ_opt().unwrap_or(ls.date_naive())
        && le.hour() == 0
        && le.minute() == 0
    {
        24 * 60
    } else {
        return Err(t("fora do horário de atendimento", "outside opening hours"));
    };
    let step = st["slotMinutes"].as_i64().unwrap_or(30).max(5);
    let inside = st["weeklyHours"][day.as_str()]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| Some((hm(w[0].as_str()?)?, hm(w[1].as_str()?)?)))
        .any(|(f, tt)| f <= from && to <= tt && (from - f) % step == 0);
    if !inside {
        return Err(t("fora do horário de atendimento", "outside opening hours"));
    }
    Ok(())
}

/// After the hours change, flags the upcoming bookings that now fall outside them (one
/// open `outside_hours` event each, for the agent to move, cancel or keep) and clears the
/// flag on those that fit again. Returns the labels of the newly flagged ones.
pub(super) fn flag_outside_hours(st: &mut Value, by: &str, now: i64) -> Vec<String> {
    let mut flagged = Vec::new();
    let bookings: Vec<Value> = st["bookings"].as_array().cloned().unwrap_or_default();
    for (i, b) in bookings.iter().enumerate() {
        if !occupies(b, now) || b["hoursKept"].as_bool().unwrap_or(false) {
            continue;
        }
        let id = b["id"].as_str().unwrap_or_default().to_string();
        let (start, end) = (
            b["startMs"].as_i64().unwrap_or(0),
            b["endMs"].as_i64().unwrap_or(0),
        );
        let outside = within_hours(st, start, end).is_err();
        let open = has_open(st, &id, "outside_hours");
        if outside && !open {
            st["bookings"][i]["outsideHours"] = json!(true);
            if push_event(st, "outside_hours", &id, by, now).is_ok() {
                flagged.push(format!(
                    "{}, {}",
                    b["serviceName"].as_str().unwrap_or_default(),
                    when_label(st, start)
                ));
            }
        } else if !outside && open {
            if let Some(m) = st["bookings"][i].as_object_mut() {
                m.remove("outsideHours");
            }
            resolve_events(st, &id, Some("outside_hours"), "fits", by, now);
        }
    }
    flagged
}

//! Agenda: a trade-agnostic booking mini-app.
//!
//! The agent fills the business name, services and weekly hours from the owner's own
//! words; clients book free slots; anything special becomes an event the agent resolves.
//! The rules that keep the shared state honest live here, on the core side: no overlap,
//! inside opening hours in the business timezone, no past or far-future times, a cap per
//! member, and only the owner, the agent or a named resolver can resolve or configure.
//! Each viewer receives only what they may see: clients get busy times, not names.

use super::{push_log, s, Applied};
use crate::i18n::t;
use crate::tr;
use chrono::{Datelike, Timelike};
use chrono_tz::Tz;
use serde_json::{json, Map, Value};

/// Open (pending or upcoming confirmed) bookings one member may hold at once.
pub const MAX_OPEN_PER_MEMBER: usize = 2;
/// Events kept in the state; resolved ones are trimmed first, oldest first.
pub const MAX_EVENTS: usize = 60;
/// Resolved bookings (cancelled or past) kept for history.
pub const MAX_RESOLVED_BOOKINGS: usize = 40;
const MAX_SERVICES: usize = 12;
const MAX_WINDOWS_PER_DAY: usize = 4;
const MAX_RESOLVERS: usize = 8;
const DEFAULT_HORIZON_DAYS: i64 = 60;
const DEFAULT_TZ: &str = "America/Sao_Paulo";
const MINUTE: i64 = 60_000;
const DAY: i64 = 86_400_000;

/// Who is acting or viewing: the member id, the display name, and the owner when the
/// actor is an agent.
pub struct Actor<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub owner: Option<&'a str>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Owner,
    Agent,
    Resolver,
    Client,
}

impl Role {
    fn key(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Agent => "agent",
            Role::Resolver => "resolver",
            Role::Client => "client",
        }
    }
    fn staff(self) -> bool {
        self != Role::Client
    }
}

pub fn role(st: &Value, id: &str) -> Role {
    if id.is_empty() {
        return Role::Client;
    }
    if st["ownerId"].as_str() == Some(id) {
        Role::Owner
    } else if st["agentId"].as_str() == Some(id) {
        Role::Agent
    } else if st["resolvers"]
        .as_array()
        .is_some_and(|r| r.iter().any(|x| x.as_str() == Some(id)))
    {
        Role::Resolver
    } else {
        Role::Client
    }
}

// ── input validation ──

/// Printable text, trimmed, at most `max` characters.
fn clean(raw: &str, max: usize) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(max)
        .collect()
}

/// A service id: lowercase letters, digits, `-` and `_`, at most 24 characters.
pub fn service_id(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        } else if (c.is_whitespace() || c == '+') && !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 24 {
            break;
        }
    }
    out.trim_matches('-').to_string()
}

/// Strict `HH:MM` (24h). `24:00` is accepted as the end of the day.
pub fn hm(raw: &str) -> Option<i64> {
    let b = raw.as_bytes();
    if b.len() != 5
        || b[2] != b':'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 2 || c.is_ascii_digit())
    {
        return None;
    }
    let h = (b[0] - b'0') as i64 * 10 + (b[1] - b'0') as i64;
    let m = (b[3] - b'0') as i64 * 10 + (b[4] - b'0') as i64;
    match (h, m) {
        (24, 0) => Some(24 * 60),
        (0..=23, 0..=59) => Some(h * 60 + m),
        _ => None,
    }
}

fn int_in(v: &Value, key: &str, lo: i64, hi: i64) -> Result<Option<i64>, String> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(x) => x
            .as_i64()
            .filter(|n| (lo..=hi).contains(n))
            .map(Some)
            .ok_or_else(|| {
                tr!(
                    "{key}: um inteiro de {lo} a {hi}",
                    "{key}: an integer from {lo} to {hi}"
                )
            }),
    }
}

pub fn parse_services(raw: &Value) -> Result<Vec<Value>, String> {
    let arr = raw
        .as_array()
        .ok_or_else(|| t("services: uma lista", "services: a list"))?;
    if arr.is_empty() || arr.len() > MAX_SERVICES {
        return Err(tr!(
            "services: de 1 a {MAX_SERVICES} serviços",
            "services: 1 to {MAX_SERVICES} services"
        ));
    }
    let mut out: Vec<Value> = Vec::new();
    for (i, svc) in arr.iter().enumerate() {
        if !svc.is_object() {
            return Err(t(
                "services: cada serviço é um objeto",
                "services: each service is an object",
            ));
        }
        let name = clean(svc["name"].as_str().unwrap_or(""), 40);
        if name.is_empty() {
            return Err(t(
                "services: cada serviço precisa de nome",
                "services: each service needs a name",
            ));
        }
        let minutes = int_in(svc, "minutes", 5, 480)?.ok_or_else(|| {
            t(
                "services: minutes de 5 a 480",
                "services: minutes from 5 to 480",
            )
        })?;
        let price = int_in(svc, "priceCents", 0, 100_000_000)?.unwrap_or(0);
        let mut id = svc["id"].as_str().map(service_id).unwrap_or_default();
        if id.is_empty() {
            id = service_id(&name);
        }
        if id.is_empty() {
            id = format!("svc{}", i + 1);
        }
        let base = id.clone();
        let mut n = 2;
        while out.iter().any(|s| s["id"].as_str() == Some(id.as_str())) {
            id = format!("{}-{n}", base.chars().take(21).collect::<String>());
            n += 1;
        }
        out.push(json!({ "id": id, "name": name, "priceCents": price, "minutes": minutes }));
    }
    Ok(out)
}

/// `{"0": [["09:00","12:00"]], … "6": []}`, Sunday = "0". Unknown keys, malformed or
/// overlapping windows are rejected; missing days are closed.
pub fn parse_weekly_hours(raw: &Value) -> Result<Value, String> {
    let obj = raw.as_object().ok_or_else(|| {
        t(
            "weeklyHours: um objeto de \"0\" (domingo) a \"6\"",
            "weeklyHours: an object keyed \"0\" (Sunday) to \"6\"",
        )
    })?;
    if let Some(k) = obj
        .keys()
        .find(|k| !matches!(k.as_str(), "0" | "1" | "2" | "3" | "4" | "5" | "6"))
    {
        return Err(tr!(
            "weeklyHours: dia desconhecido “{k}”",
            "weeklyHours: unknown day “{k}”"
        ));
    }
    let bad = || {
        t(
            "weeklyHours: janelas [\"HH:MM\", \"HH:MM\"] em ordem, sem sobrepor",
            "weeklyHours: windows [\"HH:MM\", \"HH:MM\"] in order, not overlapping",
        )
    };
    let mut out = Map::new();
    for day in 0..7 {
        let key = day.to_string();
        let mut windows: Vec<(i64, i64, String, String)> = Vec::new();
        if let Some(v) = obj.get(&key) {
            let arr = v.as_array().ok_or_else(bad)?;
            if arr.len() > MAX_WINDOWS_PER_DAY {
                return Err(bad());
            }
            for w in arr {
                let pair = w.as_array().filter(|a| a.len() == 2).ok_or_else(bad)?;
                let (fs, ts) = (
                    pair[0].as_str().ok_or_else(bad)?,
                    pair[1].as_str().ok_or_else(bad)?,
                );
                let (f, to) = (hm(fs).ok_or_else(bad)?, hm(ts).ok_or_else(bad)?);
                if f >= to || f >= 24 * 60 {
                    return Err(bad());
                }
                windows.push((f, to, fs.to_string(), ts.to_string()));
            }
        }
        windows.sort_by_key(|w| w.0);
        if windows.windows(2).any(|p| p[1].0 < p[0].1) {
            return Err(bad());
        }
        out.insert(
            key,
            Value::Array(windows.into_iter().map(|w| json!([w.2, w.3])).collect()),
        );
    }
    Ok(Value::Object(out))
}

fn closed_week() -> Value {
    Value::Object((0..7).map(|d| (d.to_string(), json!([]))).collect())
}

fn parse_tz(raw: &str) -> Result<Tz, String> {
    raw.parse::<Tz>().map_err(|_| {
        tr!(
            "fuso horário desconhecido: {raw}",
            "unknown timezone: {raw}"
        )
    })
}

fn parse_accent(raw: &str) -> Result<String, String> {
    let b = raw.as_bytes();
    if b.len() == 7 && b[0] == b'#' && b[1..].iter().all(u8::is_ascii_hexdigit) {
        Ok(raw.to_uppercase())
    } else {
        Err(t(
            "accentHex: uma cor #RRGGBB",
            "accentHex: a #RRGGBB color",
        ))
    }
}

/// Validates every configurable field present in `args` into `st`. Nothing is written
/// unless all of them are valid.
fn configure_fields(st: &mut Value, args: &Value) -> Result<(), String> {
    let mut next = st.clone();
    if let Some(v) = args.get("business").filter(|v| !v.is_null()) {
        let b = clean(
            v.as_str()
                .ok_or_else(|| t("business: um texto", "business: a text"))?,
            48,
        );
        if b.is_empty() {
            return Err(t(
                "business: não pode ficar vazio",
                "business: can't be empty",
            ));
        }
        next["business"] = json!(b);
    }
    if let Some(v) = args.get("title").filter(|v| !v.is_null()) {
        let b = clean(
            v.as_str()
                .ok_or_else(|| t("title: um texto", "title: a text"))?,
            40,
        );
        if b.is_empty() {
            return Err(t("title: não pode ficar vazio", "title: can't be empty"));
        }
        next["title"] = json!(b);
    }
    if let Some(v) = args.get("accentHex") {
        next["accentHex"] = match v.as_str().map(str::trim) {
            None | Some("") => Value::Null,
            Some(a) => json!(parse_accent(a)?),
        };
    }
    if let Some(v) = args.get("timezone").filter(|v| !v.is_null()) {
        let z = v
            .as_str()
            .ok_or_else(|| t("timezone: um texto", "timezone: a text"))?;
        parse_tz(z)?;
        next["timezone"] = json!(z);
    }
    if let Some(v) = args.get("currency").filter(|v| !v.is_null()) {
        let c = v.as_str().unwrap_or("");
        if c.len() != 3 || !c.bytes().all(|b| b.is_ascii_alphabetic()) {
            return Err(t(
                "currency: código ISO de 3 letras",
                "currency: a 3-letter ISO code",
            ));
        }
        next["currency"] = json!(c.to_uppercase());
    }
    if let Some(n) = int_in(args, "slotMinutes", 5, 240)? {
        next["slotMinutes"] = json!(n);
    }
    if let Some(n) = int_in(args, "horizonDays", 1, 365)? {
        next["horizonDays"] = json!(n);
    }
    if let Some(v) = args.get("services").filter(|v| !v.is_null()) {
        next["services"] = json!(parse_services(v)?);
    }
    if let Some(v) = args.get("weeklyHours").filter(|v| !v.is_null()) {
        next["weeklyHours"] = parse_weekly_hours(v)?;
    }
    if let Some(v) = args.get("resolvers").filter(|v| !v.is_null()) {
        let arr = v
            .as_array()
            .filter(|a| a.len() <= MAX_RESOLVERS)
            .ok_or_else(|| {
                tr!(
                    "resolvers: até {MAX_RESOLVERS} ids",
                    "resolvers: up to {MAX_RESOLVERS} ids"
                )
            })?;
        let mut ids: Vec<String> = Vec::new();
        for x in arr {
            let id = clean(
                x.as_str()
                    .ok_or_else(|| t("resolvers: ids de membros", "resolvers: member ids"))?,
                64,
            );
            if !id.is_empty() && !ids.contains(&id) {
                ids.push(id);
            }
        }
        next["resolvers"] = json!(ids);
    }
    next["configured"] = json!(is_configured(&next));
    *st = next;
    Ok(())
}

fn is_configured(st: &Value) -> bool {
    let has_services = st["services"].as_array().is_some_and(|a| !a.is_empty());
    let open = st["weeklyHours"].as_object().is_some_and(|o| {
        o.values()
            .any(|d| d.as_array().is_some_and(|a| !a.is_empty()))
    });
    has_services && open
}

// ── creation ──

pub fn create(args: &Value, actor: &Actor, now: i64) -> Result<(String, Value), String> {
    let mut st = json!({
        "business": t("Agenda", "Schedule"),
        "title": Value::Null,
        "accentHex": Value::Null,
        "timezone": DEFAULT_TZ,
        "currency": "BRL",
        "slotMinutes": 30,
        "horizonDays": DEFAULT_HORIZON_DAYS,
        "services": [],
        "weeklyHours": closed_week(),
        "ownerId": actor.owner.unwrap_or(actor.id),
        "agentId": actor.owner.map(|_| actor.id),
        "resolvers": [],
        "bookings": [],
        "events": [],
        "next": 1,
        "rev": 0,
        "log": []
    });
    configure_fields(&mut st, args)?;
    if st["title"].is_null() {
        st["title"] = st["business"].clone();
    }
    push_log(
        &mut st,
        actor.name,
        &t("abriu a agenda", "opened the schedule"),
        now,
    );
    Ok((st["title"].as_str().unwrap_or_default().to_string(), st))
}

// ── reading ──

fn occupies(b: &Value, now: i64) -> bool {
    matches!(b["status"].as_str(), Some("pending" | "confirmed"))
        && b["endMs"].as_i64().unwrap_or(0) > now
}

fn busy(st: &Value, now: i64) -> Vec<Value> {
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
    out.sort();
    out.into_iter()
        .map(|(a, b)| json!({ "startMs": a, "endMs": b }))
        .collect()
}

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
        .filter(|b| b["byId"].as_str() == Some(viewer))
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

// ── the rules ──

fn local(tz: Tz, ms: i64) -> Option<chrono::DateTime<Tz>> {
    chrono::DateTime::from_timestamp_millis(ms).map(|d| d.with_timezone(&tz))
}

fn when_label(st: &Value, ms: i64) -> String {
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
fn check_slot(
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
fn within_hours(st: &Value, start: i64, end: i64) -> Result<(), String> {
    let tz = parse_tz(st["timezone"].as_str().unwrap_or(DEFAULT_TZ))?;
    let (ls, le) = (local(tz, start), local(tz, end));
    let (Some(ls), Some(le)) = (ls, le) else {
        return Err(t("horário inválido", "invalid time"));
    };
    let day = ls.weekday().num_days_from_sunday().to_string();
    let from = (ls.hour() * 60 + ls.minute()) as i64;
    let to = if le.date_naive() == ls.date_naive() {
        (le.hour() * 60 + le.minute()) as i64
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
fn flag_outside_hours(st: &mut Value, by: &str, now: i64) -> Vec<String> {
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

fn booking_index(st: &Value, args: &Value) -> Result<usize, String> {
    let id = s(args, "bookingId").ok_or_else(|| t("qual horário?", "which booking?"))?;
    st["bookings"]
        .as_array()
        .into_iter()
        .flatten()
        .position(|b| b["id"].as_str() == Some(id.as_str()))
        .ok_or_else(|| t("horário não encontrado", "booking not found"))
}

fn next_id(st: &mut Value, prefix: &str) -> String {
    let n = st["next"].as_i64().unwrap_or(1);
    st["next"] = json!(n + 1);
    format!("{prefix}{n}")
}

fn push_event(st: &mut Value, kind: &str, booking: &str, by: &str, now: i64) -> Result<(), String> {
    let id = next_id(st, "e");
    st["events"]
        .as_array_mut()
        .ok_or_else(|| t("estado inválido", "invalid state"))?
        .push(json!({ "id": id, "kind": kind, "bookingId": booking, "status": "open", "at": now, "by": by }));
    Ok(())
}

/// Closes the open events of a booking (optionally of one kind), keeping them as history.
fn resolve_events(
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

fn has_open(st: &Value, booking: &str, kind: &str) -> bool {
    st["events"].as_array().into_iter().flatten().any(|e| {
        e["bookingId"].as_str() == Some(booking)
            && e["status"] == "open"
            && e["kind"].as_str() == Some(kind)
    })
}

/// Keeps the state bounded: oldest resolved events and resolved bookings go first.
fn trim(st: &mut Value, now: i64) {
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

fn note(args: &Value) -> Option<String> {
    args["note"].as_str().map(|n| clean(n, 120))
}

/// A tool call. Every change bumps `rev`, so a write that two devices made from the same
/// state can be told apart from one made on top of the other (see [`reconcile`]).
pub fn apply(
    tool: &str,
    args: &Value,
    st: Value,
    actor: &Actor,
    now: i64,
) -> Result<Applied, String> {
    let rev = st["rev"].as_i64().unwrap_or(0);
    let mut a = apply_tool(tool, args, st, actor, now)?;
    if tool != "schedule_get_state" {
        a.state["rev"] = json!(rev + 1);
    }
    Ok(a)
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

fn apply_tool(
    tool: &str,
    args: &Value,
    mut st: Value,
    actor: &Actor,
    now: i64,
) -> Result<Applied, String> {
    let who = role(&st, actor.id);
    match tool {
        "schedule_get_state" => Ok(Applied::new(st, String::new(), String::new())),
        "schedule_configure" => {
            if !matches!(who, Role::Owner | Role::Agent) {
                return Err(t(
                    "só o dono ou o agente ajustam a agenda",
                    "only the owner or the agent can configure the schedule",
                ));
            }
            if args.get("resolvers").is_some() && who != Role::Owner {
                return Err(t(
                    "só o dono escolhe quem resolve pedidos",
                    "only the owner picks who resolves requests",
                ));
            }
            configure_fields(&mut st, args)?;
            let flagged = flag_outside_hours(&mut st, actor.id, now);
            push_log(
                &mut st,
                actor.name,
                &t("ajustou a agenda", "updated the schedule"),
                now,
            );
            trim(&mut st, now);
            let mut a = Applied::new(
                st,
                tr!("{} ajustou a agenda", "{} updated the schedule", actor.name),
                t("Agenda atualizada.", "Schedule updated."),
            );
            a.title = a.state["title"].as_str().map(String::from);
            if !flagged.is_empty() {
                let list = flagged.join("; ");
                a.text = tr!(
                    "Agenda atualizada. Ficaram fora do novo horário: {list}.",
                    "Schedule updated. Now outside the new hours: {list}."
                );
                a.comment = Some(tr!(
                    "Com o novo horário, estes ficaram de fora: {list}. Remarco, cancelo ou mantenho?",
                    "With the new hours, these fall outside: {list}. Reschedule, cancel or keep?"
                ));
            }
            Ok(a)
        }
        "schedule_book" => {
            if !st["configured"].as_bool().unwrap_or(false) {
                return Err(t(
                    "a agenda ainda não está aberta",
                    "the schedule isn't open yet",
                ));
            }
            let sid = s(args, "serviceId")
                .map(|x| service_id(&x))
                .ok_or_else(|| t("qual serviço?", "which service?"))?;
            let svc = st["services"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|x| x["id"].as_str() == Some(sid.as_str()))
                .cloned()
                .ok_or_else(|| t("serviço desconhecido", "unknown service"))?;
            let minutes = svc["minutes"].as_i64().unwrap_or(30);
            let start = args["startMs"]
                .as_i64()
                .ok_or_else(|| t("qual horário?", "which time?"))?;
            if let Some(end) = args.get("endMs").filter(|v| !v.is_null()) {
                if end.as_i64() != Some(start + minutes * MINUTE) {
                    return Err(tr!(
                        "{} dura {minutes} min",
                        "{} takes {minutes} min",
                        svc["name"].as_str().unwrap_or("")
                    ));
                }
            }
            if who == Role::Client {
                let open = st["bookings"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|b| b["byId"].as_str() == Some(actor.id) && occupies(b, now))
                    .count();
                if open >= MAX_OPEN_PER_MEMBER {
                    return Err(tr!(
                        "você já tem {MAX_OPEN_PER_MEMBER} horários em aberto",
                        "you already have {MAX_OPEN_PER_MEMBER} open bookings"
                    ));
                }
            }
            let end = check_slot(&st, start, minutes, now, None)?;
            let id = next_id(&mut st, "b");
            let svc_name = svc["name"].as_str().unwrap_or_default().to_string();
            st["bookings"]
                .as_array_mut()
                .ok_or_else(|| t("estado inválido", "invalid state"))?
                .push(json!({
                    "id": id,
                    "serviceId": sid,
                    "serviceName": svc_name,
                    "minutes": minutes,
                    "priceCents": svc["priceCents"],
                    "startMs": start,
                    "endMs": end,
                    "byId": actor.id,
                    "byName": clean(actor.name, 48),
                    "status": "pending",
                    "note": note(args).unwrap_or_default(),
                    "at": now
                }));
            push_event(&mut st, "book_request", &id, actor.id, now)?;
            push_log(
                &mut st,
                actor.name,
                &tr!("pediu {svc_name}", "requested {svc_name}"),
                now,
            );
            trim(&mut st, now);
            let when = when_label(&st, start);
            let mut a = Applied::new(
                st,
                tr!(
                    "Novo pedido: {svc_name}, {when}",
                    "New request: {svc_name}, {when}"
                ),
                tr!(
                    "Pedido: {svc_name}, {when}. O agente confirma.",
                    "Request: {svc_name}, {when}. The agent confirms."
                ),
            );
            a.comment = Some(tr!(
                "Novo pedido: {svc_name}, {when}. Confirmo, remarco ou cancelo?",
                "New request: {svc_name}, {when}. Confirm, reschedule or cancel?"
            ));
            Ok(a)
        }
        "schedule_resolve" => {
            if !who.staff() {
                return Err(t(
                    "só o dono, o agente ou quem ele escolheu resolvem pedidos",
                    "only the owner, the agent or a chosen resolver can resolve requests",
                ));
            }
            let idx = booking_index(&st, args)?;
            let action = s(args, "action").ok_or_else(|| {
                t(
                    "confirmar, remarcar, cancelar ou manter?",
                    "confirm, adjust, cancel or keep?",
                )
            })?;
            let b = st["bookings"][idx].clone();
            let id = b["id"].as_str().unwrap_or_default().to_string();
            if who == Role::Resolver && b["byId"].as_str() == Some(actor.id) {
                return Err(t(
                    "quem pediu o horário não resolve o próprio pedido",
                    "whoever requested the slot can't resolve their own request",
                ));
            }
            let status = b["status"].as_str().unwrap_or("pending").to_string();
            let svc_name = b["serviceName"].as_str().unwrap_or_default().to_string();
            let start = b["startMs"].as_i64().unwrap_or(0);
            let what = match action.as_str() {
                "confirm" => {
                    if status != "pending" {
                        return Err(t(
                            "esse pedido não está aguardando",
                            "that request isn't waiting",
                        ));
                    }
                    let clash = st["bookings"].as_array().into_iter().flatten().any(|o| {
                        o["id"].as_str() != Some(id.as_str())
                            && occupies(o, now)
                            && o["startMs"].as_i64().unwrap_or(0) < b["endMs"].as_i64().unwrap_or(0)
                            && o["endMs"].as_i64().unwrap_or(0) > start
                    });
                    if clash {
                        return Err(t(
                            "esse horário já está ocupado",
                            "that time is already taken",
                        ));
                    }
                    st["bookings"][idx]["status"] = json!("confirmed");
                    resolve_events(
                        &mut st,
                        &id,
                        Some("book_request"),
                        "confirmed",
                        actor.id,
                        now,
                    );
                    tr!(
                        "confirmou {svc_name}, {}",
                        "confirmed {svc_name}, {}",
                        when_label(&st, start)
                    )
                }
                "adjust" => {
                    if !matches!(status.as_str(), "pending" | "confirmed") {
                        return Err(t(
                            "esse horário já foi cancelado",
                            "that booking was cancelled",
                        ));
                    }
                    let new_start = args["startMs"]
                        .as_i64()
                        .ok_or_else(|| t("para quando?", "to when?"))?;
                    let minutes = b["minutes"].as_i64().unwrap_or(30);
                    if let Some(end) = args.get("endMs").filter(|v| !v.is_null()) {
                        if end.as_i64() != Some(new_start + minutes * MINUTE) {
                            return Err(tr!(
                                "{svc_name} dura {minutes} min",
                                "{svc_name} takes {minutes} min"
                            ));
                        }
                    }
                    let end = check_slot(&st, new_start, minutes, now, Some(&id))?;
                    st["bookings"][idx]["startMs"] = json!(new_start);
                    st["bookings"][idx]["endMs"] = json!(end);
                    st["bookings"][idx]["status"] = json!("confirmed");
                    if let Some(m) = st["bookings"][idx].as_object_mut() {
                        m.remove("outsideHours");
                    }
                    // A new time answers everything open on it: the request, a cancel
                    // request (the client gets the new time instead), an hours flag.
                    resolve_events(&mut st, &id, None, "adjusted", actor.id, now);
                    tr!(
                        "remarcou {svc_name} para {}",
                        "moved {svc_name} to {}",
                        when_label(&st, new_start)
                    )
                }
                "cancel" => {
                    if !matches!(status.as_str(), "pending" | "confirmed") {
                        return Err(t(
                            "esse horário já foi cancelado",
                            "that booking was cancelled",
                        ));
                    }
                    st["bookings"][idx]["status"] = json!("cancelled");
                    resolve_events(&mut st, &id, None, "cancelled", actor.id, now);
                    tr!(
                        "cancelou {svc_name}, {}",
                        "cancelled {svc_name}, {}",
                        when_label(&st, start)
                    )
                }
                "keep" => {
                    let cancel = has_open(&st, &id, "cancel_request");
                    let hours = has_open(&st, &id, "outside_hours");
                    if !cancel && !hours {
                        return Err(t(
                            "não há nada aberto nesse horário",
                            "there's nothing open on that booking",
                        ));
                    }
                    resolve_events(&mut st, &id, Some("cancel_request"), "kept", actor.id, now);
                    if hours {
                        resolve_events(&mut st, &id, Some("outside_hours"), "kept", actor.id, now);
                        if let Some(m) = st["bookings"][idx].as_object_mut() {
                            m.remove("outsideHours");
                            // Kept on purpose: later hour changes don't flag it again.
                            m.insert("hoursKept".into(), json!(true));
                        }
                    }
                    tr!(
                        "manteve {svc_name}, {}",
                        "kept {svc_name}, {}",
                        when_label(&st, start)
                    )
                }
                _ => return Err(t("ação desconhecida", "unknown action")),
            };
            if let Some(n) = note(args) {
                st["bookings"][idx]["note"] = json!(n);
            }
            push_log(&mut st, actor.name, &what, now);
            trim(&mut st, now);
            let mut a = Applied::new(st, format!("{} {what}", actor.name), format!("{what}."));
            a.comment = Some(format!("{what}."));
            Ok(a)
        }
        "schedule_cancel" => {
            let idx = booking_index(&st, args)?;
            let b = st["bookings"][idx].clone();
            if b["byId"].as_str() != Some(actor.id) {
                return Err(t(
                    "só quem marcou pode pedir o cancelamento",
                    "only the person who booked can request a cancellation",
                ));
            }
            let id = b["id"].as_str().unwrap_or_default().to_string();
            let svc_name = b["serviceName"].as_str().unwrap_or_default().to_string();
            let a = match b["status"].as_str() {
                Some("pending") => {
                    st["bookings"][idx]["status"] = json!("cancelled");
                    resolve_events(
                        &mut st,
                        &id,
                        Some("book_request"),
                        "withdrawn",
                        actor.id,
                        now,
                    );
                    push_log(
                        &mut st,
                        actor.name,
                        &tr!("desistiu de {svc_name}", "withdrew {svc_name}"),
                        now,
                    );
                    trim(&mut st, now);
                    Applied::new(
                        st,
                        tr!(
                            "Pedido retirado: {svc_name}",
                            "Request withdrawn: {svc_name}"
                        ),
                        t("Pedido cancelado.", "Request cancelled."),
                    )
                }
                Some("confirmed") => {
                    if has_open(&st, &id, "cancel_request") {
                        return Err(t(
                            "o cancelamento já foi pedido",
                            "cancellation was already requested",
                        ));
                    }
                    push_event(&mut st, "cancel_request", &id, actor.id, now)?;
                    push_log(
                        &mut st,
                        actor.name,
                        &tr!(
                            "pediu para cancelar {svc_name}",
                            "asked to cancel {svc_name}"
                        ),
                        now,
                    );
                    trim(&mut st, now);
                    let when = when_label(&st, b["startMs"].as_i64().unwrap_or(0));
                    let mut a = Applied::new(
                        st,
                        tr!(
                            "Pedido de cancelamento: {svc_name}, {when}",
                            "Cancel request: {svc_name}, {when}"
                        ),
                        t("Pedido de cancelamento enviado.", "Cancel request sent."),
                    );
                    a.comment = Some(tr!(
                        "Pediram para cancelar {svc_name}, {when}. Cancelo ou mantenho?",
                        "Someone asked to cancel {svc_name}, {when}. Cancel or keep?"
                    ));
                    a
                }
                _ => return Err(t("já está cancelado", "already cancelled")),
            };
            Ok(a)
        }
        other => Err(tr!(
            "schedule não tem a ferramenta {other}",
            "schedule has no tool {other}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const AGENT: Actor = Actor {
        id: "bot",
        name: "Agenda",
        owner: Some("bruno"),
    };
    const CARLA: Actor = Actor {
        id: "carla",
        name: "Carla",
        owner: None,
    };
    const DANI: Actor = Actor {
        id: "dani",
        name: "Dani",
        owner: None,
    };

    /// Monday 7 Jan 2030 at `h:m` in São Paulo.
    fn at(h: u32, m: u32) -> i64 {
        chrono_tz::America::Sao_Paulo
            .with_ymd_and_hms(2030, 1, 7, h, m, 0)
            .unwrap()
            .timestamp_millis()
    }

    fn open_schedule() -> Value {
        let args = json!({
            "business": "Barbearia",
            "services": [{"name": "Corte", "minutes": 30}],
            "weeklyHours": {"1": [["09:00", "18:00"]]}
        });
        create(&args, &AGENT, at(7, 0)).unwrap().1
    }

    fn call(st: &Value, who: &Actor, tool: &str, args: Value, now: i64) -> Value {
        apply(tool, &args, st.clone(), who, now).unwrap().state
    }

    fn book(st: &Value, who: &Actor, start: i64, now: i64) -> Value {
        call(
            st,
            who,
            "schedule_book",
            json!({"serviceId": "corte", "startMs": start}),
            now,
        )
    }

    fn booking<'a>(st: &'a Value, by: &str) -> Vec<&'a Value> {
        st["bookings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["byId"] == by)
            .collect()
    }

    fn open_events(st: &Value, id: &str) -> Vec<String> {
        st["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["bookingId"] == id && e["status"] == "open")
            .map(|e| e["kind"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn two_devices_booking_the_same_slot_resolve_the_same_way_everywhere() {
        let base = open_schedule();
        let ten = at(10, 0);
        // Both clients book 10:00 from the same state, at the same moment.
        let carla = book(&base, &CARLA, ten, at(8, 0));
        let dani = book(&base, &DANI, ten, at(8, 0));
        assert_eq!(carla["rev"], dani["rev"], "same base, same rev");

        // The log put Carla's write first: hers stands, Dani's is kept as a conflict.
        let folded = reconcile(&carla, &dani, at(8, 0)).expect("concurrent writes merge");
        assert_eq!(booking(&folded, "carla")[0]["status"], "pending");
        let lost = booking(&folded, "dani");
        assert_eq!(lost.len(), 1);
        assert_eq!(lost[0]["status"], "cancelled");
        assert_eq!(lost[0]["conflict"], true);
        assert_ne!(
            lost[0]["id"],
            booking(&folded, "carla")[0]["id"],
            "fresh id"
        );
        assert!(open_events(&folded, lost[0]["id"].as_str().unwrap()).is_empty());
        assert_eq!(busy(&folded, at(8, 0)).len(), 1, "the slot is held once");
        // Every device folds the same pair the same way.
        assert_eq!(reconcile(&carla, &dani, at(8, 0)), Some(folded.clone()));

        // The other order hands the slot to Dani instead: whoever the log puts first wins.
        let other = reconcile(&dani, &carla, at(8, 0)).unwrap();
        assert_eq!(booking(&other, "dani")[0]["status"], "pending");
        assert_eq!(booking(&other, "carla")[0]["conflict"], true);

        // A write made on top of the merged state folds normally.
        let next = book(&folded, &DANI, at(11, 0), at(8, 1));
        assert_eq!(reconcile(&folded, &next, at(8, 1)), None);
    }

    #[test]
    fn concurrent_bookings_of_different_slots_both_stand() {
        let base = open_schedule();
        let carla = book(&base, &CARLA, at(10, 0), at(8, 0));
        let dani = book(&base, &DANI, at(11, 0), at(8, 0));
        let folded = reconcile(&carla, &dani, at(8, 0)).unwrap();
        assert_eq!(booking(&folded, "carla")[0]["status"], "pending");
        let d = booking(&folded, "dani")[0];
        assert_eq!(d["status"], "pending");
        assert_eq!(
            open_events(&folded, d["id"].as_str().unwrap()),
            ["book_request"]
        );
        assert_eq!(pending(&folded), 2);
    }

    #[test]
    fn a_stale_write_without_new_bookings_leaves_the_earlier_one_standing() {
        let base = open_schedule();
        let carla = book(&base, &CARLA, at(10, 0), at(8, 0));
        let renamed = call(
            &base,
            &AGENT,
            "schedule_configure",
            json!({"business": "Outro nome"}),
            at(8, 0),
        );
        assert_eq!(reconcile(&carla, &renamed, at(8, 0)), Some(carla));
    }

    #[test]
    fn changing_hours_flags_bookings_left_outside_and_the_agent_can_resolve_them() {
        let st = open_schedule();
        let st = book(&st, &CARLA, at(17, 0), at(8, 0));
        let st = book(&st, &DANI, at(10, 0), at(8, 0));
        let late = booking(&st, "carla")[0]["id"].as_str().unwrap().to_string();
        let early = booking(&st, "dani")[0]["id"].as_str().unwrap().to_string();

        // The shop now closes at 15:00: Carla's 17:00 is flagged, Dani's 10:00 is not.
        let hours = json!({"weeklyHours": {"1": [["09:00", "15:00"]]}});
        let applied = apply("schedule_configure", &hours, st.clone(), &AGENT, at(8, 5)).unwrap();
        assert!(
            applied.comment.as_deref().unwrap_or("").contains("17:00"),
            "{:?}",
            applied.comment
        );
        let st = applied.state;
        assert_eq!(booking(&st, "carla")[0]["outsideHours"], true);
        assert!(booking(&st, "dani")[0].get("outsideHours").is_none());
        assert!(open_events(&st, &late).contains(&"outside_hours".to_string()));
        assert!(!open_events(&st, &early).contains(&"outside_hours".to_string()));

        // Changing other settings doesn't flag it twice.
        let again = call(
            &st,
            &AGENT,
            "schedule_configure",
            json!({"business": "Barbearia 2"}),
            at(8, 6),
        );
        let n = again["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "outside_hours")
            .count();
        assert_eq!(n, 1);

        // Moving it inside the hours closes the flag.
        let moved = call(
            &st,
            &AGENT,
            "schedule_resolve",
            json!({"bookingId": late, "action": "adjust", "startMs": at(14, 0)}),
            at(8, 7),
        );
        assert!(open_events(&moved, &late).is_empty());
        assert!(booking(&moved, "carla")[0].get("outsideHours").is_none());

        // Or the owner keeps it on purpose; later hour changes leave it alone.
        let kept = call(
            &st,
            &AGENT,
            "schedule_resolve",
            json!({"bookingId": late, "action": "keep"}),
            at(8, 7),
        );
        assert!(open_events(&kept, &late).is_empty());
        let later = call(
            &kept,
            &AGENT,
            "schedule_configure",
            json!({"weeklyHours": {"1": [["09:00", "14:00"]]}}),
            at(8, 8),
        );
        assert!(open_events(&later, &late).is_empty());

        // Opening late again clears a flag nobody answered.
        let reopened = call(
            &st,
            &AGENT,
            "schedule_configure",
            json!({"weeklyHours": {"1": [["09:00", "18:00"]]}}),
            at(8, 9),
        );
        assert!(open_events(&reopened, &late).is_empty());
        assert!(booking(&reopened, "carla")[0].get("outsideHours").is_none());
    }

    #[test]
    fn rescheduling_closes_the_open_cancel_request() {
        let st = open_schedule();
        let st = book(&st, &CARLA, at(10, 0), at(8, 0));
        let id = booking(&st, "carla")[0]["id"].as_str().unwrap().to_string();
        let st = call(
            &st,
            &AGENT,
            "schedule_resolve",
            json!({"bookingId": id, "action": "confirm"}),
            at(8, 1),
        );
        let st = call(
            &st,
            &CARLA,
            "schedule_cancel",
            json!({"bookingId": id}),
            at(8, 2),
        );
        assert_eq!(open_events(&st, &id), ["cancel_request"]);

        // The agent offers a new time instead: the cancel request is answered by it.
        let st = call(
            &st,
            &AGENT,
            "schedule_resolve",
            json!({"bookingId": id, "action": "adjust", "startMs": at(11, 0)}),
            at(8, 3),
        );
        assert!(open_events(&st, &id).is_empty());
        let ev = st["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "cancel_request")
            .unwrap();
        assert_eq!(ev["resolution"], "adjusted");
        assert_eq!(booking(&st, "carla")[0]["status"], "confirmed");
        // And Carla can ask to cancel the new time again.
        let st = call(
            &st,
            &CARLA,
            "schedule_cancel",
            json!({"bookingId": id}),
            at(8, 4),
        );
        assert_eq!(open_events(&st, &id), ["cancel_request"]);
    }
}

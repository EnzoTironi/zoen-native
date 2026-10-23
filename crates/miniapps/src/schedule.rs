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

mod availability;
use availability::{busy, check_slot, flag_outside_hours, is_for, occupies, when_label};
mod projection;
pub(crate) use projection::booking_change;
pub use projection::{accent, pending, view_for};
mod bookings;
pub use bookings::reconcile;
use bookings::{booking_index, has_open, next_id, note, push_event, resolve_events, trim};
mod execution;
pub use execution::{apply, replayed};

#[cfg(test)]
mod tests;

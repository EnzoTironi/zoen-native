//! Shared fixture operations preserve the original clients, identities and stores.

use super::*;

pub(super) fn seeded() -> Arc<Client> {
    let e = Client::open(":memory:".into(), "pt-BR".into()).unwrap();
    assert!(e.seed_demo_if_empty().unwrap());
    assert!(!e.seed_demo_if_empty().unwrap(), "seed é idempotente");
    e
}

pub(super) fn find_space(e: &Client, title: &str) -> SpaceSummary {
    e.spaces()
        .into_iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| panic!("sem Espaço {title}"))
}

pub(super) fn agent(e: &Client, name: &str) -> Persona {
    e.personas().into_iter().find(|p| p.name == name).unwrap()
}

pub(super) fn paraty_plan(e: &Client) -> ItemDetail {
    let items = e.items();
    items
        .into_iter()
        .find(|i| i.title == "Fim de semana em Paraty")
        .unwrap()
}

pub(super) fn zoen(e: &Client) -> Persona {
    agent(e, "Zoen")
}

pub(super) fn plan_line_id(plan: &ItemDetail) -> String {
    match &plan.plan {
        Some(p) => p.sections[0].lines[0].id.clone(),
        None => panic!("not a plan"),
    }
}

/// `hh:mm` São Paulo time on the next Monday–Saturday at least two days from now.
pub(super) fn sp_slot(hh: u32, mm: u32) -> i64 {
    use chrono::{Datelike, TimeZone};
    let tz = chrono_tz::America::Sao_Paulo;
    let mut day = chrono::DateTime::from_timestamp_millis(crate::engine::now_ms())
        .unwrap()
        .with_timezone(&tz)
        .date_naive()
        + chrono::Days::new(2);
    // Weekdays only: Saturday closes at 13:00, so afternoon slots would fail on Thursdays.
    while matches!(day.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) {
        day = day + chrono::Days::new(1);
    }
    tz.from_local_datetime(&day.and_hms_opt(hh, mm, 0).unwrap())
        .single()
        .unwrap()
        .timestamp_millis()
}

pub(super) fn schedule_item(e: &Client) -> (String, Persona) {
    let crew = find_space(e, "Turma do Sábado");
    let zoen = zoen(e);
    e.agent_create_app(
        crew.id.clone(),
        zoen.id.clone(),
        "start_schedule".into(),
        SCHEDULE_ARGS.into(),
        "test".into(),
        "o João quer uma agenda".into(),
    )
    .unwrap();
    let item = e
        .items()
        .into_iter()
        .find(|i| i.app.as_ref().is_some_and(|a| a.app_id == "schedule"))
        .expect("schedule");
    (item.id, zoen)
}

/// (structured view the caller received, whether the call was refused)
pub(super) fn sched_call(
    e: &Client,
    item: &str,
    who: &str,
    tool: &str,
    args: String,
) -> (serde_json::Value, bool) {
    let out = if who == "me" {
        e.app_call_tool(item.into(), tool.into(), args, false)
    } else {
        e.demo_member_app_call(item.into(), who.into(), tool.into(), args)
    }
    .unwrap();
    let r: serde_json::Value = serde_json::from_str(&out.result_json).unwrap();
    (
        r["structuredContent"].clone(),
        r["isError"].as_bool().unwrap_or(false),
    )
}

pub(super) fn book(
    e: &Client,
    item: &str,
    who: &str,
    svc: &str,
    start: i64,
) -> (serde_json::Value, bool) {
    sched_call(
        e,
        item,
        who,
        "schedule_book",
        format!(r#"{{"serviceId":"{svc}","startMs":{start},"note":"alergia a lâmina"}}"#),
    )
}

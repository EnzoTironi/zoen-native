//! Complete schedule scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn schedule_rejects_double_booking_outside_hours_and_past() {
    let e = seeded();
    let (item, zoen) = schedule_item(&e);
    let start = sp_slot(14, 0);
    assert!(!book(&e, &item, "marina", "corte-barba", start).1);
    // Same slot, and an overlapping one, are refused for anyone, pending or confirmed.
    assert!(book(&e, &item, "lucas", "corte", start).1);
    assert!(book(&e, &item, "lucas", "corte", start + 30 * 60_000).1);
    assert!(
        !book(&e, &item, "lucas", "corte", start + 60 * 60_000).1,
        "right after is free"
    );
    // Outside hours, lunch, off-grid, past, beyond the horizon, or a wrong length.
    assert!(book(&e, &item, "ana", "corte", sp_slot(20, 0)).1);
    assert!(book(&e, &item, "ana", "corte", sp_slot(11, 45)).1);
    assert!(
        book(&e, &item, "ana", "corte-barba", sp_slot(11, 30)).1,
        "would run into lunch"
    );
    assert!(
        book(&e, &item, "ana", "corte", sp_slot(9, 10)).1,
        "off the slot grid"
    );
    assert!(book(&e, &item, "ana", "corte", start - 7 * 86_400_000).1);
    assert!(book(&e, &item, "ana", "corte", start + 90 * 86_400_000).1);
    assert!(book(&e, &item, "ana", "nope", sp_slot(9, 0)).1);
    let wrong = sp_slot(9, 0);
    assert!(
        sched_call(
            &e,
            &item,
            "ana",
            "schedule_book",
            format!(
                r#"{{"serviceId":"corte","startMs":{wrong},"endMs":{}}}"#,
                wrong + 5 * 60_000
            )
        )
        .1
    );
    // The agent can't move or confirm into a taken slot either.
    let (v, _) = sched_call(&e, &item, "me", "schedule_get_state", "{}".into());
    let lucas_booking = v["bookings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["startMs"] == start + 60 * 60_000)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let adjust =
        format!(r#"{{"bookingId":"{lucas_booking}","action":"adjust","startMs":{start}}}"#);
    assert!(sched_call(&e, &item, &zoen.handle, "schedule_resolve", adjust).1);
    let free = sp_slot(16, 0);
    let adjust = format!(r#"{{"bookingId":"{lucas_booking}","action":"adjust","startMs":{free}}}"#);
    let (v, err) = sched_call(&e, &item, &zoen.handle, "schedule_resolve", adjust);
    assert!(!err, "{v}");
    let moved = v["bookings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == lucas_booking.as_str())
        .unwrap();
    assert_eq!(
        (moved["startMs"].as_i64(), moved["status"].as_str()),
        (Some(free), Some("confirmed"))
    );
}

pub(super) fn schedule_caps_spam_and_only_owner_or_agent_configure() {
    let e = seeded();
    let (item, zoen) = schedule_item(&e);
    assert!(!book(&e, &item, "marina", "corte", sp_slot(9, 0)).1);
    assert!(!book(&e, &item, "marina", "corte", sp_slot(9, 30)).1);
    let (v, err) = book(&e, &item, "marina", "corte", sp_slot(10, 0));
    assert!(err, "a third open booking is refused: {v}");
    // A withdrawn request frees the spot.
    let (v, _) = sched_call(&e, &item, "marina", "schedule_get_state", "{}".into());
    let first = v["bookings"][0]["id"].as_str().unwrap().to_string();
    let (v, err) = sched_call(
        &e,
        &item,
        "marina",
        "schedule_cancel",
        format!(r#"{{"bookingId":"{first}"}}"#),
    );
    assert!(!err, "{v}");
    assert_eq!(v["bookings"][0]["status"], "cancelled");
    assert_eq!(v["events"][0]["resolution"], "withdrawn");
    assert!(!book(&e, &item, "marina", "corte", sp_slot(10, 0)).1);

    // Clients can't configure; the agent can, with valid input only.
    let hours = r#"{"weeklyHours":{"1":[["08:00","12:00"]]}}"#;
    assert!(sched_call(&e, &item, "lucas", "schedule_configure", hours.into()).1);
    for bad in [
        r#"{"weeklyHours":{"1":[["8:00","12:00"]]}}"#,
        r#"{"weeklyHours":{"1":[["12:00","08:00"]]}}"#,
        r#"{"weeklyHours":{"1":[["08:00","12:00"],["11:00","13:00"]]}}"#,
        r#"{"weeklyHours":{"mon":[["08:00","12:00"]]}}"#,
        r#"{"weeklyHours":[]}"#,
        r#"{"services":[]}"#,
        r#"{"services":[{"name":"Corte"}]}"#,
        r#"{"services":[{"name":"","minutes":30}]}"#,
        r#"{"slotMinutes":2}"#,
        r#"{"timezone":"Mars/Olympus"}"#,
        r#"{"accentHex":"red"}"#,
    ] {
        let (v, err) = sched_call(&e, &item, &zoen.handle, "schedule_configure", bad.into());
        assert!(err, "{bad} should be refused: {v}");
    }
    assert!(
        sched_call(
            &e,
            &item,
            &zoen.handle,
            "schedule_configure",
            r#"{"resolvers":["x"]}"#.into()
        )
        .1,
        "only the owner names resolvers"
    );
    let (v, err) = sched_call(&e, &item, &zoen.handle, "schedule_configure", r##"{"business":"Studio Lia","accentHex":"#aa3355","services":[{"name":"Pilates","minutes":50,"priceCents":9000}]}"##.into());
    assert!(!err, "{v}");
    assert_eq!(
        (v["business"].as_str(), v["accentHex"].as_str()),
        (Some("Studio Lia"), Some("#AA3355"))
    );
    assert_eq!(v["services"][0]["id"], "pilates");
    // The owner names a resolver, who can then resolve.
    let (v, _) = sched_call(&e, &item, "lucas", "schedule_get_state", "{}".into());
    let lucas_id = v["viewer"]["id"].as_str().unwrap().to_string();
    assert!(
        !sched_call(
            &e,
            &item,
            "me",
            "schedule_configure",
            format!(r#"{{"resolvers":["{lucas_id}"]}}"#)
        )
        .1
    );
    let (v, _) = sched_call(&e, &item, "lucas", "schedule_get_state", "{}".into());
    assert_eq!(v["viewer"]["role"], "resolver");
    let pending = v["bookings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["status"] == "pending")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        !sched_call(
            &e,
            &item,
            "lucas",
            "schedule_resolve",
            format!(r#"{{"bookingId":"{pending}","action":"confirm"}}"#)
        )
        .1
    );
}

pub(super) fn start_schedule_refuses_bad_input_instead_of_defaults() {
    let e = seeded();
    let crew = find_space(&e, "Turma do Sábado");
    let zoen = zoen(&e);
    let bad = r#"{"business":"X","services":[{"name":"A","minutes":30}],"weeklyHours":{"1":[["9:00","12:00"]]}}"#;
    assert!(e
        .agent_create_app(
            crew.id,
            zoen.id,
            "start_schedule".into(),
            bad.into(),
            "test".into(),
            "agenda".into()
        )
        .is_err());
}

pub(super) fn the_agent_books_only_for_the_member_it_is_serving() {
    let e = seeded();
    let (item, zoen) = schedule_item(&e);
    let ana = e
        .personas()
        .into_iter()
        .find(|p| p.handle == "ana")
        .unwrap();
    let lucas = e
        .personas()
        .into_iter()
        .find(|p| p.handle == "lucas")
        .unwrap();
    let call = |serving: Option<&str>, for_id: &str, start: i64| {
        let args = format!(
            r#"{{"serviceId":"corte","startMs":{start},"forId":"{for_id}","forName":"Anything"}}"#
        );
        let out = e
            .lock()
            .app_call_as(&item, &zoen.id, "schedule_book", &args, false, serving)
            .unwrap();
        let r: serde_json::Value = serde_json::from_str(&out.result_json).unwrap();
        r["isError"].as_bool().unwrap_or(false)
    };
    // Lucas talks the agent into booking under Ana's name: refused.
    assert!(call(Some(&lucas.id), &ana.id, sp_slot(9, 0)));
    // With nobody being served, the agent names no client at all.
    assert!(call(None, &ana.id, sp_slot(9, 0)));
    // Serving Ana, it books for Ana.
    assert!(!call(Some(&ana.id), &ana.id, sp_slot(9, 0)));
    // Human staff still name any client.
    let (_, err) = sched_call(
        &e,
        &item,
        "me",
        "schedule_book",
        format!(
            r#"{{"serviceId":"corte","startMs":{},"forId":"{}","forName":"Lucas"}}"#,
            sp_slot(10, 0),
            lucas.id
        ),
    );
    assert!(!err);
    let (v, _) = sched_call(&e, &item, "me", "schedule_get_state", "{}".into());
    let fors: Vec<&str> = v["bookings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["forId"].as_str().unwrap())
        .collect();
    assert_eq!(fors, vec![ana.id.as_str(), lucas.id.as_str()]);
    // The agent's booking carries Ana's profile name, not the "Ana" the call passed.
    assert_eq!(v["bookings"][0]["forName"], ana.name.as_str());
    // Serving Ana, the agent can't do what Ana can't: confirming is staff's call.
    let bid = v["bookings"][0]["id"].as_str().unwrap().to_string();
    let confirm = format!(r#"{{"bookingId":"{bid}","action":"confirm"}}"#);
    let out = e
        .lock()
        .app_call_as(
            &item,
            &zoen.id,
            "schedule_resolve",
            &confirm,
            false,
            Some(&ana.id),
        )
        .unwrap();
    assert!(out.message.contains("resolve"), "{}", out.result_json);
}

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
fn an_agent_booking_is_found_again_by_its_source_message() {
    let base = open_schedule();
    let args = json!({"serviceId": "corte", "startMs": at(10, 0),
            "forId": "carla", "forName": "Carla", "ref": "m1"});
    assert!(replayed("schedule_book", &args, &base).is_none());
    let st = call(&base, &AGENT, "schedule_book", args.clone(), at(8, 0));
    assert!(replayed("schedule_book", &args, &st).is_some());
    // Another message, another client or another slot is a new request.
    let other = |k: &str, v: Value| {
        let mut a = args.clone();
        a[k] = v;
        replayed("schedule_book", &a, &st)
    };
    assert!(other("ref", json!("m2")).is_none());
    assert!(other("forId", json!("dani")).is_none());
    assert!(other("startMs", json!(at(11, 0))).is_none());
    // Only the agent's stamp counts.
    let st = call(&base, &CARLA, "schedule_book", args.clone(), at(8, 0));
    assert!(replayed("schedule_book", &args, &st).is_none());
}

#[test]
fn the_agent_books_for_a_client_and_the_owner_note_names_them() {
    let base = open_schedule();
    let args = json!({"serviceId": "corte", "startMs": at(10, 0),
            "forId": "carla", "forName": "Carla"});
    let st = call(&base, &AGENT, "schedule_book", args.clone(), at(8, 0));
    let b = booking(&st, "bot")[0];
    assert_eq!(
        (&b["forId"], &b["forName"]),
        (&json!("carla"), &json!("Carla"))
    );
    let note = booking_change(&base, &st, "bot").expect("agent booking notifies");
    assert_eq!(note["booking"]["client"], "Carla");
    // A client can't book in someone else's name.
    let st = call(&base, &DANI, "schedule_book", args, at(8, 0));
    assert_eq!(booking(&st, "dani")[0]["forName"], "Dani");
    assert_eq!(
        booking_change(&base, &st, "dani").unwrap()["booking"]["client"],
        "Dani"
    );
}

#[test]
fn a_client_sees_and_cancels_what_the_agent_booked_for_them() {
    let base = open_schedule();
    let args = json!({"serviceId": "corte", "startMs": at(10, 0),
            "forId": "carla", "forName": "Carla"});
    let st = call(&base, &AGENT, "schedule_book", args, at(8, 0));
    let st = book(&st, &DANI, at(11, 0), at(8, 0));
    let id = booking(&st, "bot")[0]["id"].clone();
    // Carla sees hers with its events; Dani's stays busy-only, for both clients.
    let v = view_for(&st, "carla", at(8, 0));
    let ids: Vec<&Value> = v["bookings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| &b["id"])
        .collect();
    assert_eq!(ids, vec![&id]);
    assert!(v["events"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["bookingId"] == id));
    assert_eq!(v["busy"].as_array().unwrap().len(), 2);
    let d = view_for(&st, "dani", at(8, 0));
    assert!(d["bookings"]
        .as_array()
        .unwrap()
        .iter()
        .all(|b| b["byId"] == "dani"));
    // Dani can't cancel it; Carla can, and the owner hears it from her.
    let cancel = json!({"bookingId": id});
    assert!(apply("schedule_cancel", &cancel, st.clone(), &DANI, at(9, 0)).is_err());
    let done = call(&st, &CARLA, "schedule_cancel", cancel, at(9, 0));
    assert_eq!(booking(&done, "bot")[0]["status"], "cancelled");
    assert!(booking_change(&st, &done, "carla").is_some());
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
        applied.text.contains("17:00") && applied.comment.is_none(),
        "{:?} {:?}",
        applied.text,
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
    // Keeping answers the hours flag only; the booking request itself is still the
    // agent's to confirm.
    assert_eq!(open_events(&kept, &late), vec!["book_request".to_string()]);
    let later = call(
        &kept,
        &AGENT,
        "schedule_configure",
        json!({"weeklyHours": {"1": [["09:00", "14:00"]]}}),
        at(8, 8),
    );
    assert!(!open_events(&later, &late).contains(&"outside_hours".to_string()));

    // Opening late again clears a flag nobody answered.
    let reopened = call(
        &st,
        &AGENT,
        "schedule_configure",
        json!({"weeklyHours": {"1": [["09:00", "18:00"]]}}),
        at(8, 9),
    );
    assert!(!open_events(&reopened, &late).contains(&"outside_hours".to_string()));
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

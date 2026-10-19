use protocol::{ClientFrame, Cursor};
use relay_admission::sync::{cursor_read, include_current, plan_json, SyncRead};
use serde_json::{json, Value};

fn cursor(space: &str, next_seq: u64) -> Cursor {
    Cursor {
        space: space.into(),
        next_seq,
    }
}

fn context() -> Value {
    json!({
        "session": {"authenticated_identity":"11".repeat(32),"authenticated_device":"22".repeat(32)},
        "facts": {"identity":"11".repeat(32),"current_spaces":[],"removed_at":{}}
    })
}

fn plan(cursors: Vec<Cursor>, all: bool, context: &Value) -> Value {
    serde_json::from_str(
        &plan_json(
            &ClientFrame::Sync { cursors, all }.encode(),
            &context.to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn removed_member_can_receive_the_removal_once_but_no_later_history() {
    let mut facts = context();
    facts["facts"]["current_spaces"] = json!(["current"]);
    facts["facts"]["removed_at"] = json!({"removed":"8", "removed_at_genesis":"0"});
    assert_eq!(
        plan(
            vec![
                cursor("unknown", 0),
                cursor("removed", 7),
                cursor("removed", 8),
                cursor("removed", 9),
                cursor("removed_at_genesis", 0),
                cursor("removed_at_genesis", 1),
                cursor("current", 100),
            ],
            false,
            &facts,
        ),
        json!([
            {"space":"removed","from":"7","through":"8"},
            {"space":"removed","from":"8","through":"8"},
            {"space":"removed_at_genesis","from":"0","through":"0"},
            {"space":"current","from":"100","through":null},
        ])
    );
}

#[test]
fn all_includes_only_current_spaces_and_preserves_requested_order_and_repeats() {
    let mut facts = context();
    facts["facts"]["current_spaces"] = json!(["a", "b", "c", "a"]);
    facts["facts"]["removed_at"] = json!({"old":"5", "not_requested":"2"});
    let requested = vec![cursor("b", 9), cursor("old", 5), cursor("b", 3)];
    let explicit = json!([
        {"space":"b","from":"9","through":null},
        {"space":"old","from":"5","through":"5"},
        {"space":"b","from":"3","through":null},
    ]);
    assert_eq!(plan(requested.clone(), false, &facts), explicit);
    assert_eq!(
        plan(requested, true, &facts),
        json!([
            {"space":"b","from":"9","through":null},
            {"space":"old","from":"5","through":"5"},
            {"space":"b","from":"3","through":null},
            {"space":"a","from":"0","through":null},
            {"space":"c","from":"0","through":null},
        ])
    );
}

#[test]
fn rejoined_member_reads_current_history_instead_of_an_old_removal_boundary() {
    let mut facts = context();
    facts["facts"]["current_spaces"] = json!(["rejoined"]);
    facts["facts"]["removed_at"] = json!({"rejoined":"8"});
    assert_eq!(
        plan(vec![cursor("rejoined", 20)], false, &facts),
        json!([{"space":"rejoined","from":"20","through":null}])
    );
}

#[test]
fn portable_plan_preserves_u64_values_above_javascript_integer_precision() {
    let mut facts = context();
    facts["facts"]["current_spaces"] = json!(["current"]);
    facts["facts"]["removed_at"] = json!({"removed":u64::MAX.to_string()});
    assert_eq!(
        plan(
            vec![
                cursor("current", 9_007_199_254_740_993),
                cursor("removed", u64::MAX)
            ],
            false,
            &facts,
        ),
        json!([
            {"space":"current","from":"9007199254740993","through":null},
            {"space":"removed","from":"18446744073709551615","through":"18446744073709551615"},
        ])
    );
    for invalid in ["01", "-1", "1.0", "18446744073709551616", ""] {
        facts["facts"]["removed_at"] = json!({"removed":invalid});
        let failure = plan_json(
            &ClientFrame::Sync {
                cursors: vec![cursor("removed", 0)],
                all: false,
            }
            .encode(),
            &facts.to_string(),
        )
        .unwrap_err();
        assert_eq!(failure.code, "invalid_integer");
        assert!(!failure.permanent);
    }
}

#[test]
fn facts_must_belong_to_the_authenticated_identity_and_use_the_exact_schema() {
    let frame = ClientFrame::Sync {
        cursors: vec![],
        all: true,
    }
    .encode();
    let mut facts = context();
    facts["facts"]["identity"] = json!("33".repeat(32));
    let failure = plan_json(&frame, &facts.to_string()).unwrap_err();
    assert_eq!(failure.code, "invalid_sync_context");
    assert!(!failure.permanent);
    for (field, value) in [
        ("authenticated_identity", "not-an-identity"),
        ("authenticated_device", "not-a-device"),
    ] {
        let mut facts = context();
        facts["session"][field] = json!(value);
        assert_eq!(
            plan_json(&frame, &facts.to_string()).unwrap_err().code,
            "invalid_sync_context"
        );
    }
    let mut facts = context();
    facts["facts"]["client_membership"] = json!(true);
    assert_eq!(
        plan_json(&frame, &facts.to_string()).unwrap_err().code,
        "invalid_sync_context"
    );
    assert_eq!(
        plan_json(&frame, "{}").unwrap_err().code,
        "invalid_sync_context"
    );
    assert_eq!(
        plan_json(&frame, &" ".repeat(1024 * 1024 + 1))
            .unwrap_err()
            .code,
        "invalid_sync_context"
    );
}

#[test]
fn only_bounded_canonical_sync_frames_reach_planning() {
    let facts = context().to_string();
    for frame in [
        vec![],
        vec![0xff],
        vec![0; 1024 * 1024 + 1],
        ClientFrame::Ping.encode(),
    ] {
        let failure = plan_json(&frame, &facts).unwrap_err();
        assert_eq!(failure.code, "sync");
        assert!(failure.permanent);
    }
    let mut frame = ClientFrame::Sync {
        cursors: vec![],
        all: true,
    }
    .encode();
    // Valid protobuf with an unknown field would otherwise be silently ignored.
    frame.extend([0xf8, 0x07, 0x01]);
    assert_eq!(
        plan_json(&frame, &facts).unwrap_err().reason,
        "sync frame is not canonical"
    );
}

#[test]
fn native_and_portable_plans_agree_for_each_access_state_and_boundary() {
    for member in [false, true] {
        for removed_at in [None, Some(0), Some(8), Some(u64::MAX)] {
            for next in [0, 7, 8, 9, u64::MAX] {
                for all in [false, true] {
                    let current = if member { vec!["space".into()] } else { vec![] };
                    let mut native = cursor_read(cursor("space", next), member, removed_at)
                        .into_iter()
                        .collect::<Vec<_>>();
                    include_current(&mut native, &current, all);
                    let mut facts = context();
                    facts["facts"]["current_spaces"] = json!(current);
                    if let Some(at) = removed_at {
                        facts["facts"]["removed_at"] = json!({"space":at.to_string()});
                    }
                    let portable = plan(vec![cursor("space", next)], all, &facts);
                    let expected: Vec<_> = native
                        .into_iter()
                        .map(|read| {
                            json!({
                                "space": read.space, "from": read.from.to_string(),
                                "through": read.through.map(|at| at.to_string()),
                            })
                        })
                        .collect();
                    assert_eq!(portable, json!(expected));
                }
            }
        }
    }
    assert_eq!(
        cursor_read(cursor("space", 8), false, Some(8)),
        Some(SyncRead {
            space: "space".into(),
            from: 8,
            through: Some(8),
        })
    );
}

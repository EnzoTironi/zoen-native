use super::*;
pub(super) fn open() -> (Session, HostBridge, Scope, Head, Vec<u8>) {
    let mut fresh = Session::new();
    fresh.new_workbook();
    let bytes = io::save_bytes(&fresh.doc().unwrap().wb, "Workbook.xlsx").unwrap();
    let scope = Scope {
        artifact_id: "grid-fixture".into(),
        actor_id: "actual-core-actor".into(),
        actor_kind: ActorKind::Human,
    };
    let head = Head {
        version: "1".into(),
        sha256: hash(&bytes),
    };
    let mut s = Session::new();
    let mut bridge = HostBridge::default();
    bridge
        .open(
            &mut s,
            Open {
                scope: scope.clone(),
                head: head.clone(),
            },
            &bytes,
        )
        .unwrap();
    (s, bridge, scope, head, bytes)
}
fn command(scope: &Scope, base: &Head, id: &str, cell: &str, input: &str) -> Command {
    Command {
        mutation: Mutation {
            scope: scope.clone(),
            base: base.clone(),
            mutation_id: id.into(),
        },
        action: Action::CellSet(CellSet {
            sheet: "Sheet1".into(),
            cell: cell.into(),
            input: input.into(),
        }),
    }
}
pub(super) fn accept(
    s: &mut Session,
    b: &mut HostBridge,
    scope: &Scope,
    id: &str,
    version: &str,
    p: &Proposal,
) -> Head {
    let head = Head {
        version: version.into(),
        sha256: hash(&p.bytes),
    };
    b.accept(
        s,
        Accept {
            scope: scope.clone(),
            mutation_id: id.into(),
            head: head.clone(),
        },
    )
    .unwrap();
    head
}
#[test]
fn actual_engine_manual_agent_calculation_undo_cas_and_exact_cold_xlsx_export() {
    let (mut s, mut b, scope, head, original) = open();
    assert_eq!(
        b.export(
            &s,
            Export {
                scope: scope.clone(),
                head: head.clone(),
                format: ExportFormat::Xlsx
            }
        )
        .unwrap()
        .bytes,
        original
    );
    // Same real cell command used by SheetApp::commit_edit; not a model rewrite.
    s.execute("cell.set", json!({"cell":"A1","input":"7"}))
        .unwrap();
    s.execute("cell.set", json!({"cell":"B1","input":"=A1*2"}))
        .unwrap();
    assert_eq!(
        s.execute("cell.get", json!({"cell":"B1"})).unwrap()["value"],
        14.0
    );
    let manual = Mutation {
        scope: scope.clone(),
        base: head,
        mutation_id: "human-1".into(),
    };
    let p = b.snapshot(&mut s, manual).unwrap();
    assert!(
        s.execute("cell.set", json!({"cell":"A1","input":"8"}))
            .is_err()
    );
    let head = accept(&mut s, &mut b, &scope, "human-1", "2", &p);
    let c = command(&scope, &head, "agent-1", "A1", "9");
    let p = b.execute(&mut s, c.clone()).unwrap();
    assert_eq!(
        s.execute("cell.get", json!({"cell":"B1"})).unwrap()["value"],
        18.0
    );
    assert_eq!(b.execute(&mut s, c.clone()).unwrap().bytes, p.bytes);
    let head = accept(&mut s, &mut b, &scope, "agent-1", "3", &p);
    assert_eq!(b.execute(&mut s, c).unwrap().bytes, p.bytes);
    let mut cold = Session::new();
    let mut cold_bridge = HostBridge::default();
    cold_bridge
        .open(
            &mut cold,
            Open {
                scope: scope.clone(),
                head: head.clone(),
            },
            &p.bytes,
        )
        .unwrap();
    assert_eq!(
        cold.execute("cell.get", json!({"cell":"B1"})).unwrap()["value"],
        18.0
    );
    assert_eq!(
        cold_bridge
            .export(
                &cold,
                Export {
                    scope: scope.clone(),
                    head,
                    format: ExportFormat::Xlsx
                }
            )
            .unwrap()
            .bytes,
        p.bytes
    );
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(
        s.execute("cell.get", json!({"cell":"B1"})).unwrap()["value"],
        14.0
    );
}
#[test]
fn pending_proposal_requires_exact_receipt_and_never_exports_unaccepted_draft() {
    let (mut s, mut b, scope, head, original) = open();
    let p = b
        .execute(&mut s, command(&scope, &head, "write-1", "A1", "5"))
        .unwrap();
    assert_eq!(
        b.export(
            &s,
            Export {
                scope: scope.clone(),
                head: head.clone(),
                format: ExportFormat::Xlsx
            }
        )
        .unwrap()
        .bytes,
        original
    );
    for bad in [
        Head {
            version: "2".into(),
            sha256: head.sha256.clone(),
        },
        Head {
            version: "3".into(),
            sha256: hash(&p.bytes),
        },
    ] {
        assert!(
            b.accept(
                &mut s,
                Accept {
                    scope: scope.clone(),
                    mutation_id: "write-1".into(),
                    head: bad
                }
            )
            .is_err()
        );
        assert!(s.managed_frozen);
    }
    let next = accept(&mut s, &mut b, &scope, "write-1", "2", &p);
    assert!(!s.managed_frozen);
    assert_eq!(
        b.export(
            &s,
            Export {
                scope,
                head: next,
                format: ExportFormat::Xlsx
            }
        )
        .unwrap()
        .bytes,
        p.bytes
    );
}
#[test]
fn command_scope_capacity_external_io_and_invalid_formula_never_mutate_workbook() {
    let (mut s, mut b, scope, head, _) = open();
    let before = s.doc().unwrap().wb.clone();
    for (cell, input) in [
        ("IW1", "7"),
        ("A10001", "7"),
        ("A1", "=[other.xlsx]Sheet1!A1"),
        ("A1", "=WEBSERVICE(\"https://example.invalid\")"),
    ] {
        assert!(
            b.execute(&mut s, command(&scope, &head, "rejected", cell, input))
                .is_err()
        );
        assert!(Arc::ptr_eq(&before, &s.doc().unwrap().wb));
    }
    for id in [
        "file.open",
        "file.save",
        "file.new",
        "automate.run",
        "data.getData",
        "insert.hyperlink",
        "range.setValues",
    ] {
        assert!(s.execute(id, json!({})).is_err(), "{id}");
    }
    let mut foreign = scope.clone();
    foreign.artifact_id = "other".into();
    assert!(
        b.execute(&mut s, command(&foreign, &head, "foreign", "A1", "8"))
            .is_err()
    );
    assert!(Arc::ptr_eq(&before, &s.doc().unwrap().wb));
}

#[test]
fn paused_flush_accept_keeps_keyboard_frozen_and_resume_cannot_release_pending() {
    let (mut s, mut b, scope, head, _) = open();
    s.execute("cell.set", json!({"cell":"A1","input":"7"}))
        .unwrap();
    assert_eq!(b.freeze(&mut s, true).unwrap()["frozen"], true);
    let p = b
        .snapshot(
            &mut s,
            Mutation {
                scope: scope.clone(),
                base: head,
                mutation_id: "paused".into(),
            },
        )
        .unwrap();
    assert_eq!(b.freeze(&mut s, false).unwrap()["frozen"], true);
    b.freeze(&mut s, true).unwrap();
    accept(&mut s, &mut b, &scope, "paused", "2", &p);
    assert_eq!(b.status(&s)["frozen"], true);
    assert!(
        s.execute("cell.set", json!({"cell":"A2","input":"9"}))
            .is_err()
    );
    assert_eq!(b.freeze(&mut s, false).unwrap()["frozen"], false);
    s.execute("cell.set", json!({"cell":"A2","input":"9"}))
        .unwrap();
}

#[test]
fn canonical_open_never_adopts_or_overwrites_a_standalone_draft() {
    let (_, _, scope, head, bytes) = open();
    let mut standalone = Session::new();
    standalone.new_workbook();
    standalone
        .execute("cell.set", json!({"cell":"A1","input":"private draft"}))
        .unwrap();
    let before = standalone.doc().unwrap().wb.clone();
    let mut bridge = HostBridge::default();
    assert!(!bridge.is_open());
    assert_eq!(
        bridge
            .open(&mut standalone, Open { scope, head }, &bytes)
            .unwrap_err(),
        "unbound_session"
    );
    assert!(Arc::ptr_eq(&before, &standalone.doc().unwrap().wb));
    assert!(!bridge.is_open());
}

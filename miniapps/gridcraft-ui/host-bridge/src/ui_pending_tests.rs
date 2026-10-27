//! Actual Grid keyboard input while an immutable manual proposal awaits its receipt.
use super::*;

#[test]
fn delayed_manual_receipt_retains_real_keyboard_and_original_xlsx() -> Result<()> {
    use gridcraft_engine::core::{CellRef, Value};
    let (mut h, mut bridge, scope, head) = harness();
    text(&mut h, "7");
    key(&mut h, Key::Enter);
    let mutation = Mutation {
        scope: scope.clone(),
        base: head,
        mutation_id: "held-manual".into(),
    };
    let original = bridge.snapshot(&mut h.state_mut().session, mutation.clone())?;
    text(&mut h, "9");
    key(&mut h, Key::Enter);
    assert_eq!(
        value(&h, "A2"),
        Value::Number(9.0),
        "ordinary autosave must not discard real keyboard input during its receipt"
    );
    let retry = bridge.snapshot(&mut h.state_mut().session, mutation)?;
    assert_eq!(retry.bytes, original.bytes);
    assert_eq!(retry.metadata, original.metadata);
    let (captured, warnings) = io::open_bytes("Workbook.xlsx", &original.bytes).map_err(engine)?;
    assert!(warnings.is_empty());
    let first_sheet = captured
        .active()
        .ok_or_else(|| failure("fixture_sheet_missing"))?;
    assert_eq!(first_sheet.value(CellRef::new(0, 0)), Value::Number(7.0));
    assert_eq!(
        first_sheet.value(CellRef::new(1, 0)),
        Value::Empty,
        "the original XLSX cannot adopt newer input before its own ACK"
    );
    let accepted = crate::tests::accept(
        &mut h.state_mut().session,
        &mut bridge,
        &scope,
        "held-manual",
        "2",
        &original,
    );
    assert_eq!(value(&h, "A2"), Value::Number(9.0));
    assert!(bridge.dirty(&h.state().session));
    let latest = bridge.snapshot(
        &mut h.state_mut().session,
        Mutation {
            scope: scope.clone(),
            base: accepted,
            mutation_id: "later-manual".into(),
        },
    )?;
    assert!(
        bridge
            .accept(
                &mut h.state_mut().session,
                Accept {
                    scope: scope.clone(),
                    mutation_id: "held-manual".into(),
                    head: Head {
                        version: "2".into(),
                        sha256: hash(&original.bytes)
                    }
                }
            )
            .is_err()
    );
    let head = crate::tests::accept(
        &mut h.state_mut().session,
        &mut bridge,
        &scope,
        "later-manual",
        "3",
        &latest,
    );
    h.state_mut().run("edit.undo", json!({}))?;
    assert_eq!(value(&h, "A2"), Value::Empty);
    let exported = bridge.export(
        &h.state().session,
        Export {
            scope: scope.clone(),
            head: head.clone(),
            format: ExportFormat::Xlsx,
        },
    )?;
    assert_eq!(exported.bytes, latest.bytes);
    let mut cold = HostBridge::default();
    let mut session = Session::new();
    cold.open(&mut session, Open { scope, head }, &exported.bytes)?;
    let cold_doc = session.doc().map_err(engine)?;
    let sheet = cold_doc
        .wb
        .active()
        .ok_or_else(|| failure("fixture_sheet_missing"))?;
    assert_eq!(sheet.value(CellRef::new(0, 0)), Value::Number(7.0));
    assert_eq!(sheet.value(CellRef::new(1, 0)), Value::Number(9.0));
    Ok(())
}

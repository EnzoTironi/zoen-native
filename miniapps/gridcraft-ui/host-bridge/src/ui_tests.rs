//! Actual egui input path; distinct from the Session-only native contract tests.
use super::*;
use egui::{Event, Key, Modifiers};
use gridcraft_ui_egui::SheetApp;
type Harness = egui_kittest::Harness<'static, SheetApp>;
fn harness() -> (Harness, HostBridge, Scope, Head) {
    let (s, b, scope, head, _) = super::tests::open();
    let mut app = SheetApp::new(s, Default::default());
    app.chrome.title_bar = false;
    app.chrome.file_tab = false;
    let mut h = egui_kittest::Harness::builder()
        .with_size(egui::vec2(390.0, 844.0))
        .build_ui_state(
            |ui, app: &mut SheetApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            app,
        );
    h.run_steps(4);
    (h, b, scope, head)
}
fn text(h: &mut Harness, value: &str) {
    h.input_mut().events.push(Event::Text(value.into()));
    h.run_steps(2);
}
fn key(h: &mut Harness, key: Key) {
    h.input_mut()
        .events
        .extend([true, false].map(|pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        }));
    h.run_steps(2);
}
fn value(h: &Harness, cell: &str) -> gridcraft_engine::core::Value {
    h.state()
        .session
        .doc()
        .unwrap()
        .wb
        .active()
        .unwrap()
        .value(gridcraft_engine::core::CellRef::parse(cell).unwrap())
}
#[test]
fn actual_keyboard_enter_checkpoint_and_pending_receipt_freeze_at_phone_geometry() {
    let (mut h, mut b, scope, head) = harness();
    text(&mut h, "7");
    key(&mut h, Key::Enter);
    assert_eq!(value(&h, "A1"), gridcraft_engine::core::Value::Number(7.0));
    let p = b
        .snapshot(
            &mut h.state_mut().session,
            Mutation {
                scope: scope.clone(),
                base: head,
                mutation_id: "keyboard".into(),
            },
        )
        .unwrap();
    // Raw text, Enter and menu command cannot commit a second edit while CAS is unresolved.
    text(&mut h, "9");
    key(&mut h, Key::Enter);
    assert_eq!(value(&h, "A2"), gridcraft_engine::core::Value::Empty);
    assert!(
        h.state().editor.is_none(),
        "pending input must not create a later draft"
    );
    assert!(h
        .state_mut()
        .run("cell.set", json!({"cell":"A2","input":"9"}))
        .is_err());
    super::tests::accept(
        &mut h.state_mut().session,
        &mut b,
        &scope,
        "keyboard",
        "2",
        &p,
    );
    key(&mut h, Key::Escape);
    text(&mut h, "9");
    key(&mut h, Key::Enter);
    assert_eq!(value(&h, "A2"), gridcraft_engine::core::Value::Number(9.0));
}
#[test]
fn invalid_external_formula_stays_a_human_draft_and_never_becomes_saved() {
    let (mut h, mut b, scope, head) = harness();
    text(&mut h, "=WEBSERVICE(\"https://example.invalid\")");
    key(&mut h, Key::Enter);
    assert_eq!(value(&h, "A1"), gridcraft_engine::core::Value::Empty);
    assert!(
        h.state().editor.is_some(),
        "invalid draft must remain recoverable"
    );
    assert!(b
        .snapshot(
            &mut h.state_mut().session,
            Mutation {
                scope,
                base: head,
                mutation_id: "invalid".into()
            }
        )
        .is_err());
}

#[test]
fn native_share_is_accessible_host_intent_without_saving_or_exporting_its_draft() {
    use egui_kittest::kittest::Queryable;
    use std::{cell::Cell, rc::Rc};
    let (mut h, b, _, _) = harness();
    text(&mut h, "7");
    key(&mut h, Key::Enter);
    assert!(b.dirty(&h.state().session));
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    h.state_mut().services.host_export_intent = Some(Box::new(move |format| {
        assert_eq!(format, "xlsx");
        observed.set(observed.get() + 1);
        true
    }));
    h.run_steps(2);
    h.get_by_label("Share").click();
    h.run_steps(2);
    assert_eq!(calls.get(), 1);
    assert!(b.dirty(&h.state().session));
    h.state_mut().run("file.save", json!({})).unwrap();
    assert_eq!(calls.get(), 2);
    assert!(b.dirty(&h.state().session));
    h.state_mut().services.host_export_intent = Some(Box::new(|_| false));
    assert!(h.state_mut().run("file.save", json!({})).is_err());
    assert!(b.dirty(&h.state().session));
}

#[test]
fn actual_refused_column_drag_preserves_two_accepted_cell_undo_steps() {
    let (mut h, mut b, scope, mut head) = harness();
    for (input, version) in [("7", "2"), ("9", "3")] {
        text(&mut h, input);
        key(&mut h, Key::Enter);
        let mutation_id = format!("edit-{version}");
        let proposal = b
            .snapshot(
                &mut h.state_mut().session,
                Mutation {
                    scope: scope.clone(),
                    base: head,
                    mutation_id: mutation_id.clone(),
                },
            )
            .expect("actual UI proposal");
        head = super::tests::accept(
            &mut h.state_mut().session,
            &mut b,
            &scope,
            &mutation_id,
            version,
            &proposal,
        );
    }
    assert_eq!(value(&h, "A1"), gridcraft_engine::core::Value::Number(7.0));
    assert_eq!(value(&h, "A2"), gridcraft_engine::core::Value::Number(9.0));
    let before = h.state().session.doc().expect("document").undo.len();
    assert_eq!(
        before, 2,
        "two distinct real edits retain separate undo entries"
    );
    let geometry = h.state().grid.geo.clone().expect("rendered geometry");
    let workbook = h.state().session.doc().expect("document").wb.clone();
    let sheet = workbook.active().expect("actual sheet");
    let original_width = sheet.col_width(0);
    let rect = geometry.screen_cell_rect(sheet, gridcraft_engine::core::CellRef::new(0, 0));
    let start = egui::pos2(rect.right(), geometry.cells.top() - geometry.header_h * 0.5);
    h.input_mut().events.extend([
        Event::PointerMoved(start),
        Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(1);
    assert!(
        matches!(
            h.state().grid.drag,
            gridcraft_ui_egui::grid::Drag::ResizeCol { col: 0, .. }
        ),
        "actual header pointer selected the column resize path"
    );
    let end = start + egui::vec2(24.0, 0.0);
    h.input_mut().events.push(Event::PointerMoved(end));
    h.run_steps(1);
    h.input_mut().events.push(Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    let document = h.state().session.doc().expect("document");
    assert_eq!(
        document.wb.active().expect("sheet").col_width(0),
        original_width,
        "profile still refuses the unsupported column mutation"
    );
    assert_eq!(
        document.undo.len(),
        before,
        "a refused drag must not coalesce already accepted cell history"
    );
    h.state_mut()
        .run("edit.undo", json!({}))
        .expect("normal supported undo");
    assert_eq!(value(&h, "A1"), gridcraft_engine::core::Value::Number(7.0));
    assert_eq!(value(&h, "A2"), gridcraft_engine::core::Value::Empty);
    assert_eq!(
        head.version, "3",
        "local undo does not invent an accepted version"
    );
}

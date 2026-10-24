//! The actual Grid UI creates a TextEdit from a label/cell trigger on release.
use super::*;
use egui::PointerButton;
#[path = "../../../vendor/eframe-0.36.2/src/web/text_input_activation.rs"]
mod web_activation;

fn touch(h: &mut Harness, at: egui::Pos2, pressed: bool) {
    h.input_mut().time = Some(h.ctx.input(|input| input.time) + 0.1);
    h.input_mut().events.push(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    if !pressed {
        h.input_mut().events.push(Event::PointerGone);
    }
    h.input_mut().events.push(Event::Touch {
        device_id: egui::TouchDeviceId(0),
        id: egui::TouchId::from(17),
        phase: if pressed { egui::TouchPhase::Start } else { egui::TouchPhase::End },
        pos: at,
        force: None,
    });
    web_activation::clear_custom(&h.ctx);
    h.step();
}
fn tap(h: &mut Harness, at: egui::Pos2) {
    touch(h, at, true);
    touch(h, at, false);
}
fn admitted(h: &Harness) -> bool {
    web_activation::accepted_output_text(&h.ctx, h.output().platform_output.ime.as_ref())
}

fn formula(h: &Harness) -> egui::Pos2 {
    let id = egui::Id::new("gridcraft.formula_bar");
    h.ctx.read_response(if h.state().editor.is_some() { id } else { id.with("click") }).unwrap().rect.center()
}
fn cell(h: &Harness, at: &str) -> egui::Pos2 {
    let app = h.state();
    let sh = app.session.doc().unwrap().wb.active().unwrap();
    let geo = gridcraft_ui_egui::grid::Geo::new(sh, app.grid.rect.unwrap(), egui::Vec2::ZERO);
    geo.screen_cell_rect(sh, gridcraft_engine::core::CellRef::parse(at).unwrap()).center()
}
fn tap_formula(h: &mut Harness) {
    let at = formula(h);
    tap(h, at);
}

#[test]
fn first_formula_and_double_cell_touch_publish_current_owner_and_save_the_real_values() {
    let (mut h, mut bridge, scope, head) = harness();
    tap_formula(&mut h);
    assert!(h.state().editor.as_ref().is_some_and(|ed| ed.from_formula_bar));
    assert!(
        h.output().platform_output.ime.is_some(),
        "first release current IME: focus={:?} text={} formula-response={:?} pulse={:?}",
        h.ctx.memory(|m| m.focused()),
        h.ctx.text_edit_focused(),
        h.ctx.read_response(egui::Id::new("gridcraft.formula_bar")),
        h.ctx.data(|d| d.get_temp::<Option<(u64, egui::Id)>>(egui::Id::new("eframe.text_input_activation")))
    );
    let owner = h.ctx.memory(|m| m.focused());
    assert!(admitted(&h), "the real label trigger admits its current formula TextEdit");
    assert_eq!(h.ctx.memory(|m| m.focused()), owner, "observing admission cannot surrender the admitted owner");
    assert!(!admitted(&h), "a consumed trigger cannot authorize a second focus");
    assert_eq!(h.ctx.memory(|m| m.focused()), owner, "a consumed trigger remains a side-effect-free refusal");
    text(&mut h, "11");
    assert_eq!(
        h.state().editor.as_ref().map(|ed| (&ed.text, ed.cell, ed.from_formula_bar)),
        Some((&"11".to_string(), gridcraft_engine::core::CellRef::parse("A1").unwrap(), true)),
        "typed editor state; focus={:?}",
        h.ctx.memory(|m| m.focused())
    );
    key(&mut h, Key::Enter);
    assert_eq!(value(&h, "A1"), gridcraft_engine::core::Value::Number(11.0));
    let b2 = cell(&h, "B2");
    tap(&mut h, b2);
    assert!(!admitted(&h), "cell selection alone is not text editing");
    assert!(h.state().editor.is_none(), "selection must not create a latent text editor");
    assert_eq!(
        h.state().session.doc().unwrap().selection.active,
        gridcraft_engine::core::CellRef::parse("B2").unwrap(),
        "actual touched cell; click={:?} pan={} view={:?} at={:?}",
        h.state().grid.last_click,
        h.state().grid.touch_pan,
        h.state().view().scroll,
        b2
    );
    let b2 = cell(&h, "B2");
    tap(&mut h, b2);
    assert!(h.state().editor.as_ref().is_some_and(|ed| !ed.from_formula_bar), "the second real tap must edit without an intervening idle repaint");
    assert!(admitted(&h), "double-cell trigger admits the actual current in-cell TextEdit");
    text(&mut h, "22");
    tap_formula(&mut h);
    assert!(h.state().editor.as_ref().is_some_and(|ed| ed.from_formula_bar));
    assert!(admitted(&h), "in-cell to formula handoff admits only the current editor");
    h.run_steps(2);
    assert!(!admitted(&h), "an ordinary IME repaint is not another gesture");
    key(&mut h, Key::Enter);
    assert_eq!(value(&h, "B2"), gridcraft_engine::core::Value::Number(22.0));
    let proposal = bridge.snapshot(&mut h.state_mut().session, Mutation { scope, base: head, mutation_id: "touch".into() }).unwrap();
    assert!(proposal.bytes.starts_with(b"PK"));
}

#[test]
fn an_old_document_contact_cannot_authorize_the_replacement_document_editor() {
    let (mut h, _, _, _) = harness();
    let at = formula(&h);
    touch(&mut h, at, true);
    let mut replacement = gridcraft_engine::Session::new();
    replacement.new_workbook();
    h.state_mut().session = replacement;
    touch(&mut h, at, false);
    assert!(!admitted(&h), "same widget ID on a new document is not the pressed owner");
    assert!(h.state().editor.is_none(), "a release from another document cannot create a new editor");
    key(&mut h, Key::Enter);
    tap_formula(&mut h);
    assert!(admitted(&h), "a fresh current-document contact still works");
}

#[test]
fn current_editor_pulses_need_same_owner_frame_and_ime_and_frozen_cas_cannot_edit() {
    let (mut h, mut bridge, _, _) = harness();
    tap_formula(&mut h);
    assert!(admitted(&h));
    h.run_steps(2); // Isolate invalid pulses from the independent real click above.
    let pulse_key = egui::Id::new("eframe.text_input_activation");
    let frame = h.ctx.cumulative_frame_nr().checked_sub(1).unwrap();
    for (at, owner, ime) in [
        (frame, egui::Id::new("another-editor"), true),
        (frame.checked_sub(1).unwrap(), egui::Id::new("gridcraft.formula_bar"), true),
        (frame, egui::Id::new("gridcraft.formula_bar"), false),
    ] {
        h.ctx.data_mut(|data| data.insert_temp(pulse_key, Some((at, owner))));
        assert!(!web_activation::accepted_output_text(&h.ctx, if ime { h.output().platform_output.ime.as_ref() } else { None }));
    }
    key(&mut h, Key::Enter);
    bridge.freeze(&mut h.state_mut().session, true).unwrap();
    tap_formula(&mut h);
    assert!(h.state().editor.is_none());
    assert!(!admitted(&h));
}

#[test]
fn cancelled_contact_does_not_become_a_late_editor_admission() {
    let (mut h, _, _, _) = harness();
    let at = formula(&h);
    touch(&mut h, at, true);
    let mut contact = web_activation::PrimaryContact::default();
    assert!(contact.start(web_activation::Family::Touch, 17, true));
    let mut events = Vec::new();
    assert!(web_activation::cancel_primary(&h.ctx, &mut contact, &mut events));
    h.input_mut().events.extend(events);
    h.input_mut().events.push(Event::Touch {
        device_id: egui::TouchDeviceId(0),
        id: egui::TouchId::from(17),
        phase: egui::TouchPhase::Cancel,
        pos: at,
        force: None,
    });
    h.step();
    touch(&mut h, at, false);
    assert!(!admitted(&h));
    assert!(h.state().editor.is_none());
    tap(&mut h, at);
    assert!(admitted(&h), "cancellation does not poison a later real contact");
}

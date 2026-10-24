//! Actual Word canvas and the exact predicate consumed by `AppRunner`'s user callback.
use super::*;
#[path = "../../../vendor/eframe-0.36.2/src/web/text_input_activation.rs"]
mod web_activation;

fn point(h: &mut Harness, offset: usize) -> Result<egui::Pos2> {
    let target = wordcraft_doc::Pos::body(0, offset);
    let caret = h
        .state_mut()
        .session
        .layout()
        .caret_on(&target, 0)
        .ok_or_else(|| failure("fixture_target_caret"))?;
    wordcraft_ui_egui::canvas::page_to_screen(
        h.state_mut(),
        caret.page,
        caret.x,
        caret.top + caret.height * 0.5,
    )
    .ok_or_else(|| failure("fixture_target_screen"))
}
fn touch(h: &mut Harness, at: egui::Pos2, pressed: bool) {
    h.input_mut().time = Some(h.ctx.input(|input| input.time) + 0.1);
    h.input_mut().events.push(Event::PointerMoved(at));
    h.input_mut().events.push(Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
    if !pressed {
        h.input_mut().events.push(Event::PointerGone);
    }
    h.input_mut().events.push(Event::Touch {
        device_id: egui::TouchDeviceId(0),
        id: egui::TouchId::from(17),
        phase: if pressed {
            egui::TouchPhase::Start
        } else {
            egui::TouchPhase::End
        },
        pos: at,
        force: None,
    });
    web_activation::clear_custom(&h.ctx);
    h.step();
}
fn admitted(h: &Harness) -> bool {
    web_activation::accepted_output_text(&h.ctx, h.output().platform_output.ime.as_ref())
}
#[test]
fn first_actual_word_canvas_tap_admits_only_current_rendered_caret() -> Result<()> {
    let (mut h, _, _) = harness()?;
    let at = point(&mut h, 4)?;
    touch(&mut h, at, true);
    touch(&mut h, at, false);
    assert!(
        h.state().canvas.focused,
        "actual canvas must retain its real keyboard owner"
    );
    assert!(
        h.output().platform_output.ime.is_some(),
        "actual rendered Word caret must publish IME"
    );
    assert!(
        admitted(&h),
        "actual AppRunner gate must admit this primary gesture's current canvas owner"
    );
    let (caret, height) = wordcraft_ui_egui::canvas::caret_screen(h.state_mut())
        .ok_or_else(|| failure("fixture_current_caret"))?;
    let ime = h
        .output()
        .platform_output
        .ime
        .as_ref()
        .ok_or_else(|| failure("fixture_current_ime"))?;
    assert!(
        ime.cursor_rect.min.distance(caret) < 1.0,
        "geometry must be from the selected current caret"
    );
    assert!((ime.cursor_rect.height() - height).abs() < 1.0);
    h.run_steps(2);
    assert!(
        !admitted(&h),
        "ordinary focused IME repaint is not a new gesture"
    );
    Ok(())
}

#[test]
fn cancelled_primary_cannot_focus_and_a_fresh_actual_contact_recovers() -> Result<()> {
    let (mut h, _, _) = harness()?;
    let at = point(&mut h, 2)?;
    touch(&mut h, at, true);
    let mut contact = web_activation::PrimaryContact::default();
    assert!(contact.start(web_activation::Family::Touch, 17, true));
    let mut events = Vec::new();
    assert!(web_activation::cancel_primary(
        &h.ctx,
        &mut contact,
        &mut events
    ));
    events.push(Event::Touch {
        device_id: egui::TouchDeviceId(0),
        id: egui::TouchId::from(17),
        phase: egui::TouchPhase::Cancel,
        pos: at,
        force: None,
    });
    h.input_mut().events.extend(events);
    h.step();
    assert!(!admitted(&h));
    h.run_steps(2);
    touch(&mut h, at, true);
    touch(&mut h, at, false);
    assert!(admitted(&h));
    Ok(())
}
#[test]
fn another_document_cannot_inherit_the_pressed_canvas_and_new_contact_still_works() -> Result<()> {
    let (mut h, _, _) = harness()?;
    let at = point(&mut h, 3)?;
    touch(&mut h, at, true);
    let (_, replacement, _) = opened("Other document")?;
    h.state_mut().session = replacement;
    touch(&mut h, at, false);
    assert!(!admitted(&h));
    assert_eq!(
        h.state().session.doc.plain_text(StoryRef::Body),
        "Other document"
    );
    h.run_steps(2);
    let current = point(&mut h, 4)?;
    touch(&mut h, current, true);
    touch(&mut h, current, false);
    assert!(admitted(&h));
    Ok(())
}
#[test]
fn current_owner_requires_actual_ime_and_never_admits_stale_or_foreign_pulses() -> Result<()> {
    let (mut h, _, _) = harness()?;
    let at = point(&mut h, 3)?;
    touch(&mut h, at, true);
    touch(&mut h, at, false);
    assert!(!web_activation::accepted_output_text(&h.ctx, None));
    assert!(
        !admitted(&h),
        "a pulse is single-use even when IME was absent"
    );
    h.run_steps(2);
    let owner = h
        .ctx
        .memory(egui::Memory::focused)
        .ok_or_else(|| failure("fixture_owner"))?;
    let frame = h
        .ctx
        .cumulative_frame_nr()
        .checked_sub(1)
        .ok_or_else(|| failure("fixture_frame"))?;
    for (frame, owner) in [
        (frame, owner),
        (frame, owner.with("foreign")),
        (frame.saturating_sub(1), owner),
    ] {
        h.ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::new("eframe.text_input_activation"),
                Some((frame, owner)),
            )
        });
        assert!(
            !admitted(&h),
            "ordinary focused repaint or forged owner is not a primary gesture"
        );
    }
    Ok(())
}

#[test]
fn multitouch_cannot_authorize_ime_and_a_fresh_single_contact_still_works() -> Result<()> {
    let (mut h, _, _) = harness()?;
    let at = point(&mut h, 2)?;
    touch(&mut h, at, true);
    h.input_mut().events.push(Event::Touch {
        device_id: egui::TouchDeviceId(0),
        id: egui::TouchId::from(18),
        phase: egui::TouchPhase::Start,
        pos: at + egui::vec2(20.0, 0.0),
        force: None,
    });
    h.step();
    touch(&mut h, at, false);
    assert!(!admitted(&h));
    h.input_mut().events.push(Event::Touch {
        device_id: egui::TouchDeviceId(0),
        id: egui::TouchId::from(18),
        phase: egui::TouchPhase::End,
        pos: at + egui::vec2(20.0, 0.0),
        force: None,
    });
    h.step();
    h.run_steps(2);
    touch(&mut h, at, true);
    touch(&mut h, at, false);
    assert!(admitted(&h));
    Ok(())
}
#[test]
fn paused_historical_canvas_cannot_admit_keyboard_or_mutate_document() -> Result<()> {
    let (mut h, mut b, _) = harness()?;
    let retained = h.state().session.doc.clone();
    b.freeze(&mut h.state_mut().session, true)?;
    let at = point(&mut h, 2)?;
    touch(&mut h, at, true);
    touch(&mut h, at, false);
    assert!(!admitted(&h));
    h.input_mut()
        .events
        .push(Event::Text("must not write".into()));
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(h.state().session.doc, retained);
    b.freeze(&mut h.state_mut().session, false)?;
    touch(&mut h, at, true);
    touch(&mut h, at, false);
    assert!(admitted(&h));
    Ok(())
}

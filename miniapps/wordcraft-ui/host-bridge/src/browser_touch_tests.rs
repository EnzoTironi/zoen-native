//! Actual Word canvas and the exact predicate consumed by AppRunner's user callback.
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

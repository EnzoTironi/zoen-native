//! Actual upstream App: a touch releases hover before it opens the paragraph.
use super::{fixture, ready};
use egui::{Event, Modifiers, PointerButton, Pos2, TouchDeviceId, TouchId, TouchPhase};
use egui_kittest::Harness;
use fixture::{Editor, Result, source};

fn touch(phase: TouchPhase, pos: Pos2) -> Event {
    Event::Touch {
        device_id: TouchDeviceId(1),
        id: TouchId(1),
        phase,
        pos,
        force: None,
    }
}
fn tap(editor: &mut Harness<'_, Editor>, pos: Pos2) {
    editor.input_mut().events.extend([
        Event::PointerMoved(pos),
        touch(TouchPhase::Start, pos),
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
    ]);
    editor.run_steps(1);
    editor.input_mut().events.extend([
        touch(TouchPhase::End, pos),
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
        Event::PointerGone,
    ]);
    editor.run_steps(2);
}

#[test]
fn actual_touch_release_opens_paragraph_without_retaining_hover() -> Result<()> {
    let bytes = source()?;
    let mut editor = ready(Editor::open(&bytes)?)?;
    assert!(editor.state_mut().app.execute("edit.edit_text"));
    editor.run_steps(2);
    let pos = editor.state().app.views[0]
        .glyph_screen_pos(0, 2)
        .ok_or("actual_paragraph_not_visible")?;
    tap(&mut editor, pos);
    assert!(
        editor.state().app.views[0].line_editor.is_some(),
        "the admitted touch click must open the same paragraph as a mouse click"
    );
    editor
        .input_mut()
        .events
        .push(Event::Text(" touch draft".into()));
    editor.run_steps(2);
    assert!(
        editor.state().app.views[0]
            .line_editor
            .as_ref()
            .is_some_and(|draft| draft.text.contains(" touch draft"))
    );
    let current = editor.state();
    assert_eq!(
        current.app.session.save_bytes(current.id()?)?.as_slice(),
        bytes
    );
    Ok(())
}

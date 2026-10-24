//! Actual upstream WordApp/canvas input, layout, engine and writer at phone geometry.
use super::*;
use crate::{
    browser::BrowserBridge,
    browser_tests::{head, mutation, opened, scope},
};
use egui::{Event, Key, Modifiers, PointerButton};
use wordcraft_ui_egui::{Services, WordApp};
type Harness = egui_kittest::Harness<'static, WordApp>;
fn harness() -> Result<(Harness, BrowserBridge, Vec<u8>)> {
    let (bridge, session, bytes) = opened("Original")?;
    let app = WordApp::new(session, Services::default());
    let mut h = egui_kittest::Harness::builder()
        .with_size(egui::vec2(390.0, 844.0))
        .build_ui_state(
            |ui, app: &mut WordApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            app,
        );
    h.run_steps(4);
    Ok((h, bridge, bytes))
}
fn click_canvas(h: &mut Harness) -> Result<()> {
    let rect = h
        .state()
        .canvas
        .page_rects
        .first()
        .copied()
        .ok_or_else(|| failure("fixture_page_missing"))?;
    let pos = rect.center();
    h.input_mut().events.push(Event::PointerMoved(pos));
    h.input_mut().events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    assert!(h.state().canvas.host_interaction());
    h.input_mut().events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    assert!(!h.state().canvas.host_interaction());
    assert!(h.state().canvas.focused);
    Ok(())
}
fn key(h: &mut Harness, key: Key, modifiers: Modifiers) {
    h.input_mut()
        .events
        .extend([true, false].map(|pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        }));
    h.run_steps(2);
}
#[test]
fn actual_canvas_keyboard_checkpoint_pending_freeze_ack_undo_and_accepted_export() -> Result<()> {
    let (mut h, mut bridge, bytes) = harness()?;
    click_canvas(&mut h)?;
    key(&mut h, Key::A, Modifiers::COMMAND);
    h.input_mut()
        .events
        .push(Event::Text("Human edit Olá".into()));
    h.run_steps(2);
    assert_eq!(
        h.state().session.doc.plain_text(StoryRef::Body),
        "Human edit Olá"
    );
    let proposal = bridge.snapshot(&mut h.state_mut().session, mutation(&bytes, "keyboard"))?;
    h.input_mut()
        .events
        .push(Event::Text("unacknowledged second edit".into()));
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert_eq!(
        h.state().session.doc.plain_text(StoryRef::Body),
        "Human edit Olá"
    );
    assert!(h.state_mut().run("format.bold", json!({})).is_err());
    bridge.accept(
        &mut h.state_mut().session,
        &Accept {
            scope: scope(),
            mutation_id: "keyboard".into(),
            head: head("2", &proposal.bytes),
        },
    )?;
    key(&mut h, Key::Z, Modifiers::COMMAND);
    assert_eq!(h.state().session.doc.plain_text(StoryRef::Body), "Original");
    assert_eq!(
        bridge
            .export(
                &h.state().session,
                &Export {
                    scope: scope(),
                    head: head("2", &proposal.bytes),
                    format: ExportFormat::Docx
                }
            )?
            .bytes,
        proposal.bytes
    );
    Ok(())
}
#[test]
fn actual_composition_and_pointer_gesture_are_visible_to_host_without_text_payload() -> Result<()> {
    let (mut h, _, _) = harness()?;
    click_canvas(&mut h)?;
    h.input_mut()
        .events
        .push(Event::Ime(egui::ImeEvent::Preedit {
            text: "日本".into(),
            active_range_chars: None,
        }));
    h.run_steps(2);
    assert!(h.state().canvas.host_interaction());
    assert_eq!(h.state().session.doc.plain_text(StoryRef::Body), "Original");
    h.input_mut()
        .events
        .push(Event::Ime(egui::ImeEvent::Commit("日本".into())));
    h.run_steps(2);
    assert!(!h.state().canvas.host_interaction());
    assert!(
        h.state()
            .session
            .doc
            .plain_text(StoryRef::Body)
            .contains("日本")
    );
    Ok(())
}

#[test]
fn actual_keyboard_tab_enter_and_ime_create_the_same_plain_document_profile() -> Result<()> {
    let (mut h, mut b, bytes) = harness()?;
    click_canvas(&mut h)?;
    key(&mut h, Key::A, Modifiers::COMMAND);
    h.input_mut().events.push(Event::Text("First".into()));
    h.run_steps(2);
    key(&mut h, Key::Tab, Modifiers::NONE);
    h.input_mut()
        .events
        .push(Event::Ime(egui::ImeEvent::Commit("日本語".into())));
    h.run_steps(2);
    key(&mut h, Key::Enter, Modifiers::NONE);
    h.input_mut().events.push(Event::Text("Last".into()));
    h.run_steps(2);
    assert_eq!(
        h.state().session.doc.plain_text(StoryRef::Body),
        "First\t日本語\nLast"
    );
    assert_eq!(h.state().session.doc.body.len(), 2);
    let proposal = b.snapshot(
        &mut h.state_mut().session,
        mutation(&bytes, "tab-enter-ime"),
    )?;
    assert_eq!(
        profile::parse(&proposal.bytes)?.plain_text(StoryRef::Body),
        "First\t日本語\nLast"
    );
    Ok(())
}

#[path = "browser_touch_tests.rs"]
mod touch;

#[path = "browser_pending_tests.rs"]
mod pending;

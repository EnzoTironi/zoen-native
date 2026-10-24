//! Actual pinned egui layout after a retained cell editor encounters keyboard-sized space.
use super::*;

fn keyboard_view(h: &mut Harness, height: f32) {
    h.set_size(egui::vec2(874.0, height));
    h.input_mut().safe_area_insets = Some(egui::SafeAreaInsets(egui::epaint::MarginF32 {
        top: 56.0,
        left: 62.0,
        right: 0.0,
        bottom: 0.0,
    }));
    h.run_steps(3);
}

#[test]
fn retained_cell_editor_is_visible_inside_the_real_keyboard_viewport() {
    let (mut h, _, _, _) = harness();
    h.state_mut()
        .run("selection.set", json!({"cell":"B1"}))
        .expect("existing selection API");
    h.state_mut().begin_edit(Some("QA B1 🪴".into()), false);
    h.run_steps(3);
    let focused = h.ctx.memory(|m| m.focused());
    assert_eq!(focused, Some(egui::Id::new("gridcraft.cell_editor")));
    let original = h
        .state()
        .editor
        .as_ref()
        .expect("real retained editor")
        .clone();
    keyboard_view(&mut h, 114.0);
    let ime = h
        .output()
        .platform_output
        .ime
        .as_ref()
        .expect("a visible actual current TextEdit must still publish IME");
    let visible = h.ctx.content_rect();
    assert!(
        visible.contains_rect(ime.cursor_rect),
        "current caret {:?} must fit {:?}",
        ime.cursor_rect,
        visible
    );
    assert!(
        visible.intersects(ime.rect),
        "the real editor cannot be fully clipped"
    );
    assert_eq!(
        h.ctx.memory(|m| m.focused()),
        focused,
        "resizing cannot invent a replacement text owner"
    );
    let ed = h.state().editor.as_ref().expect("retained same edit");
    assert_eq!(
        (&ed.text, ed.cell, ed.sheet, ed.caret, ed.from_formula_bar),
        (
            &original.text,
            original.cell,
            original.sheet,
            original.caret,
            original.from_formula_bar
        )
    );
    assert!(
        !h.state().ui.ribbon_collapsed,
        "compact render cannot rewrite ribbon preference"
    );
    text(&mut h, " still editing");
    key(&mut h, Key::Enter);
    assert_eq!(
        value(&h, "B1"),
        gridcraft_engine::core::Value::Text("QA B1 🪴 still editing".into())
    );
}

fn current_input(h: &Harness) -> &str {
    &h.state().editor.as_ref().expect("current real editor").text
}

#[test]
fn compact_cancel_and_apply_use_existing_actions_without_rewriting_ui_preferences() {
    use egui_kittest::kittest::Queryable as _;
    let (mut h, _, _, _) = harness();
    h.state_mut().ui.ribbon_collapsed = true;
    h.state_mut().ui.formula_bar_expanded = true;
    h.state_mut().session.prefs.formula_bar = false;
    let prefs = serde_json::to_value(&h.state().ui).expect("existing serializable preferences");
    h.state_mut().begin_edit(Some("cancel this".into()), true);
    keyboard_view(&mut h, 114.0);
    let visible = h.ctx.content_rect();
    for label in ["Cancel", "Enter", "Commands"] {
        assert!(
            visible.contains_rect(h.get_by_label(label).rect()),
            "{label} must be a real visible control"
        );
    }
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(h.state().editor.is_none());
    assert_eq!(value(&h, "A1"), gridcraft_engine::core::Value::Empty);
    assert_eq!(
        serde_json::to_value(&h.state().ui).expect("UI preferences"),
        prefs
    );
    assert!(!h.state().session.prefs.formula_bar);

    h.state_mut().begin_edit(Some("42".into()), false);
    h.run_steps(3);
    assert!(
        h.ctx
            .content_rect()
            .contains_rect(h.get_by_label("Enter").rect())
    );
    h.get_by_label("Enter").click();
    h.run_steps(3);
    assert!(h.state().editor.is_none());
    assert_eq!(value(&h, "A1"), gridcraft_engine::core::Value::Number(42.0));
    assert_eq!(
        serde_json::to_value(&h.state().ui).expect("UI preferences"),
        prefs
    );
    assert!(!h.state().session.prefs.formula_bar);
}

#[test]
fn compact_resize_preserves_actual_composition_cursor_and_editor_identity() {
    let (mut h, _, _, _) = harness();
    h.state_mut().begin_edit(Some(String::new()), false);
    h.run_steps(3);
    let owner = h.ctx.memory(|m| m.focused());
    h.input_mut()
        .events
        .extend([Event::Ime(egui::ImeEvent::Preedit {
            text: "あ".into(),
            active_range_chars: None,
        })]);
    h.run_steps(2);
    for height in [339.33334, 213.0, 237.0, 270.0, 114.0] {
        keyboard_view(&mut h, height);
        assert_eq!(h.ctx.memory(|m| m.focused()), owner);
        assert_eq!(current_input(&h), "あ");
        assert!(
            h.ctx.content_rect().contains_rect(
                h.output()
                    .platform_output
                    .ime
                    .as_ref()
                    .expect("same actual composing field after each observed-size resize")
                    .cursor_rect
            )
        );
    }
    assert!(
        h.ctx.content_rect().contains_rect(
            h.output()
                .platform_output
                .ime
                .as_ref()
                .expect("current composing editor")
                .cursor_rect
        )
    );
    keyboard_view(&mut h, 402.0);
    assert_eq!(h.ctx.memory(|m| m.focused()), owner);
    assert_eq!(current_input(&h), "あ");
    h.input_mut()
        .events
        .push(Event::Ime(egui::ImeEvent::Commit("い".into())));
    h.run_steps(2);
    assert_eq!(
        current_input(&h),
        "い",
        "same TextEdit composition replaces its preedit once"
    );
    keyboard_view(&mut h, 114.0);
    key(&mut h, Key::Enter);
    assert_eq!(
        value(&h, "A1"),
        gridcraft_engine::core::Value::Text("い".into())
    );
}

#[test]
fn compact_commands_are_confined_scrollable_and_do_not_commit_the_active_draft() {
    use egui_kittest::kittest::Queryable as _;
    let (mut h, _, _, _) = harness();
    h.state_mut()
        .begin_edit(Some("still private input".into()), false);
    keyboard_view(&mut h, 114.0);
    let catalog = h
        .state()
        .session
        .commands()
        .into_iter()
        .filter(|c| c.enabled)
        .collect::<Vec<_>>();
    let label = |command: &gridcraft_engine::CommandInfo| {
        format!("{}    {}", command.label, command.menu.join(" › "))
    };
    let first = label(catalog.first().expect("actual nonempty catalog"));
    let last = label(catalog.last().expect("actual last command"));
    h.get_by_label("Commands").click();
    h.run_steps(4);
    let rect = h.get_by_label(&first).rect();
    let visible = h.ctx.content_rect();
    assert!(
        visible.contains_rect(rect),
        "the actual first command must be reachable"
    );
    h.input_mut().events.extend([
        Event::PointerMoved(rect.center()),
        Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(
                0.0,
                -(f32::from(
                    u16::try_from(catalog.len()).expect("the fixed command catalog fits u16"),
                ) * 32.0),
            ),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(4);
    assert!(
        visible.contains_rect(h.get_by_label(&last).rect()),
        "the last existing command must be reachable by real scrolling"
    );
    assert_eq!(current_input(&h), "still private input");
    assert_eq!(value(&h, "A1"), gridcraft_engine::core::Value::Empty);
    assert!(!h.state().ui.ribbon_collapsed);
    key(&mut h, Key::Escape);
    assert_eq!(
        current_input(&h),
        "still private input",
        "Escape closes Commands before touching the retained draft"
    );
    assert_eq!(value(&h, "A1"), gridcraft_engine::core::Value::Empty);
}

#[test]
fn compact_menu_retains_the_existing_explicit_host_share_intent_once() {
    use egui_kittest::kittest::Queryable as _;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (mut h, _, _, _) = harness();
    let intents = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&intents);
    h.state_mut().services.host_export_intent = Some(Box::new(move |format| {
        assert_eq!(format, "xlsx");
        observed.fetch_add(1, Ordering::SeqCst);
        false
    }));
    h.state_mut()
        .begin_edit(Some("retained during command menu".into()), false);
    keyboard_view(&mut h, 114.0);
    assert_eq!(intents.load(Ordering::SeqCst), 0);
    h.get_by_label("Commands").click();
    h.run_steps(4);
    assert_eq!(intents.load(Ordering::SeqCst), 0);
    let share = h.get_by_label("Share");
    assert!(h.ctx.content_rect().contains_rect(share.rect()));
    share.click();
    h.run_steps(4);
    assert_eq!(intents.load(Ordering::SeqCst), 1);
    assert_eq!(current_input(&h), "retained during command menu");
    assert_eq!(value(&h, "A1"), gridcraft_engine::core::Value::Empty);
    assert!(!h.state().ui.ribbon_collapsed);
}

#[test]
fn multiline_formula_draft_scrolls_the_same_caret_inside_the_compact_field() {
    let (mut h, _, _, _) = harness();
    let draft = "line one\nline two\nline three\nline four\nline five";
    h.state_mut().begin_edit(Some(draft.into()), true);
    h.run_steps(3);
    let owner = h.ctx.memory(|m| m.focused());
    keyboard_view(&mut h, 114.0);
    assert_eq!(h.ctx.memory(|m| m.focused()), owner);
    let ime = h
        .output()
        .platform_output
        .ime
        .as_ref()
        .expect("same multiline editor");
    assert!(
        h.ctx.content_rect().contains_rect(ime.cursor_rect),
        "the actual last-line caret must scroll into the short viewport: {:?}",
        ime.cursor_rect
    );
    assert_eq!(current_input(&h), draft);
    key(&mut h, Key::Escape);
    assert_eq!(value(&h, "A1"), gridcraft_engine::core::Value::Empty);
}

#[test]
fn an_already_safe_embedding_parent_does_not_apply_the_inset_twice() {
    let (session, _, _, _, _) = super::super::tests::open();
    let mut app = gridcraft_ui_egui::SheetApp::new(session, Default::default());
    app.chrome.title_bar = false;
    app.chrome.file_tab = false;
    let mut h = egui_kittest::Harness::builder()
        .with_size(egui::vec2(874.0, 402.0))
        .build_ui_state(
            |ui, app: &mut gridcraft_ui_egui::SheetApp| {
                app.logic(ui.ctx());
                let safe = ui.ctx().content_rect();
                ui.scope_builder(egui::UiBuilder::new().max_rect(safe), |ui| app.ui(ui));
            },
            app,
        );
    h.run_steps(4);
    h.state_mut()
        .begin_edit(Some("same safe frame".into()), true);
    keyboard_view(&mut h, 114.0);
    let ime = h
        .output()
        .platform_output
        .ime
        .as_ref()
        .expect("actual current safe editor");
    assert!(
        h.ctx.content_rect().contains_rect(ime.cursor_rect),
        "already-safe parent must not turn top56 into112: {:?}",
        ime.cursor_rect
    );
    assert_eq!(current_input(&h), "same safe frame");
}

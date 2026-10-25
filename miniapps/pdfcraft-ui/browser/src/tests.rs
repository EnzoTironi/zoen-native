//! Native retained-App checks. Browser touch and iPhone keyboard are separate qualifications.
mod fixture;
mod touch;
use egui_kittest::Harness;
use fixture::{Editor, Result, accepted, head, mutation, source};
use pdfcraft_engine::Edit;
use pdfcraft_zoen_host::{Command, Export, ExportFormat, Operation};
use std::time::{Duration, Instant};

fn ready(editor: Editor) -> Result<Harness<'static, Editor>> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(402.0, 874.0))
        .build_eframe(move |_| editor);
    harness.input_mut().safe_area_insets = Some(safe(118, 34));
    settle(&mut harness)?;
    Ok(harness)
}
fn safe(top: i8, bottom: i8) -> egui::SafeAreaInsets {
    egui::SafeAreaInsets(egui::epaint::MarginF32 {
        top: f32::from(top),
        bottom: f32::from(bottom),
        left: 0.0,
        right: 0.0,
    })
}
fn settle(harness: &mut Harness<'_, Editor>) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(20) {
        harness.run_steps(2);
        if harness.state().app.views[0]
            .glyph_screen_pos(0, 2)
            .is_some()
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err("actual_app_text_layer_timeout".into())
}

fn click(harness: &mut Harness<'_, Editor>, position: egui::Pos2) {
    harness.hover_at(position);
    harness.run_steps(1);
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(1);
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(2);
}

#[test]
fn actual_canvas_consumes_safe_area_once_across_orientation_and_keyboard_height() -> Result<()> {
    let bytes = source()?;
    let mut harness = ready(Editor::open(&bytes)?)?;
    for (size, top, bottom) in [
        (egui::vec2(402.0, 874.0), 118, 34),
        (egui::vec2(874.0, 402.0), 70, 20),
        (egui::vec2(874.0, 220.0), 70, 0),
    ] {
        // egui_kittest intentionally bypasses App.raw_input_hook. Feed the real
        // RawInput insets directly, matching production Shell before Context.run.
        harness.input_mut().safe_area_insets = Some(safe(top, bottom));
        harness.set_size(size).run_steps(4);
        let view = harness.state().app.views[0].viewport_rect();
        let safe = egui::Rect::from_min_max(
            egui::pos2(0.0, f32::from(top)),
            egui::pos2(size.x, size.y - f32::from(bottom)),
        );
        assert!(view.is_positive(), "actual document viewport: {view:?}");
        assert!(safe.contains_rect(view), "canvas {view:?} outside {safe:?}");
        let editor = harness.state();
        assert_eq!(
            editor.app.session.save_bytes(editor.id()?)?.as_slice(),
            bytes
        );
        assert!(!editor.app.session.javascript());
    }
    Ok(())
}

fn first_manual_checkpoint(
    editor: &mut Editor,
    bytes: &[u8],
) -> Result<pdfcraft_zoen_host::Proposal> {
    let id = editor.id()?;
    editor.app.session.apply(
        id,
        Edit::EditTextBlock {
            page: 0,
            block: 0,
            text: "First manual paragraph.".into(),
            style: pdfcraft_engine::BlockStyle::default(),
        },
    )?;
    Ok(editor.bridge.snapshot(
        &mut editor.app.session,
        mutation(&head("1", bytes), "human-1"),
    )?)
}

#[test]
fn older_ack_preserves_new_live_paragraph_draft_and_actual_native_undo() -> Result<()> {
    let bytes = source()?;
    let mut harness = ready(Editor::open(&bytes)?)?;
    let first = first_manual_checkpoint(harness.state_mut(), &bytes)?;
    settle(&mut harness)?;
    assert!(harness.state_mut().app.execute("edit.edit_text"));
    harness.run_steps(2);
    let position = harness.state().app.views[0]
        .glyph_screen_pos(0, 2)
        .ok_or("actual_paragraph_not_visible")?;
    click(&mut harness, position);
    harness
        .input_mut()
        .events
        .push(egui::Event::Text(" live draft".into()));
    harness.run_steps(2);
    let draft = harness.state().app.views[0]
        .line_editor
        .as_ref()
        .ok_or("actual_pointer_did_not_open_paragraph")?
        .text
        .clone();
    assert!(
        draft.contains("live draft"),
        "actual egui Text input was not consumed: {draft}"
    );
    assert!(harness.state().app.host_interaction());
    let receipt = accepted("human-1", "2", &first.bytes);
    {
        let editor = harness.state_mut();
        let interaction = editor.app.host_interaction();
        editor
            .bridge
            .accept(&mut editor.app.session, &receipt, interaction)?;
        assert_eq!(
            editor.app.views[0]
                .line_editor
                .as_ref()
                .ok_or("draft_lost")?
                .text,
            draft
        );
        assert!(editor.bridge.finish_input(&mut editor.app, true).is_err());
        assert_eq!(
            editor.app.views[0]
                .line_editor
                .as_ref()
                .ok_or("draft_lost")?
                .text,
            draft
        );
        editor.bridge.finish_input(&mut editor.app, false)?;
    }
    harness.run_steps(2);
    let second = {
        let editor = harness.state_mut();
        editor
            .bridge
            .snapshot(&mut editor.app.session, mutation(&receipt.head, "human-2"))?
    };
    assert_ne!(second.bytes, first.bytes);
    {
        let editor = harness.state_mut();
        editor.bridge.accept(
            &mut editor.app.session,
            &accepted("human-2", "3", &second.bytes),
            false,
        )?;
        assert!(
            editor.app.execute("edit.undo"),
            "ACK must retain actual Undo"
        );
        assert_eq!(
            editor
                .bridge
                .export(
                    &editor.app.session,
                    &Export {
                        scope: fixture::scope(),
                        head: head("3", &second.bytes),
                        format: ExportFormat::Pdf
                    }
                )?
                .bytes,
            second.bytes,
            "live Undo cannot replace the selected accepted version"
        );
    }
    let reopened = Editor::open(&second.bytes)?;
    assert_eq!(
        reopened.app.session.save_bytes(reopened.id()?)?.as_slice(),
        second.bytes.as_slice()
    );
    Ok(())
}

#[test]
fn refused_agent_edit_restores_native_editor_generation_and_redo() -> Result<()> {
    let bytes = source()?;
    let mut editor = Editor::open(&bytes)?;
    let id = editor.id()?;
    editor.app.session.apply(
        id,
        Edit::RotatePages {
            pages: vec![0],
            degrees: 90,
        },
    )?;
    editor.app.session.undo(id)?;
    let generation = editor
        .app
        .session
        .get(id)
        .ok_or("missing_document")?
        .edit_generation();
    // Bring canonical generation to this clean native state, retaining its real Redo.
    let proposal = editor.bridge.snapshot(
        &mut editor.app.session,
        mutation(&head("1", &bytes), "human"),
    )?;
    editor.bridge.accept(
        &mut editor.app.session,
        &accepted("human", "2", &proposal.bytes),
        true,
    )?;
    let result = editor.bridge.execute(
        &mut editor.app.session,
        Command {
            mutation: mutation(&head("2", &proposal.bytes), "agent"),
            action: Operation::ReplaceText {
                page: 0,
                block: 0,
                expected: "wrong paragraph".into(),
                text: "must not appear".into(),
            },
        },
    );
    assert!(result.is_err());
    assert_eq!(
        editor
            .app
            .session
            .get(id)
            .ok_or("missing_document")?
            .edit_generation(),
        generation
    );
    editor.app.session.redo(id)?;
    assert_eq!(
        editor
            .app
            .session
            .get(id)
            .ok_or("missing_document")?
            .info
            .pages[0]
            .rotation,
        90
    );
    Ok(())
}

#[test]
fn retained_editor_compaction_keeps_real_undo_redo_and_selected_export() -> Result<()> {
    let bytes = source()?;
    let mut editor = Editor::open(&bytes)?;
    let id = editor.id()?;
    let mut current = head("1", &bytes);
    let mut accepted_bytes = bytes;
    let mut compactions = 0;
    let mut previous_revisions = 1;
    for index in 1..=70 {
        editor.app.session.apply(
            id,
            Edit::RotatePages {
                pages: vec![0],
                degrees: 90,
            },
        )?;
        let key = format!("manual-{index}");
        let proposal = editor
            .bridge
            .snapshot(&mut editor.app.session, mutation(&current, &key))?;
        let cos = pdfcraft_cos::Document::open(std::sync::Arc::new(proposal.bytes.clone()))?;
        let catalog = cos.get(cos.root().ok_or("missing_root")?);
        assert_eq!(
            catalog
                .as_dict()
                .and_then(|d| d.get(b"ZoenOpaque"))
                .and_then(pdfcraft_cos::Object::as_string)
                .map(pdfcraft_cos::PdfString::to_text)
                .as_deref(),
            Some("Retain original publisher data")
        );
        let revisions = cos.revisions().len();
        compactions += usize::from(revisions < previous_revisions);
        previous_revisions = revisions;
        assert!(revisions <= 32);
        current = head(&(index + 1).to_string(), &proposal.bytes);
        editor.bridge.accept(
            &mut editor.app.session,
            &accepted(&key, &current.version, &proposal.bytes),
            false,
        )?;
        accepted_bytes = proposal.bytes;
    }
    assert!(compactions >= 2);
    editor.app.session.undo(id)?;
    assert_eq!(
        editor
            .app
            .session
            .get(id)
            .ok_or("missing_document")?
            .info
            .pages[0]
            .rotation,
        90
    );
    assert_eq!(
        editor
            .bridge
            .export(
                &editor.app.session,
                &Export {
                    scope: fixture::scope(),
                    head: current,
                    format: ExportFormat::Pdf,
                }
            )?
            .bytes,
        accepted_bytes
    );
    editor.app.session.redo(id)?;
    assert_eq!(
        editor
            .app
            .session
            .get(id)
            .ok_or("missing_document")?
            .info
            .pages[0]
            .rotation,
        180
    );
    Ok(())
}

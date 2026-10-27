//! Refused canonical refresh never discards native pending work or history.
use super::{
    fixture::{self, Editor, Result, accepted, head, source},
    live_review::{agent_rotation, text},
};
use pdfcraft_engine::Edit;
use pdfcraft_zoen_host::Open;

#[test]
fn pending_ack_keeps_exact_proposal_and_binding_when_peer_version_arrives() -> Result<()> {
    let bytes = source()?;
    let peer = agent_rotation(&bytes, "1")?;
    let mut editor = Editor::open(&bytes)?;
    let manual = super::first_manual_checkpoint(&mut editor, &bytes)?;
    let binding = editor.app.session.managed_binding;
    let generation = editor
        .app
        .session
        .get(editor.id()?)
        .ok_or("missing_document")?
        .edit_generation();
    let draft = text(&editor)?;
    assert!(
        editor
            .bridge
            .open(
                &mut editor.app,
                Open {
                    scope: fixture::scope(),
                    head: head("2", &peer.bytes)
                },
                &peer.bytes
            )
            .is_err()
    );
    assert_eq!(editor.app.session.managed_binding, binding);
    assert_eq!(text(&editor)?, draft);
    assert_eq!(
        editor
            .app
            .session
            .get(editor.id()?)
            .ok_or("missing_document")?
            .edit_generation(),
        generation
    );
    assert_eq!(
        editor.bridge.status(&editor.app.session)?["pendingMutationId"],
        "human-1"
    );
    // A blocked observation must not invalidate the original exact save receipt.
    editor.bridge.accept(
        &mut editor.app.session,
        &accepted("human-1", "2", &manual.bytes),
        false,
    )?;
    assert_eq!(
        editor.bridge.status(&editor.app.session)?["head"],
        serde_json::json!(head("2", &manual.bytes))
    );
    Ok(())
}

#[test]
fn unsaved_native_edit_and_redo_survive_refused_peer_refresh() -> Result<()> {
    let bytes = source()?;
    let peer = agent_rotation(&bytes, "1")?;
    let mut editor = Editor::open(&bytes)?;
    let id = editor.id()?;
    let binding = editor.app.session.managed_binding;
    editor.app.session.apply(
        id,
        Edit::RotatePages {
            pages: vec![0],
            degrees: 90,
        },
    )?;
    assert!(
        editor
            .bridge
            .open(
                &mut editor.app,
                Open {
                    scope: fixture::scope(),
                    head: head("2", &peer.bytes)
                },
                &peer.bytes
            )
            .is_err()
    );
    assert_eq!(editor.app.session.managed_binding, binding);
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
        0
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
fn newer_identical_bytes_update_only_the_head_and_preserve_native_redo() -> Result<()> {
    let bytes = source()?;
    let mut editor = Editor::open(&bytes)?;
    let id = editor.id()?;
    let binding = editor.app.session.managed_binding;
    let manual = super::first_manual_checkpoint(&mut editor, &bytes)?;
    editor.bridge.accept(
        &mut editor.app.session,
        &accepted("human-1", "2", &manual.bytes),
        false,
    )?;
    let manual_text = text(&editor)?;
    editor.app.session.undo(id)?;
    let undone = editor.bridge.snapshot(
        &mut editor.app.session,
        fixture::mutation(&head("2", &manual.bytes), "undo-first"),
    )?;
    editor.bridge.accept(
        &mut editor.app.session,
        &accepted("undo-first", "3", &undone.bytes),
        false,
    )?;
    let generation = editor
        .app
        .session
        .get(id)
        .ok_or("missing_document")?
        .edit_generation();
    editor.bridge.open(
        &mut editor.app,
        Open {
            scope: fixture::scope(),
            head: head("4", &undone.bytes),
        },
        &undone.bytes,
    )?;
    assert_eq!(editor.app.session.managed_binding, binding);
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
    assert_eq!(text(&editor)?, manual_text);
    Ok(())
}

#[test]
fn invalid_or_foreign_canonical_input_preserves_head_pause_and_native_history() -> Result<()> {
    let bytes = source()?;
    let mut editor = Editor::open(&bytes)?;
    let id = editor.id()?;
    let binding = editor.app.session.managed_binding;
    let original_text = text(&editor)?;
    let manual = super::first_manual_checkpoint(&mut editor, &bytes)?;
    editor.bridge.accept(
        &mut editor.app.session,
        &accepted("human-1", "2", &manual.bytes),
        false,
    )?;
    let before = editor.bridge.status(&editor.app.session)?;
    editor.bridge.freeze(&mut editor.app.session, true)?;
    let invalid = b"%PDF-1.7\nnot a complete PDF";
    assert!(
        editor
            .bridge
            .open(
                &mut editor.app,
                Open {
                    scope: fixture::scope(),
                    head: head("3", invalid)
                },
                invalid
            )
            .is_err()
    );
    let mut foreign = fixture::scope();
    foreign.actor_id = "different-owner".into();
    assert!(
        editor
            .bridge
            .open(
                &mut editor.app,
                Open {
                    scope: foreign,
                    head: head("3", &manual.bytes)
                },
                &manual.bytes
            )
            .is_err()
    );
    assert_eq!(
        editor.bridge.status(&editor.app.session)?["head"],
        before["head"]
    );
    assert_eq!(editor.app.session.managed_binding, binding);
    assert!(editor.app.session.managed_frozen);
    editor.bridge.freeze(&mut editor.app.session, false)?;
    editor.app.session.undo(id)?;
    assert_eq!(text(&editor)?, original_text);
    Ok(())
}

#[test]
fn actual_open_paragraph_draft_blocks_canonical_adoption_without_losing_text() -> Result<()> {
    let bytes = source()?;
    let peer = agent_rotation(&bytes, "1")?;
    let mut harness = super::ready(Editor::open(&bytes)?)?;
    assert!(harness.state_mut().app.execute("edit.edit_text"));
    harness.run_steps(2);
    let pos = harness.state().app.views[0]
        .glyph_screen_pos(0, 2)
        .ok_or("paragraph_not_visible")?;
    super::click(&mut harness, pos);
    harness
        .input_mut()
        .events
        .push(egui::Event::Text(" unfinished".into()));
    harness.run_steps(2);
    let draft = harness.state().app.views[0]
        .line_editor
        .as_ref()
        .ok_or("paragraph_not_open")?
        .text
        .clone();
    let editor = harness.state_mut();
    let binding = editor.app.session.managed_binding;
    assert!(
        editor
            .bridge
            .open(
                &mut editor.app,
                Open {
                    scope: fixture::scope(),
                    head: head("2", &peer.bytes)
                },
                &peer.bytes
            )
            .is_err()
    );
    assert_eq!(
        editor.app.views[0]
            .line_editor
            .as_ref()
            .ok_or("draft_lost")?
            .text,
        draft
    );
    assert_eq!(editor.app.session.managed_binding, binding);
    assert_eq!(
        editor.bridge.status(&editor.app.session)?["head"]["version"],
        "1"
    );
    Ok(())
}

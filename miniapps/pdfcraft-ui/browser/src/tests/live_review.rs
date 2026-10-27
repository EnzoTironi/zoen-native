//! New canonical bytes must enter the retained upstream Undo history.
use super::fixture::{self, Editor, Result, accepted, head, mutation, source};
use pdfcraft_zoen_host::{ActorKind, Command, HostBridge, Open, Operation, Proposal};

pub(super) fn agent_rotation(bytes: &[u8], version: &str) -> Result<Proposal> {
    let mut scope = fixture::scope();
    scope.actor_id = "other-author".into();
    scope.actor_kind = ActorKind::Agent;
    let base = head(version, bytes);
    let mut worker = HostBridge::open(
        Open {
            scope: scope.clone(),
            head: base.clone(),
        },
        bytes,
    )?;
    Ok(worker.execute(Command {
        mutation: pdfcraft_zoen_host::Mutation {
            scope,
            base,
            mutation_id: "peer-rotation".into(),
        },
        action: Operation::RotatePages {
            pages: vec![0],
            degrees: 90,
        },
    })?)
}
pub(super) fn text(editor: &Editor) -> Result<Vec<String>> {
    Ok(editor
        .app
        .session
        .get(editor.id()?)
        .ok_or("missing_document")?
        .text_blocks(0)
        .iter()
        .map(|block| block.text.clone())
        .collect())
}

#[test]
fn canonical_agent_update_retains_binding_and_native_undo_through_new_saves() -> Result<()> {
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
    let manual_text = text(&editor)?;
    assert_ne!(original_text, manual_text);
    let peer = agent_rotation(&manual.bytes, "2")?;
    editor.bridge.freeze(&mut editor.app.session, true)?;
    editor.bridge.open(
        &mut editor.app,
        Open {
            scope: fixture::scope(),
            head: head("3", &peer.bytes),
        },
        &peer.bytes,
    )?;
    assert_eq!(
        editor.app.session.managed_binding, binding,
        "canonical refresh cannot replace the live Session binding"
    );
    assert_eq!(editor.id()?, id);
    assert!(editor.app.session.managed_frozen);
    assert!(
        editor.app.session.undo(id).is_err(),
        "the canonical refresh cannot unpause human input"
    );
    assert_eq!(text(&editor)?, manual_text);
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
    editor.bridge.freeze(&mut editor.app.session, false)?;
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
    assert_eq!(text(&editor)?, manual_text);
    let undo = editor.bridge.snapshot(
        &mut editor.app.session,
        mutation(&head("3", &peer.bytes), "undo-peer"),
    )?;
    editor.bridge.accept(
        &mut editor.app.session,
        &accepted("undo-peer", "4", &undo.bytes),
        false,
    )?;
    assert_eq!(text(&editor)?, manual_text);
    editor.app.session.undo(id)?;
    assert_eq!(
        text(&editor)?,
        original_text,
        "Undo before the peer update must remain available"
    );
    let prior = editor.bridge.snapshot(
        &mut editor.app.session,
        mutation(&head("4", &undo.bytes), "undo-human"),
    )?;
    editor.bridge.accept(
        &mut editor.app.session,
        &accepted("undo-human", "5", &prior.bytes),
        false,
    )?;
    assert_eq!(text(&editor)?, original_text);
    assert_eq!(editor.app.session.managed_binding, binding);
    Ok(())
}

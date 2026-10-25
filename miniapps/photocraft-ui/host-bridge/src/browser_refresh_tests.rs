//! Higher canonical bytes must become one real upstream Undo entry, not a new Session.
use super::*;
use crate::{
    Operation,
    managed_tests::{head, scope, seed},
};
use photocraft_format::SaveOptions;

fn bytes(session: &Session) -> Result<Vec<u8>> {
    photocraft_format::save_to_bytes(
        &session.active().ok_or("missing_document")?.doc,
        &SaveOptions::default(),
    )
    .map_err(|_| failure("fixture_encoding"))
}
fn open(version: &str, source: &[u8]) -> Open {
    Open {
        scope: scope(),
        head: head(version, source),
    }
}
fn checkpoint(base: &str, source: &[u8], id: &str) -> Mutation {
    Mutation {
        scope: scope(),
        base: head(base, source),
        mutation_id: id.into(),
    }
}
fn png(bridge: &BrowserBridge, session: &Session, version: &str, source: &[u8]) -> Result<Vec<u8>> {
    Ok(bridge
        .export(
            session,
            &Export {
                scope: scope(),
                head: head(version, source),
                format: managed::ExportFormat::Png,
            },
        )?
        .bytes)
}
fn external(source: &[u8], version: &str) -> Result<Vec<u8>> {
    let mut worker = HostBridge::open(open(version, source), source)?;
    Ok(worker
        .execute(Command {
            mutation: checkpoint(version, source, "external-rotation"),
            action: Operation::RotateClockwise,
        })?
        .bytes)
}

#[test]
fn higher_head_keeps_binding_real_history_pixels_and_undo_checkpoint_baseline() -> Result<()> {
    let original = seed()?;
    let mut session = Session::new();
    let mut bridge = BrowserBridge::default();
    bridge.open(&mut session, open("1", &original), &original)?;
    session
        .execute("image.imageRotation.flipCanvasHorizontal", json!({}))
        .map_err(|_| failure("manual_transform"))?;
    let manual = bridge.snapshot(&mut session, checkpoint("1", &original, "manual"))?;
    bridge.accept(
        &mut session,
        &Accept {
            scope: scope(),
            mutation_id: "manual".into(),
            head: head("2", &manual.bytes),
        },
    )?;
    let old_png = png(&bridge, &session, "2", &manual.bytes)?;
    let next = external(&manual.bytes, "2")?;
    let binding = session.managed_binding;
    let previous_steps = session
        .active()
        .ok_or("missing_document")?
        .history
        .past_len();
    bridge.freeze(&mut session, true)?;
    bridge.open(&mut session, open("3", &next), &next)?;
    assert_eq!(
        session.managed_binding, binding,
        "higher head cannot replace the canvas owner"
    );
    assert_eq!(session.documents().len(), 1);
    assert_eq!(
        session
            .active()
            .ok_or("missing_document")?
            .history
            .past_len(),
        previous_steps + 1
    );
    assert_eq!(bytes(&session)?, next);
    assert!(
        !session.undo(),
        "canonical refresh must not thaw user input"
    );
    let next_png = png(&bridge, &session, "3", &next)?;
    let before = photocraft_codecs::decode(&old_png)
        .map_err(|_| failure("png_decode"))?
        .to_rgba8();
    let after = photocraft_codecs::decode(&next_png)
        .map_err(|_| failure("png_decode"))?
        .to_rgba8();
    for y in 0..240 {
        for x in 0..320 {
            let a = (y * 320 + x) * 4;
            let b = (x * 240 + (239 - y)) * 4;
            assert_eq!(
                &before[a..a + 4],
                &after[b..b + 4],
                "every clockwise pixel must match"
            );
        }
    }
    // Idempotent notifications add neither an edit nor another document.
    bridge.open(&mut session, open("3", &next), &next)?;
    assert_eq!(
        session
            .active()
            .ok_or("missing_document")?
            .history
            .past_len(),
        previous_steps + 1
    );
    bridge.freeze(&mut session, false)?;
    assert!(
        session.undo(),
        "upstream Undo must restore the previous actual project"
    );
    assert_eq!(bytes(&session)?, manual.bytes);
    assert_eq!(bridge.status(&session)?["dirty"], true);
    let undo = bridge.snapshot(&mut session, checkpoint("3", &next, "undo"))?;
    assert_eq!(undo.bytes, manual.bytes);
    assert_eq!(undo.metadata["base"], json!(head("3", &next)));
    assert_eq!(
        undo.metadata["operation"],
        json!({"kind":"humanCheckpoint"})
    );
    assert_eq!(
        png(&bridge, &session, "3", &next)?,
        next_png,
        "baseline stays at v3 until exact ACK"
    );
    bridge.accept(
        &mut session,
        &Accept {
            scope: scope(),
            mutation_id: "undo".into(),
            head: head("4", &undo.bytes),
        },
    )?;
    assert_eq!(png(&bridge, &session, "4", &undo.bytes)?, old_png);
    assert!(
        session.undo(),
        "the older human history must also survive refresh"
    );
    assert_eq!(bytes(&session)?, original);
    let mut cold_session = Session::new();
    let mut cold = BrowserBridge::default();
    cold.open(&mut cold_session, open("4", &undo.bytes), &undo.bytes)?;
    assert_eq!(bytes(&cold_session)?, undo.bytes);
    assert_eq!(png(&cold, &cold_session, "4", &undo.bytes)?, old_png);
    Ok(())
}

#[test]
fn higher_head_refuses_drafts_pending_receipts_authority_and_foreign_document_without_loss()
-> Result<()> {
    let original = seed()?;
    let next = external(&original, "1")?;
    let mut session = Session::new();
    let mut bridge = BrowserBridge::default();
    bridge.open(&mut session, open("1", &original), &original)?;
    let binding = session.managed_binding;
    session
        .execute("image.imageRotation.flipCanvasHorizontal", json!({}))
        .map_err(|_| failure("manual_transform"))?;
    let draft = bytes(&session)?;
    assert!(bridge.open(&mut session, open("2", &next), &next).is_err());
    assert_eq!(bytes(&session)?, draft);
    let pending = bridge.snapshot(&mut session, checkpoint("1", &original, "pending"))?;
    assert!(bridge.open(&mut session, open("2", &next), &next).is_err());
    assert_eq!(bytes(&session)?, draft);
    bridge.accept(
        &mut session,
        &Accept {
            scope: scope(),
            mutation_id: "pending".into(),
            head: head("2", &pending.bytes),
        },
    )?;
    bridge.freeze(&mut session, true)?;
    let steps = session
        .active()
        .ok_or("missing_document")?
        .history
        .past_len();
    let mut wrong = open("3", &next);
    wrong.scope.actor_id = "different-owner".into();
    assert!(bridge.open(&mut session, wrong, &next).is_err());
    assert!(
        bridge
            .open(&mut session, open("1", &original), &original)
            .is_err()
    );
    let foreign = seed()?;
    assert!(
        bridge
            .open(&mut session, open("3", &foreign), &foreign)
            .is_err()
    );
    assert_eq!(session.managed_binding, binding);
    assert_eq!(
        session
            .active()
            .ok_or("missing_document")?
            .history
            .past_len(),
        steps
    );
    assert_eq!(bytes(&session)?, draft);
    assert!(session.managed_frozen);
    let mut replaced = Session::new();
    std::mem::swap(&mut session, &mut replaced);
    assert!(bridge.open(&mut session, open("3", &next), &next).is_err());
    assert!(bridge.status(&session).is_err());
    Ok(())
}

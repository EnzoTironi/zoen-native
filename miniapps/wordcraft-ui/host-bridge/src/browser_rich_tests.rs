//! Same real Session: human formatting → agent edit/style → exact ACK → Undo/export/cold.
use super::*;
fn request(base: Head, id: &str, action: serde_json::Value) -> Result<Command> {
    serde_json::from_value(
        json!({"mutation":{"scope":scope(),"base":base,"mutationId":id},"action":action}),
    )
    .map_err(|_| failure("fixture_command"))
}
fn range(start: u32, end: u32) -> serde_json::Value {
    json!({"start":{"paragraph":0,"offset":start},"end":{"paragraph":0,"offset":end}})
}
fn ack(
    b: &mut BrowserBridge,
    s: &mut Session,
    id: &str,
    version: &str,
    p: &Proposal,
) -> Result<()> {
    b.accept(
        s,
        &Accept {
            scope: scope(),
            mutation_id: id.into(),
            head: head(version, &p.bytes),
        },
    )?;
    Ok(())
}
#[test]
fn rich_manual_agent_ack_cold_docx_pdf_and_undo_keep_one_actual_document() -> Result<()> {
    let (mut b, mut s, original) = opened("Title\nBody remains intact")?;
    s.doc.core.title = "Owner metadata".into();
    s.run(
        "select.range",
        &json!({"anchor":{"block":0,"off":0},"focus":{"block":0,"off":5}}),
    )
    .map_err(|_| failure("select"))?;
    s.run("format.italic", &json!({"value":true}))
        .map_err(|_| failure("manual_format"))?;
    let human = b.snapshot(&mut s, mutation(&original, "human-rich"))?;
    ack(&mut b, &mut s, "human-rich", "2", &human)?;
    let manual = s.doc.clone();
    let depth = s.edit_snapshot().undo_depth();
    let document_id = s.document_id();
    let command = request(
        head("2", &human.bytes),
        "agent-format",
        json!({"command":"range.format","params":{"range":range(0,5),"format":{"paragraphStyle":"Heading1","bold":true,"size":18,"color":"204080","alignment":"center","spaceAfter":12,"lineSpacing":1.5}}}),
    )?;
    let styled = b.execute(&mut s, command.clone())?;
    assert_eq!(
        s.document_id(),
        document_id,
        "agent must never replace the visible Session"
    );
    assert_eq!(
        s.edit_snapshot().undo_depth(),
        depth + 1,
        "all formatting is one Undo"
    );
    let styled_model = s.doc.clone();
    assert_eq!(profile::parse(&styled.bytes)?, styled_model);
    assert_eq!(styled_model.core.title, "Owner metadata");
    assert_eq!(styled_model.body[1], manual.body[1]);
    assert_eq!(b.execute(&mut s, command)?.bytes, styled.bytes);
    assert_eq!(
        s.edit_snapshot().undo_depth(),
        depth + 1,
        "retry must not edit twice"
    );
    assert_eq!(
        b.export(
            &s,
            &Export {
                scope: scope(),
                head: head("2", &human.bytes),
                format: ExportFormat::Docx
            }
        )?
        .bytes,
        human.bytes
    );
    assert!(
        b.accept(
            &mut s,
            &Accept {
                scope: scope(),
                mutation_id: "wrong".into(),
                head: head("3", &styled.bytes)
            }
        )
        .is_err()
    );
    assert_eq!(s.doc, styled_model);
    ack(&mut b, &mut s, "agent-format", "3", &styled)?;
    let edit = request(
        head("3", &styled.bytes),
        "agent-edit",
        json!({"command":"text.edit","params":{"range":range(0,5),"text":"Edited title"}}),
    )?;
    let edited = b.execute(&mut s, edit)?;
    ack(&mut b, &mut s, "agent-edit", "4", &edited)?;
    let final_model = s.doc.clone();
    assert_eq!(final_model.body[1], manual.body[1]);
    assert_eq!(final_model.core, styled_model.core);
    let exported = b.export(
        &s,
        &Export {
            scope: scope(),
            head: head("4", &edited.bytes),
            format: ExportFormat::Docx,
        },
    )?;
    assert_eq!(exported.bytes, edited.bytes);
    let pdf = b.export(
        &s,
        &Export {
            scope: scope(),
            head: head("4", &edited.bytes),
            format: ExportFormat::Pdf,
        },
    )?;
    assert!(pdf.bytes.starts_with(b"%PDF-"));
    let mut cold = BrowserBridge::default();
    let mut next = Session::new(Document::new());
    cold.open(
        &mut next,
        Open {
            scope: scope(),
            head: head("4", &exported.bytes),
        },
        &exported.bytes,
    )?;
    assert_eq!(
        next.doc, final_model,
        "cold reopen keeps every rich property"
    );
    s.run("edit.undo", &json!({}))
        .map_err(|_| failure("undo_edit"))?;
    assert_eq!(
        s.doc, styled_model,
        "one Undo reverses just the reviewed text edit"
    );
    s.run("edit.undo", &json!({}))
        .map_err(|_| failure("undo_format"))?;
    assert_eq!(
        s.doc, manual,
        "next Undo reverses the entire reviewed format patch"
    );
    s.run("edit.redo", &json!({}))
        .map_err(|_| failure("redo_format"))?;
    assert_eq!(s.doc, styled_model);
    assert_eq!(
        b.export(
            &s,
            &Export {
                scope: scope(),
                head: head("4", &edited.bytes),
                format: ExportFormat::Docx
            }
        )?
        .bytes,
        edited.bytes,
        "unsaved Undo/Redo never changes the accepted export"
    );
    Ok(())
}
#[test]
fn invalid_unicode_stale_scope_and_lossy_serialization_roll_back_actual_session() -> Result<()> {
    let (mut b, mut s, bytes) = opened("Olá title")?;
    s.run("caret.end", &json!({}))
        .map_err(|_| failure("caret"))?;
    s.run("format.italic", &json!({"value":true}))
        .map_err(|_| failure("pending_format"))?;
    let before = s.doc.clone();
    let sel = s.sel.clone();
    let pending = s.pending.clone();
    let depth = s.edit_snapshot().undo_depth();
    let command = s.last_command.clone();
    for (id, action) in [
        (
            "unicode",
            json!({"command":"text.edit","params":{"range":range(3,4),"text":"X"}}),
        ),
        (
            "lossy",
            json!({"command":"range.format","params":{"range":range(0,4),"format":{"bold":true,"spaceBefore":0.123}}}),
        ),
        (
            "style",
            json!({"command":"range.format","params":{"range":range(0,4),"format":{"paragraphStyle":"nonexistent"}}}),
        ),
    ] {
        assert!(
            b.execute(&mut s, request(head("1", &bytes), id, action)?)
                .is_err(),
            "{id}"
        );
        assert_eq!(s.doc, before, "{id}");
        assert_eq!(s.sel, sel, "{id}");
        assert_eq!(s.pending, pending, "{id}");
        assert_eq!(s.last_command, command, "{id}");
        assert_eq!(s.edit_snapshot().undo_depth(), depth, "{id}");
        assert_eq!(b.status(&s)?["pendingMutationId"], Value::Null);
        assert!(!s.managed_frozen);
    }
    let action =
        json!({"command":"range.format","params":{"range":range(0,4),"format":{"bold":true}}});
    let mut foreign = request(head("1", &bytes), "foreign", action.clone())?;
    foreign.mutation.scope.artifact_id = "other".into();
    assert!(b.execute(&mut s, foreign).is_err());
    assert!(
        b.execute(&mut s, request(head("0", &bytes), "stale", action)?)
            .is_err()
    );
    assert_eq!(s.doc, before);
    assert_eq!(s.edit_snapshot().undo_depth(), depth);
    Ok(())
}

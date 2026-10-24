//! Real upstream model/commands and strict writer; no canonical authority is simulated here.
use super::*;
use crate::browser::BrowserBridge;
use wordcraft_doc::Document;

pub(crate) fn scope() -> Scope {
    Scope {
        artifact_id: "document-fixture".into(),
        actor_id: "human-fixture".into(),
        actor_kind: ActorKind::Human,
    }
}
pub(crate) fn head(version: &str, bytes: &[u8]) -> Head {
    Head {
        version: version.into(),
        sha256: hash(bytes),
    }
}
pub(crate) fn opened(text: &str) -> Result<(BrowserBridge, Session, Vec<u8>)> {
    let bytes = profile::encode(text)?;
    let mut s = Session::new(Document::new());
    let mut b = BrowserBridge::default();
    b.open(
        &mut s,
        Open {
            scope: scope(),
            head: head("1", &bytes),
        },
        &bytes,
    )?;
    Ok((b, s, bytes))
}
pub(crate) fn mutation(bytes: &[u8], id: &str) -> Mutation {
    Mutation {
        scope: scope(),
        base: head("1", bytes),
        mutation_id: id.into(),
    }
}
fn insert(s: &mut Session, text: &str) -> Result<()> {
    s.run("select.all", &json!({"whole":true}))
        .map_err(|_| failure("fixture_selection"))?;
    s.run("text.insert", &json!({"text":text,"raw":true}))
        .map_err(|_| failure("fixture_edit"))?;
    Ok(())
}
#[test]
fn actual_manual_model_ack_preserves_undo_and_exact_accepted_export() -> Result<()> {
    let (mut b, mut s, bytes) = opened("Original")?;
    s.run("select.all", &json!({"whole":true}))
        .map_err(|_| failure("fixture_selection"))?;
    // Real Paste Text maps transferred LF to paragraph separators. Raw typed LF
    // is an upstream soft break and is deliberately not normalized on checkpoint.
    s.run("edit.pasteText", &json!({"text":"Manual Olá\t日本語\n"}))
        .map_err(|_| failure("fixture_paste"))?;
    assert_eq!(b.status(&s)?["dirty"], true);
    let proposal = b.snapshot(&mut s, mutation(&bytes, "manual-1"))?;
    assert_eq!(
        profile::parse(&proposal.bytes)?.plain_text(StoryRef::Body),
        "Manual Olá\t日本語\n"
    );
    b.freeze(&mut s, true)?;
    assert!(
        s.run("text.insert", &json!({"text":"hidden","raw":true}))
            .is_err()
    );
    assert_eq!(
        b.snapshot(&mut s, mutation(&bytes, "manual-1"))?.bytes,
        proposal.bytes
    );
    assert_eq!(
        b.export(
            &s,
            &Export {
                scope: scope(),
                head: head("1", &bytes),
                format: ExportFormat::Docx
            }
        )?
        .bytes,
        bytes
    );
    b.accept(
        &mut s,
        &Accept {
            scope: scope(),
            mutation_id: "manual-1".into(),
            head: head("2", &proposal.bytes),
        },
    )?;
    assert!(s.managed_frozen, "ACK cannot undo the host's pause");
    b.freeze(&mut s, false)?;
    s.run("edit.undo", &json!({}))
        .map_err(|_| failure("fixture_undo"))?;
    assert_eq!(s.doc.plain_text(StoryRef::Body), "Original");
    assert_eq!(b.status(&s)?["dirty"], true);
    assert_eq!(
        b.export(
            &s,
            &Export {
                scope: scope(),
                head: head("2", &proposal.bytes),
                format: ExportFormat::Docx
            }
        )?
        .bytes,
        proposal.bytes
    );
    let (mut cold, mut next, _) = opened("Unrelated")?;
    // Opening another actual host requires a fresh Session, never adopting an old edit buffer.
    assert!(
        cold.open(
            &mut next,
            Open {
                scope: scope(),
                head: head("1", &bytes)
            },
            &bytes
        )
        .is_err()
    );
    cold = BrowserBridge::default();
    next = Session::new(Document::new());
    cold.open(
        &mut next,
        Open {
            scope: scope(),
            head: head("2", &proposal.bytes),
        },
        &proposal.bytes,
    )?;
    assert_eq!(next.doc.plain_text(StoryRef::Body), "Manual Olá\t日本語\n");
    Ok(())
}
#[test]
fn rich_model_cannot_be_silently_normalized_and_manual_draft_blocks_agent() -> Result<()> {
    let (mut b, mut s, bytes) = opened("Original")?;
    s.doc.core.title = "not in accepted plain profile".into();
    let retained = s.doc.clone();
    assert!(b.snapshot(&mut s, mutation(&bytes, "rich")).is_err());
    assert_eq!(s.doc, retained);
    assert!(!s.managed_frozen);
    assert_eq!(
        b.execute(
            &mut s,
            Command {
                mutation: mutation(&bytes, "agent"),
                action: Action::TextReplace(TextReplace {
                    text: "must not replace draft".into()
                })
            }
        )
        .err(),
        Some(failure("local_changes"))
    );
    assert!(s.run("format.bold", &json!({})).is_err());
    assert!(s.run("file.open", &json!({"path":"untrusted"})).is_err());
    assert_eq!(s.doc, retained);
    Ok(())
}
#[test]
fn actual_replaced_session_foreign_owner_and_late_ack_never_adopt_visible_content() -> Result<()> {
    let (mut b, mut s, bytes) = opened("Original")?;
    insert(&mut s, "Pending")?;
    let proposal = b.snapshot(&mut s, mutation(&bytes, "first"))?;
    let mut wrong = Accept {
        scope: scope(),
        mutation_id: "wrong".into(),
        head: head("2", &proposal.bytes),
    };
    assert!(b.accept(&mut s, &wrong).is_err());
    wrong.mutation_id = "first".into();
    wrong.scope.actor_id = "other".into();
    assert!(b.accept(&mut s, &wrong).is_err());
    let (mut other, mut other_session, _) = opened("Other")?;
    assert_eq!(
        b.status(&other_session).err(),
        Some(failure("document_changed"))
    );
    assert!(
        other
            .open(
                &mut s,
                Open {
                    scope: scope(),
                    head: head("1", &bytes)
                },
                &bytes
            )
            .is_err()
    );
    other_session.set_document(Document::from_text("Replaced"));
    assert!(other.status(&other_session).is_err());
    assert_eq!(s.doc.plain_text(StoryRef::Body), "Pending");
    Ok(())
}
#[test]
fn exact_agent_proposal_visible_document_replay_and_ack_use_one_head() -> Result<()> {
    let (mut b, mut s, bytes) = opened("Original")?;
    let request = Command {
        mutation: mutation(&bytes, "agent"),
        action: Action::TextReplace(TextReplace {
            text: "Agent revision".into(),
        }),
    };
    let proposal = b.execute(&mut s, request.clone())?;
    assert_eq!(s.doc.plain_text(StoryRef::Body), "Agent revision");
    assert!(s.managed_frozen);
    assert_eq!(b.execute(&mut s, request.clone())?.bytes, proposal.bytes);
    b.accept(
        &mut s,
        &Accept {
            scope: scope(),
            mutation_id: "agent".into(),
            head: head("2", &proposal.bytes),
        },
    )?;
    assert!(!s.managed_frozen);
    assert_eq!(
        b.execute(&mut s, request)?.metadata["acceptedHead"],
        json!(head("2", &proposal.bytes))
    );
    Ok(())
}

#[test]
fn typed_soft_break_refuses_before_mutation_while_real_paste_paragraphs_stay_supported()
-> Result<()> {
    let (mut b, mut s, bytes) = opened("Original")?;
    let original = s.doc.clone();
    assert!(
        s.run(
            "text.insert",
            &json!({"text":"different\nsemantics","raw":true})
        )
        .is_err()
    );
    assert_eq!(s.doc, original);
    assert!(!s.managed_frozen);
    assert_eq!(b.status(&s)?["dirty"], false);
    assert_eq!(
        b.export(
            &s,
            &Export {
                scope: scope(),
                head: head("1", &bytes),
                format: ExportFormat::Docx
            }
        )?
        .bytes,
        bytes
    );
    s.run("select.all", &json!({"whole":true}))
        .map_err(|_| failure("fixture_selection"))?;
    s.run("edit.pasteText", &json!({"text":"First\nSecond"}))
        .map_err(|_| failure("fixture_paste"))?;
    assert_eq!(s.doc.body.len(), 2);
    assert_eq!(s.doc.plain_text(StoryRef::Body), "First\nSecond");
    let proposal = b.snapshot(&mut s, mutation(&bytes, "paste-paragraphs"))?;
    assert_eq!(
        profile::parse(&proposal.bytes)?.plain_text(StoryRef::Body),
        "First\nSecond"
    );
    Ok(())
}

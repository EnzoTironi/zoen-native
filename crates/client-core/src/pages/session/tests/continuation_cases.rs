//! Complete continuation scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn lost_reply_empty_diff_undo_and_redo_continue_own_branch(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let db = Database::new()?;
    let mut f = Fixture::at(&db.path())?;
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let edit = text_edit(&read, "abcdef repeated 😀");
    let first = apply(&f, &mut s, &read, "typed", std::slice::from_ref(&edit)).unwrap();
    drop(s);
    f = f.reopen();
    s = f.session();
    let undone = apply(&f, &mut s, &read, "undo", &[]).unwrap();
    assert_eq!(undone.current.view, read.view);
    assert_eq!(undone.applied_content_hash, read.content_hash);
    assert!(journal(&s).has_draft);
    assert!(journal(&s).commit.is_none());
    let history = plan_doc(&s.live).unwrap().full_update().to_json().unwrap();
    let receipts = serde_json::to_value(&journal(&s).receipts).unwrap();
    let snapshot = serde_json::to_value(&journal(&s).snapshot).unwrap();
    assert_eq!(
        s.commit(&mut f.engine, "accepted local undo").unwrap(),
        CommitStatus::Unchanged
    );
    assert!(!journal(&s).has_draft);
    assert_eq!(f.engine.store.outbox_len().unwrap(), 0);
    assert_eq!(
        serde_json::to_value(&journal(&s).receipts).unwrap(),
        receipts
    );
    assert_eq!(
        serde_json::to_value(&journal(&s).snapshot).unwrap(),
        snapshot
    );
    let ciphertext = f.cipher().unwrap();
    drop(s);
    f = f.reopen();
    s = f.session();
    assert!(!journal(&s).has_draft);
    assert_eq!(
        plan_doc(&s.live).unwrap().full_update().to_json().unwrap(),
        history
    );
    assert_eq!(
        serde_json::to_value(&journal(&s).receipts).unwrap(),
        receipts
    );
    assert_eq!(
        serde_json::to_value(&journal(&s).snapshot).unwrap(),
        snapshot
    );
    let first_retry = apply(&f, &mut s, &read, "typed", std::slice::from_ref(&edit)).unwrap();
    assert!(first_retry.replayed);
    assert_eq!(first_retry.applied_edit_context, first.applied_edit_context);
    assert_eq!(first_retry.current.view, read.view);
    assert!(!first_retry.current.unsaved);
    assert_eq!(f.cipher().unwrap(), ciphertext);
    let redone = apply(&f, &mut s, &read, "redo", std::slice::from_ref(&edit)).unwrap();
    assert_eq!(redone.applied_content_hash, first.applied_content_hash);
    let first_retry = apply(&f, &mut s, &read, "typed", &[edit]).unwrap();
    assert!(first_retry.replayed);
    assert_eq!(first_retry.applied_edit_context, first.applied_edit_context);
    assert_eq!(
        blocks(&redone.current)
            .iter()
            .filter(|b| b.plain_text() == "abcdef repeated 😀")
            .count(),
        1
    );
    drop(s);
    drop(f);

    Ok(())
}

pub(super) fn continuation_does_not_replace_unseen_remote_fields_or_blocks(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    Ok(for remote_first in [false, true] {
        let mut f = Fixture::new()?;
        let mut s = f.session();
        let read = s.read(&f.engine).unwrap();
        let line = line(&read);
        let mut remote = f
            .engine
            .plan_timeline(&f.item)
            .unwrap()
            .document
            .unwrap()
            .fork()
            .unwrap();
        let rev = remote.revision().unwrap();
        let mut cost = line.clone();
        if let PlanBlockContent::Line { cost_cents, .. } = &mut cost.content {
            *cost_cents = 909;
        }
        let summary = PlanBlockEdit {
            id: blocks(&read)[1].id.clone(),
            content: PlanBlockContent::Summary,
            text: "remote summary".into(),
            spans: vec![Span {
                start: 0,
                end: 6,
                key: "b".into(),
                value: String::new(),
            }],
        };
        let prose = PlanBlockEdit {
            id: "remote-prose".into(),
            content: PlanBlockContent::Prose {
                kind: Kind::Paragraph,
            },
            text: "unseen block".into(),
            spans: vec![],
        };
        let mut order = order_read(&read);
        order.push(prose.id.clone());
        remote
            .apply_from(&rev, &order, &[cost, summary, prose])
            .unwrap();
        let event = f.signed_as(&f.owner, f.version(ItemContent::Page(f.page(&remote)?)))?;
        if remote_first {
            deliver(&mut f.engine, &event, Ingest::Applied);
        }
        apply(&f, &mut s, &read, "first", &[text_edit(&read, "own first")]).unwrap();
        if !remote_first {
            deliver(&mut f.engine, &event, Ingest::Applied);
        }
        let accepted = apply(
            &f,
            &mut s,
            &read,
            "second",
            &[text_edit(&read, "own second")],
        )
        .unwrap();
        assert!(blocks(&accepted.current)
            .iter()
            .any(|b| b.plain_text() == "unseen block"));
        assert!(blocks(&accepted.current)
            .iter()
            .any(|b| b.plain_text() == "remote summary" && b.runs.iter().any(|r| r.attrs.bold)));
        let line = blocks(&accepted.current)
            .iter()
            .find(|b| b.id == line.id)
            .unwrap();
        assert_eq!(line.plain_text(), "own second");
        assert!(matches!(
            line.content,
            PlanBlockContent::Line {
                cost_cents: 909,
                ..
            }
        ));
    })
}

pub(super) fn continuation_removes_own_new_block_without_reusing_history(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = Fixture::new()?;
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let mut order = order_read(&read);
    order.push("own-prose".into());
    let prose = PlanBlockEdit {
        id: "own-prose".into(),
        content: PlanBlockContent::Prose {
            kind: Kind::Paragraph,
        },
        text: "own insertion".into(),
        spans: vec![],
    };
    s.apply_from(
        &f.engine,
        "insert",
        &read.edit_context,
        DocumentEdit::Plan {
            order: &order,
            changed: std::slice::from_ref(&prose),
        },
    )
    .unwrap();
    let removed = apply(&f, &mut s, &read, "remove", &[]).unwrap();
    assert_eq!(removed.current.view, read.view);
    let current = s.read(&f.engine).unwrap();
    let before = exact(&f, &s);
    let error = s
        .apply_from(
            &f.engine,
            "historical-redo",
            &current.edit_context,
            DocumentEdit::Plan {
                order: &order,
                changed: &[prose],
            },
        )
        .err()
        .unwrap();
    assert!(error.to_string().contains("historical plan block ID"));
    no_change(&f, &s, &before);
    // Authenticated legacy negatives remain readable but do not become ordinary restores.
    let mut negative = changed(
        blocks(&current)
            .iter()
            .find(|b| matches!(b.content, PlanBlockContent::Line { cost_cents: -7, .. }))
            .unwrap(),
    );
    if let PlanBlockContent::Line { cost_cents, .. } = &mut negative.content {
        *cost_cents = 9;
    }
    let positive = apply(&f, &mut s, &current, "positive", &[negative]).unwrap();
    let original = changed(
        blocks(&current)
            .iter()
            .find(|b| matches!(b.content, PlanBlockContent::Line { cost_cents: -7, .. }))
            .unwrap(),
    );
    let before = exact(&f, &s);
    let error = s
        .apply_from(
            &f.engine,
            "negative-restore",
            &positive.applied_edit_context,
            DocumentEdit::Plan {
                order: &order_read(&current),
                changed: &[original],
            },
        )
        .err()
        .unwrap();
    assert!(error.to_string().contains("new cost cannot be negative"));
    no_change(&f, &s, &before);

    Ok(())
}

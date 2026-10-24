//! Real `WordApp` input while an immutable manual checkpoint awaits its exact receipt.
use super::*;
fn type_text(h: &mut Harness, text: &str) {
    h.input_mut().events.push(Event::Text(text.into()));
    h.run_steps(2);
}
#[test]
fn delayed_manual_ack_retains_real_input_without_rewriting_original_proposal() -> Result<()> {
    let (mut h, mut bridge, original) = harness()?;
    click_canvas(&mut h)?;
    key(&mut h, Key::A, Modifiers::COMMAND);
    type_text(&mut h, "Checkpoint");
    let pending = bridge.snapshot(
        &mut h.state_mut().session,
        mutation(&original, "manual-inflight"),
    )?;
    type_text(&mut h, " and newer input");
    key(&mut h, Key::Enter, Modifiers::NONE);
    type_text(&mut h, "Second line");
    let expected = "Checkpoint and newer input\nSecond line";
    assert_eq!(
        h.state().session.doc.plain_text(StoryRef::Body),
        expected,
        "an autosave receipt delay must not discard real keyboard input"
    );
    assert_original_pending(&mut h, &mut bridge, &original, &pending, expected)?;
    let latest = accept_original_and_capture_latest(&mut h, &mut bridge, &pending, expected)?;
    accept_latest_undo_and_reopen(&mut h, &mut bridge, &pending, &latest, expected)?;
    Ok(())
}
fn assert_original_pending(
    h: &mut Harness,
    bridge: &mut BrowserBridge,
    original: &[u8],
    pending: &Proposal,
    expected: &str,
) -> Result<()> {
    let original_retry = bridge.snapshot(
        &mut h.state_mut().session,
        mutation(original, "manual-inflight"),
    )?;
    assert_eq!(original_retry.bytes, pending.bytes);
    assert_eq!(original_retry.metadata, pending.metadata);
    assert_eq!(
        profile::parse(&pending.bytes)?.plain_text(StoryRef::Body),
        "Checkpoint"
    );
    assert!(
        bridge
            .snapshot(
                &mut h.state_mut().session,
                mutation(original, "second-before-ack")
            )
            .is_err()
    );
    assert_eq!(
        bridge
            .export(
                &h.state().session,
                &Export {
                    scope: scope(),
                    head: head("1", original),
                    format: ExportFormat::Docx
                }
            )?
            .bytes,
        original
    );
    assert!(
        bridge
            .accept(
                &mut h.state_mut().session,
                &Accept {
                    scope: scope(),
                    mutation_id: "foreign-receipt".into(),
                    head: head("2", &pending.bytes),
                }
            )
            .is_err()
    );
    assert_eq!(h.state().session.doc.plain_text(StoryRef::Body), expected);
    assert!(
        bridge
            .open(
                &mut h.state_mut().session,
                Open {
                    scope: scope(),
                    head: head("1", original)
                },
                original,
            )
            .is_err(),
        "reopen cannot replace an outstanding proposal or its newer local draft"
    );
    assert_eq!(h.state().session.doc.plain_text(StoryRef::Body), expected);
    Ok(())
}
fn accept_original_and_capture_latest(
    h: &mut Harness,
    bridge: &mut BrowserBridge,
    pending: &Proposal,
    expected: &str,
) -> Result<Proposal> {
    bridge.accept(
        &mut h.state_mut().session,
        &Accept {
            scope: scope(),
            mutation_id: "manual-inflight".into(),
            head: head("2", &pending.bytes),
        },
    )?;
    assert_eq!(h.state().session.doc.plain_text(StoryRef::Body), expected);
    assert_eq!(bridge.status(&h.state().session)?["dirty"], true);
    assert_eq!(
        bridge
            .export(
                &h.state().session,
                &Export {
                    scope: scope(),
                    head: head("2", &pending.bytes),
                    format: ExportFormat::Docx
                }
            )?
            .bytes,
        pending.bytes
    );
    let latest = bridge.snapshot(
        &mut h.state_mut().session,
        Mutation {
            scope: scope(),
            base: head("2", &pending.bytes),
            mutation_id: "retained-input".into(),
        },
    )?;
    assert_eq!(
        profile::parse(&latest.bytes)?.plain_text(StoryRef::Body),
        expected
    );
    Ok(latest)
}
fn accept_latest_undo_and_reopen(
    h: &mut Harness,
    bridge: &mut BrowserBridge,
    pending: &Proposal,
    latest: &Proposal,
    expected: &str,
) -> Result<()> {
    assert!(
        bridge
            .accept(
                &mut h.state_mut().session,
                &Accept {
                    scope: scope(),
                    mutation_id: "manual-inflight".into(),
                    head: head("2", &pending.bytes),
                },
            )
            .is_err(),
        "a delayed duplicate receipt cannot consume the next proposal"
    );
    assert_eq!(
        bridge
            .snapshot(
                &mut h.state_mut().session,
                Mutation {
                    scope: scope(),
                    base: head("2", &pending.bytes),
                    mutation_id: "retained-input".into(),
                },
            )?
            .bytes,
        latest.bytes
    );
    bridge.accept(
        &mut h.state_mut().session,
        &Accept {
            scope: scope(),
            mutation_id: "retained-input".into(),
            head: head("3", &latest.bytes),
        },
    )?;
    let before_undo = bridge.export(
        &h.state().session,
        &Export {
            scope: scope(),
            head: head("3", &latest.bytes),
            format: ExportFormat::Docx,
        },
    )?;
    key(h, Key::Z, Modifiers::COMMAND);
    assert_ne!(
        h.state().session.doc.plain_text(StoryRef::Body),
        expected,
        "the actual local Undo history survives both receipts"
    );
    assert_eq!(
        bridge
            .export(
                &h.state().session,
                &Export {
                    scope: scope(),
                    head: head("3", &latest.bytes),
                    format: ExportFormat::Docx
                }
            )?
            .bytes,
        before_undo.bytes
    );
    let mut cold = BrowserBridge::default();
    let mut session = Session::new(wordcraft_doc::Document::new());
    cold.open(
        &mut session,
        Open {
            scope: scope(),
            head: head("3", &before_undo.bytes),
        },
        &before_undo.bytes,
    )?;
    assert_eq!(
        session.doc.plain_text(StoryRef::Body),
        expected,
        "a fresh renderer opens only the actual accepted DOCX, not later unsaved Undo"
    );
    Ok(())
}

#[test]
fn actual_agent_proposal_stays_frozen_and_receipt_cannot_remove_host_pause() -> Result<()> {
    let (mut h, mut bridge, bytes) = harness()?;
    click_canvas(&mut h)?;
    let proposal = bridge.execute(
        &mut h.state_mut().session,
        Command {
            mutation: mutation(&bytes, "agent-inflight"),
            action: Action::TextReplace(TextReplace {
                text: "Agent text".into(),
            }),
        },
    )?;
    bridge.freeze(&mut h.state_mut().session, false)?;
    type_text(&mut h, "must not enter agent proposal");
    key(&mut h, Key::Enter, Modifiers::NONE);
    assert!(h.state().session.managed_frozen);
    assert_eq!(
        h.state().session.doc.plain_text(StoryRef::Body),
        "Agent text"
    );
    bridge.freeze(&mut h.state_mut().session, true)?;
    bridge.accept(
        &mut h.state_mut().session,
        &Accept {
            scope: scope(),
            mutation_id: "agent-inflight".into(),
            head: head("2", &proposal.bytes),
        },
    )?;
    assert!(h.state().session.managed_frozen);
    type_text(&mut h, "still paused");
    assert_eq!(
        h.state().session.doc.plain_text(StoryRef::Body),
        "Agent text"
    );
    bridge.freeze(&mut h.state_mut().session, false)?;
    click_canvas(&mut h)?;
    key(&mut h, Key::A, Modifiers::COMMAND);
    type_text(&mut h, "Human after receipt");
    assert_eq!(
        h.state().session.doc.plain_text(StoryRef::Body),
        "Human after receipt"
    );
    Ok(())
}

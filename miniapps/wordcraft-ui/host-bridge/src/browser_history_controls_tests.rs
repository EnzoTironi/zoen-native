//! Actual managed WordApp history controls, real touch input and anchored host checkpoints.
use super::*;
use egui_kittest::kittest::{NodeT as _, Queryable as _};

fn control(h: &Harness, label: &str) -> Result<(egui::Rect, bool)> {
    let label = wordcraft_ui_egui::i18n::t(label);
    let node = h
        .query_by_label(label)
        .ok_or_else(|| failure("managed_history_control_missing"))?;
    let rect = node.rect();
    assert!(
        rect.width() >= 44.0 && rect.height() >= 44.0,
        "history controls need touch-sized targets"
    );
    Ok((rect, !node.accesskit_node().is_disabled()))
}
fn tap(h: &mut Harness, label: &str) -> Result<()> {
    let (rect, _) = control(h, label)?;
    let at = rect.center();
    for pressed in [true, false] {
        h.input_mut().time = Some(h.ctx.input(|input| input.time) + 0.1);
        h.input_mut().events.extend([
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
            Event::Touch {
                device_id: egui::TouchDeviceId(0),
                id: egui::TouchId::from(27),
                phase: if pressed {
                    egui::TouchPhase::Start
                } else {
                    egui::TouchPhase::End
                },
                pos: at,
                force: None,
            },
        ]);
        h.step();
    }
    h.run_steps(2);
    Ok(())
}
fn agent_revision(bytes: &[u8]) -> Result<Proposal> {
    let mut actor = scope();
    actor.actor_kind = ActorKind::Agent;
    actor.actor_id = "history-reviewed-agent".into();
    let mut worker = HostBridge::open(
        Open {
            scope: actor.clone(),
            head: head("1", bytes),
        },
        bytes,
    )?;
    let command: Command = serde_json::from_value(json!({"mutation":{"scope":actor,"base":head("1",bytes),"mutationId":"history-agent"},"action":{"command":"range.format","params":{"range":{"start":{"paragraph":0,"offset":0},"end":{"paragraph":0,"offset":5}},"format":{"alignment":"center"}}}})).map_err(|_|failure("fixture_command"))?;
    worker.execute(command)
}
#[test]
fn managed_touch_undo_after_agent_refresh_preserves_session_and_anchors_new_checkpoint()
-> Result<()> {
    let (mut h, mut bridge, original) = harness()?;
    let manual = h.state().session.doc.clone();
    let binding = h.state().session.managed_binding;
    let document = h.state().session.document_id();
    let proposal = agent_revision(&original)?;
    bridge.open(
        &mut h.state_mut().session,
        Open {
            scope: scope(),
            head: head("2", &proposal.bytes),
        },
        &proposal.bytes,
    )?;
    h.run_steps(4);
    assert_eq!(h.state().session.doc, profile::parse(&proposal.bytes)?);
    assert!(
        control(&h, "Undo")?.1,
        "accepted remote edit must have a visible enabled Undo"
    );
    assert!(!control(&h, "Redo")?.1);
    tap(&mut h, "Undo")?;
    assert_eq!(h.state().session.doc, manual);
    assert_eq!(h.state().session.managed_binding, binding);
    assert_eq!(h.state().session.document_id(), document);
    assert!(control(&h, "Redo")?.1);
    assert_eq!(
        bridge.status(&h.state().session)?["head"],
        json!(head("2", &proposal.bytes))
    );
    assert_eq!(bridge.status(&h.state().session)?["dirty"], true);
    assert!(
        bridge
            .snapshot(&mut h.state_mut().session, mutation(&original, "old-base"))
            .is_err()
    );
    let undo = bridge.snapshot(
        &mut h.state_mut().session,
        Mutation {
            scope: scope(),
            base: head("2", &proposal.bytes),
            mutation_id: "touch-undo".into(),
        },
    )?;
    assert_eq!(profile::parse(&undo.bytes)?, manual);
    bridge.accept(
        &mut h.state_mut().session,
        &Accept {
            scope: scope(),
            mutation_id: "touch-undo".into(),
            head: head("3", &undo.bytes),
        },
    )?;
    assert_eq!(
        bridge.status(&h.state().session)?["head"],
        json!(head("3", &undo.bytes))
    );
    assert_eq!(bridge.status(&h.state().session)?["dirty"], false);
    tap(&mut h, "Redo")?;
    assert_eq!(h.state().session.doc, profile::parse(&proposal.bytes)?);
    assert_eq!(h.state().session.document_id(), document);
    Ok(())
}
#[test]
fn managed_history_controls_respect_empty_history_pending_agent_and_host_pause() -> Result<()> {
    let (mut h, mut bridge, original) = harness()?;
    assert!(!control(&h, "Undo")?.1);
    assert!(!control(&h, "Redo")?.1);
    let command: Command = serde_json::from_value(json!({"mutation":{"scope":scope(),"base":head("1",&original),"mutationId":"pending-history"},"action":{"command":"text.replace","params":{"text":"Pending reviewed change"}}})).map_err(|_|failure("fixture_command"))?;
    let pending = bridge.execute(&mut h.state_mut().session, command)?;
    h.run_steps(3);
    assert!(!control(&h, "Undo")?.1);
    let model = h.state().session.doc.clone();
    tap(&mut h, "Undo")?;
    assert_eq!(h.state().session.doc, model);
    bridge.accept(
        &mut h.state_mut().session,
        &Accept {
            scope: scope(),
            mutation_id: "pending-history".into(),
            head: head("2", &pending.bytes),
        },
    )?;
    bridge.freeze(&mut h.state_mut().session, true)?;
    h.run_steps(3);
    assert!(!control(&h, "Undo")?.1);
    tap(&mut h, "Undo")?;
    assert_eq!(h.state().session.doc, model);
    bridge.freeze(&mut h.state_mut().session, false)?;
    h.run_steps(2);
    assert!(control(&h, "Undo")?.1);
    Ok(())
}

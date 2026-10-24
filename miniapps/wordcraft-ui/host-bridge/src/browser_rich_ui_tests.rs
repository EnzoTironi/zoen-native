//! Genuine WordApp keyboard input and canonical agent refresh share the same model.
use super::*;
#[test]
fn actual_wordapp_typing_agent_style_refresh_and_keyboard_undo_preserve_current_head() -> Result<()>
{
    let (mut h, mut bridge, original) = harness()?;
    click_canvas(&mut h)?;
    key(&mut h, Key::A, Modifiers::COMMAND);
    h.input_mut().events.push(Event::Text("Title".into()));
    h.run_steps(2);
    key(&mut h, Key::Enter, Modifiers::NONE);
    h.input_mut()
        .events
        .push(Event::Text("Body remains intact".into()));
    h.run_steps(2);
    let human = bridge.snapshot(&mut h.state_mut().session, mutation(&original, "ui-human"))?;
    bridge.accept(
        &mut h.state_mut().session,
        &Accept {
            scope: scope(),
            mutation_id: "ui-human".into(),
            head: head("2", &human.bytes),
        },
    )?;
    let manual = h.state().session.doc.clone();
    let mut actor = scope();
    actor.actor_kind = ActorKind::Agent;
    actor.actor_id = "ui-reviewed-agent".into();
    let mut worker = HostBridge::open(
        Open {
            scope: actor.clone(),
            head: head("2", &human.bytes),
        },
        &human.bytes,
    )?;
    let command:Command=serde_json::from_value(json!({"mutation":{"scope":actor,"base":head("2",&human.bytes),"mutationId":"ui-agent"},"action":{"command":"range.format","params":{"range":{"start":{"paragraph":0,"offset":0},"end":{"paragraph":0,"offset":5}},"format":{"bold":true,"size":18,"alignment":"center"}}}})).map_err(|_|failure("command"))?;
    let proposed = worker.execute(command)?;
    worker.accept(&Accept {
        scope: actor,
        mutation_id: "ui-agent".into(),
        head: head("3", &proposed.bytes),
    })?;
    bridge.open(
        &mut h.state_mut().session,
        Open {
            scope: scope(),
            head: head("3", &proposed.bytes),
        },
        &proposed.bytes,
    )?;
    h.run_steps(3);
    assert!(
        !h.state().canvas.page_rects.is_empty(),
        "actual upstream UI must lay out the rich revision"
    );
    assert!(h.state().session.doc == profile::parse(&proposed.bytes)?);
    key(&mut h, Key::Z, Modifiers::COMMAND);
    assert!(
        h.state().session.doc == manual,
        "the actual keyboard shortcut undoes the received style edit"
    );
    assert_eq!(
        bridge.status(&h.state().session)?["head"],
        json!(head("3", &proposed.bytes))
    );
    assert_eq!(bridge.status(&h.state().session)?["dirty"], true);
    let undo = bridge.snapshot(
        &mut h.state_mut().session,
        Mutation {
            scope: scope(),
            base: head("3", &proposed.bytes),
            mutation_id: "ui-human-undo".into(),
        },
    )?;
    bridge.accept(
        &mut h.state_mut().session,
        &Accept {
            scope: scope(),
            mutation_id: "ui-human-undo".into(),
            head: head("4", &undo.bytes),
        },
    )?;
    assert_eq!(
        bridge.status(&h.state().session)?["head"],
        json!(head("4", &undo.bytes))
    );
    Ok(())
}

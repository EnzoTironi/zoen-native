//! Existing test-only dispatch probes; no production authority or new policy.

use super::*;

#[cfg(test)]
pub(crate) async fn exercise_deckcraft_operation(
    e: &Arc<Client>,
    worker: Arc<zoen_agentd::deckcraft::DeckcraftWorker>,
    journal: &Path,
    binding: (&str, &str, &str),
    arguments: serde_json::Value,
) -> Vec<ToolStep> {
    use zoen_agentd::citizen::testing::scripted::ScriptedBrain;
    let (item, source, serving) = binding;
    let space = e.item(item.into()).unwrap().space_id;
    let account = e.account().unwrap();
    let call_name = crate::deckcraft::model_name("deckcraft_slide_notes", item).unwrap();
    let response = format!(
        "```tools\n{}\n```\nApplied the reviewed notes.",
        json!([{"name":call_name,"arguments":arguments}])
    );
    let cx = Ctx {
        me: account.identity_id,
        device: account.device_id,
        owner: e.agent_owner().unwrap(),
        runtime: Runtime::new(
            Gate::default(),
            Box::new(ScriptedBrain {
                replies: [("reviewed edits".into(), response)].into(),
            }),
        ),
        deckcraft: Some(worker),
        journal: journal.into(),
    };
    let host = SpaceTools::new(e, &cx, &space, serving, source);
    let definition: Definition = serde_json::from_value(json!({"version":1,"kind":"user","persona":"Apply the reviewed presentation edit.","rules":[],"state":{}})).unwrap();
    let context = TurnContext {
        agent: &cx.me,
        device: &cx.device,
        owner: &cx.owner,
        definition: &definition,
    };
    let entry = e
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == source)
        .unwrap();
    let EntryKind::Message { text, .. } = entry.kind else {
        panic!("signed message source")
    };
    let outcome = cx
        .runtime
        .turn_with_tools(
            context,
            &[],
            &Turn {
                space,
                from: entry.author.id,
                text,
            },
            &host,
            vec![],
        )
        .await
        .unwrap();
    assert!(
        matches!(outcome, TurnOutcome::Say { ref text } if text == "Applied the reviewed notes."),
        "actual Runtime outcome: {outcome:?}; durable steps: {:?}",
        host.done()
    );
    let steps = host.done();
    assert_eq!(steps.len(), 1);
    assert!(!steps[0].is_error);
    assert_eq!(steps[0].call.name, call_name);
    steps
}

#[cfg(test)]
pub(crate) async fn exercise_deckcraft_dispatch(
    e: &Arc<Client>,
    worker: Arc<zoen_agentd::deckcraft::DeckcraftWorker>,
    journal: &Path,
    binding: (&str, &str, &str),
    arguments: serde_json::Value,
) {
    use zoen_agentd::citizen::testing::scripted::ScriptedBrain;
    let (item, source, serving) = binding;
    let space = e.item(item.into()).unwrap().space_id;
    let account = e.account().unwrap();
    let call_name = crate::deckcraft::model_name("deckcraft_slide_notes", item).unwrap();
    let replay_arguments = arguments.clone();
    let response = format!(
        "```tools\n{}\n```\nApplied the reviewed notes.",
        json!([{"name":call_name,"arguments":arguments}])
    );
    let cx = Ctx {
        me: account.identity_id,
        device: account.device_id,
        owner: e.agent_owner().unwrap(),
        runtime: Runtime::new(
            Gate::default(),
            Box::new(ScriptedBrain {
                replies: [("reviewed edit".into(), response)].into(),
            }),
        ),
        deckcraft: Some(worker),
        journal: journal.into(),
    };
    assert_eq!(item.len(), 72); // actual deterministic app_binary_create format
    let first_host = SpaceTools::new(e, &cx, &space, serving, source);
    assert_eq!(
        first_host
            .names
            .iter()
            .find(|(tool, _, _)| tool.name == call_name)
            .unwrap()
            .1,
        item
    );
    let before_order: Vec<String> = e
        .items()
        .into_iter()
        .filter(|entry| {
            entry.space_id == space
                && entry
                    .app
                    .as_ref()
                    .is_some_and(|app| app.app_id == "deckcraft")
        })
        .map(|entry| entry.id)
        .collect();
    assert_eq!(before_order, vec![item.to_owned()]);
    let source_document = e.app_binary_read(item.into(), None, None).unwrap();
    let other = e
        .app_binary_create(
            space.clone(),
            "Other deck inserted during this turn".into(),
            source_document.data_base64,
            "other_deck".into(),
        )
        .unwrap();
    let after_order: Vec<String> = e
        .items()
        .into_iter()
        .filter(|entry| {
            entry.space_id == space
                && entry
                    .app
                    .as_ref()
                    .is_some_and(|app| app.app_id == "deckcraft")
        })
        .map(|entry| entry.id)
        .collect();
    assert_eq!(after_order.len(), 2);
    assert_eq!(after_order, vec![other.item_id.clone(), item.to_owned()]);
    let host = SpaceTools::new(e, &cx, &space, serving, source);
    assert_eq!(host.deckcraft_items.len(), 2);
    assert!(host
        .tools()
        .iter()
        .filter(|tool| tool.name == call_name)
        .all(|tool| tool.parameters["properties"]["artifactId"]["enum"] == json!([item])));
    assert_eq!(
        host.names
            .iter()
            .find(|(tool, _, _)| tool.name == call_name)
            .unwrap()
            .1,
        item
    );
    assert_ne!(
        crate::deckcraft::model_name("deckcraft_slide_notes", &other.item_id).unwrap(),
        call_name
    );
    assert!(host.tools().len() <= 64);
    let mut mismatched = replay_arguments.clone();
    mismatched["artifactId"] = other.item_id.clone().into();
    assert!(host
        .call(&ToolCall {
            requires_review: false,
            id: "wrong_full_artifact".into(),
            name: call_name.clone(),
            arguments: mismatched
        })
        .is_err());
    assert!(host.done().is_empty());
    assert_eq!(
        e.app_binary_read(item.into(), None, None).unwrap().version,
        source_document.version
    );
    assert_eq!(
        e.app_binary_read(other.item_id.clone(), None, None)
            .unwrap(),
        other
    );
    let definition: Definition = serde_json::from_value(json!({"version":1,"kind":"user","persona":"Update only the reviewed presentation.","rules":[],"state":{}})).unwrap();
    let context = zoen_agentd::TurnContext {
        agent: &cx.me,
        device: &cx.device,
        owner: &cx.owner,
        definition: &definition,
    };
    let source_entry = e
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == source)
        .unwrap();
    let EntryKind::Message { text, .. } = source_entry.kind else {
        panic!("signed message source")
    };
    let inbound = Turn {
        space,
        from: source_entry.author.id,
        text,
    };
    let outcome = cx
        .runtime
        .turn_with_tools(context, &[], &inbound, &host, vec![])
        .await
        .unwrap();
    assert!(
        matches!(&outcome, zoen_agentd::TurnOutcome::Say { text } if text == "Applied the reviewed notes."),
        "actual Runtime outcome: {outcome:?}; durable steps: {:?}",
        host.done()
    );
    let steps = host.done();
    assert_eq!(steps.len(), 1);
    assert!(!steps[0].is_error);
    assert_eq!(steps[0].call.name, call_name);
    let repeated = host
        .call(&ToolCall {
            requires_review: false,
            id: "retained_call".into(),
            name: call_name,
            arguments: replay_arguments,
        })
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&repeated).unwrap()["replayed"],
        true
    );
    assert_eq!(host.done().len(), 1);
    assert_eq!(
        e.app_binary_read(other.item_id.clone(), None, None)
            .unwrap(),
        other
    );
    let accepted: serde_json::Value = serde_json::from_str(&steps[0].result).unwrap();
    assert_eq!(accepted["authorId"], cx.me);
    assert_eq!(accepted["replayed"], false);
}

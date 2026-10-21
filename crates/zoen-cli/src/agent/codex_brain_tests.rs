//! Explicit local real-model journey; never enabled by the ordinary test runner.
use super::*;

pub(crate) async fn exercise_codex_deckcraft(
    e: &Arc<Client>,
    worker: Arc<zoen_agentd::deckcraft::DeckcraftWorker>,
    journal: &Path,
    binding: (&str, &str, &str),
) -> Vec<ToolStep> {
    assert_eq!(std::env::var("ZOEN_TEST_REAL_CODEX").as_deref(), Ok("1"));
    let (item, source, serving) = binding;
    let space = e.item(item.into()).unwrap().space_id;
    let account = e.account().unwrap();
    let cx = Ctx {
        me: account.identity_id,
        device: account.device_id,
        owner: e.agent_owner().unwrap(),
        runtime: Runtime::new(Gate::default(), Box::new(CodexBrain::from_env().unwrap())),
        deckcraft: Some(worker),
        journal: journal.into(),
    };
    let host = SpaceTools::new(e, &cx, &space, serving, source);
    let definition: Definition = serde_json::from_value(json!({
        "version":1,"kind":"user","persona":"You validate a synthetic presentation edit. First propose exactly one read_deckcraft tool call using the offered artifact ID. Next, from that successful read, propose exactly one deckcraft_slide_notes tool call for its first slide and exact observed version/sha256. Set notes to the exact JSON string \"Codex local model — gated edit.\". After that successful write, return kind say with text exactly: Applied the reviewed notes. Do not repeat completed steps or choose any other tool. kind call uses an empty text and calls with unique id, offered name and arguments_json containing the serialized JSON object. A tool proposal executes only later through the Zoen Gate; you have no environment access.",
        "rules":[],"state":{}})).unwrap();
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
        panic!("signed source message required")
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
        matches!(&outcome,TurnOutcome::Say{text}if text=="Applied the reviewed notes."),
        "real model outcome: {outcome:?}"
    );
    let steps = host.done();
    assert_eq!(steps.len(), 2, "one observed read and one accepted edit");
    assert!(
        steps.iter().all(|s| !s.is_error),
        "all gate/host results succeeded"
    );
    assert_eq!(
        steps[0].call.name,
        crate::deckcraft::model_name("read_deckcraft", item).unwrap()
    );
    assert_eq!(
        steps[1].call.name,
        crate::deckcraft::model_name("deckcraft_slide_notes", item).unwrap()
    );
    let accepted: serde_json::Value = serde_json::from_str(&steps[1].result).unwrap();
    assert_eq!(accepted["authorId"], cx.me);
    assert_eq!(accepted["replayed"], false);
    assert_eq!(accepted["version"], 2);
    let repeated = host.call(&steps[1].call).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&repeated).unwrap()["replayed"],
        true
    );
    assert_eq!(host.done().len(), 2);
    steps
}

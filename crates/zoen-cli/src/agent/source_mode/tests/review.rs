use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_signed_owner_review_freezes_the_flag_and_arguments_without_exposing_raw_rule() {
    let server = relay(Mode::Normal).await;
    let mut f = Fixture::new(&server.url);
    online(&f.client, true).await;
    f.client.stop_sync();
    let base = deck(&f);
    let request = stage_review(&f, &base.item_id, &f.source, true);
    assert!(!request.can_always_approve);
    assert!(!request.reason.contains("native:"));
    let review = request.tool_approval.as_ref().unwrap();
    assert_eq!(review.operation, "Read presentation");
    assert_eq!(review.artifact_id, base.item_id);
    let pending = f
        .client
        .tool_approval_requests(&f.space, &f.source)
        .pop()
        .unwrap();
    let proposal = pending.tool_proposal.as_ref().unwrap();
    let ask: ToolAsk = serde_json::from_str(&proposal.continuation_json).unwrap();
    assert!(ask.call.requires_review);
    assert_eq!(ask.rule, zoen_agentd::NATIVE_REVIEW_REQUIRED);
    f.approve(&request).await;
    let permit = f
        .client
        .approved_tool_turn(&request.id, &"88".repeat(32))
        .unwrap();
    assert_eq!(
        permit.proposal().continuation_json,
        serde_json::to_string(&ask).unwrap()
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let cx = context(&f, &calls, false);
    let run = Run::prepare(&f.client, source(&f.source)).unwrap();
    let mut tools = SpaceTools::new(&f.client, &cx, &f.space, &f.owner, &f.source);
    tools.selection = Some(&run.bound);
    let admission = CanonicalToolAdmission {
        tools: &tools,
        permit: super::super::super::page_tools::CanonicalPermit::Deck(permit),
    };
    let mut changed = ask.clone();
    changed.call.requires_review = false;
    assert_eq!(
        admission.execute(&changed).unwrap_err(),
        "The approved intent changed"
    );
    let mut changed = ask.clone();
    changed.call.arguments["artifactId"] = "foreign".into();
    assert_eq!(
        admission.execute(&changed).unwrap_err(),
        "The approved intent changed"
    );
    assert_eq!(
        admission.execute(&ask).unwrap_err(),
        "The continuation attempt was not reserved"
    );
    assert!(!f.path.join("source-steps.json").exists());
    JournalState::register_hold(&cx.journal, &request.id, &pending.content_hash, &f.source)
        .unwrap();
    assert!(tools
        .begin_resume(&request.id, &pending.content_hash)
        .unwrap());
    assert_eq!(
        admission.execute(&ask).unwrap_err(),
        "The presentation executor is unavailable"
    );
    let journal = JournalState::load(&cx.journal).unwrap();
    assert_eq!(journal.continuations[&request.id].attempts, 1);
    assert!(journal.continuations[&request.id].outcome_unknown);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(server.cursors.lock().unwrap().is_empty());
    assert!(f.path.join("source-steps.json").exists());
    assert_eq!(
        f.client
            .app_binary_read(base.item_id.clone(), None, None)
            .unwrap()
            .version,
        base.version
    );
}

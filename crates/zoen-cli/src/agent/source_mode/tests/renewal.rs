//! The real async turn gate defers an actual signed intent until SyncDone.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn signed_renewal_waits_for_actual_sync_without_reply_attempt_or_effect_then_stages_once() {
    let server = relay(Mode::Unsynced).await;
    let mut f = Fixture::new(&server.url);
    let item = f
        .client
        .page_import_markdown(
            f.space.clone(),
            "renew.md".into(),
            "Public text\n\nKeep this sibling.\n".into(),
        )
        .unwrap()
        .id;
    let journal = f.path.join("renewal-steps.json");
    let starts = Arc::new(AtomicUsize::new(0));
    {
        let cx =
            super::super::super::page_tool_tests::cx(&f.client, &journal, &item, starts.clone());
        super::super::super::canonical_tool_approval_tests::handle(
            &f.client, &cx, &f.space, &f.source,
        )
        .await;
    }
    let original = f
        .client
        .tool_approval_requests(&f.space, &f.source)
        .pop()
        .unwrap();
    let (expired, intent, history) = f.expired_review(&original);
    JournalState::register_hold(&journal, &expired.id, &expired.content_hash, &f.source).unwrap();
    // ACKs extend the exact synthetic signed prefix, never a fresh fake log.
    server
        .histories
        .lock()
        .unwrap()
        .insert(f.space.clone(), history);
    f.publish_owner_packages(&server).await;
    online(&f.client, false).await;
    let cx = super::super::super::page_tool_tests::cx(&f.client, &journal, &item, starts.clone());
    let def = super::super::super::canonical_tool_approval_tests::definition();
    let mut run = Run::prepare(&f.client, source(&f.source)).unwrap();
    let before = std::fs::read(&journal).unwrap();
    let end = deadline();
    assert!(!run.tick(&f.client, &cx, &def, end).await.unwrap());
    assert_eq!(f.client.page(item.clone()).unwrap().version, 1);
    assert_eq!(
        f.client.tool_approval_requests(&f.space, &f.source).len(),
        2
    );
    assert_eq!(std::fs::read(&journal).unwrap(), before);
    assert!(
        !f.client
            .timeline(f.space.clone())
            .unwrap()
            .iter()
            .any(|m| m.reply_to.as_ref().is_some_and(|r| r.id == f.source)),
        "pre-IO unsettled is not a terminal reconciliation reply"
    );
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    server.released.store(true, Ordering::SeqCst);
    server.release.notify_waiters();
    f.client.wait_until_settled(2000).await.unwrap();
    let tools = SpaceTools::new(&f.client, &cx, &f.space, &f.owner, &f.source);
    let ask: ToolAsk =
        serde_json::from_str(&expired.tool_proposal.as_ref().unwrap().continuation_json).unwrap();
    let timeline_after = f.client.timeline(f.space.clone()).unwrap();
    let source_after = timeline_after.iter().find(|e| e.id == f.source).unwrap();
    let inbound = Turn {
        space: f.space.clone(),
        from: f.owner.clone(),
        text: text_of(source_after).unwrap().into(),
    };
    let tc = TurnContext {
        agent: &cx.me,
        device: &cx.device,
        owner: &cx.owner,
        definition: &def,
    };
    assert!(
        JournalState::renewal_safe(&journal, &expired.id, &expired.content_hash, &f.source)
            .unwrap()
    );
    let current = f.client.agent_page_turn(&item, &f.source, &f.owner);
    assert!(
        current.is_ok(),
        "fixture authority after sync: {:?}",
        current.err()
    );
    let check = cx.runtime.validate_tool_review(tc, &inbound, &tools, &ask);
    assert!(
        matches!(check, Ok(true)),
        "fixture held context after sync: {check:?}"
    );
    drop(tools);
    assert!(!run.tick(&f.client, &cx, &def, end).await.unwrap());
    let requests = f.client.tool_approval_requests(&f.space, &f.source);
    assert_eq!(requests.len(), 3);
    let next = requests.last().unwrap();
    let lineage = next
        .tool_proposal
        .as_ref()
        .unwrap()
        .review
        .as_ref()
        .unwrap();
    assert_eq!(lineage.intent, intent);
    assert_eq!(lineage.predecessor, expired.id);
    assert!(f.client.approved_page_tool_turn(&next.id).is_err());
    assert!(!run.tick(&f.client, &cx, &def, end).await.unwrap());
    assert_eq!(
        f.client.tool_approval_requests(&f.space, &f.source).len(),
        3
    );
    assert_eq!(f.client.page(item).unwrap().version, 1);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read(&journal).unwrap(), before);
    assert!(!server
        .calls
        .lock()
        .unwrap()
        .contains(&"forbidden_agent_operation"));
    assert!(!server.calls.lock().unwrap().contains(&"read_receipt"));
    drop(cx);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn normal_signed_renewal_reports_scheduling_deferred_without_consuming_a_turn_attempt() {
    let server = relay(Mode::Unsynced).await;
    let mut f = Fixture::new(&server.url);
    let item = f
        .client
        .page_import_markdown(
            f.space.clone(),
            "normal.md".into(),
            "Public text\n\nKeep this sibling.\n".into(),
        )
        .unwrap()
        .id;
    let journal = f.path.join("normal-renewal-steps.json");
    let starts = Arc::new(AtomicUsize::new(0));
    {
        let cx =
            super::super::super::page_tool_tests::cx(&f.client, &journal, &item, starts.clone());
        super::super::super::canonical_tool_approval_tests::handle(
            &f.client, &cx, &f.space, &f.source,
        )
        .await;
    }
    let original = f
        .client
        .tool_approval_requests(&f.space, &f.source)
        .pop()
        .unwrap();
    let (expired, _, history) = f.expired_review(&original);
    JournalState::register_hold(&journal, &expired.id, &expired.content_hash, &f.source).unwrap();
    server
        .histories
        .lock()
        .unwrap()
        .insert(f.space.clone(), history);
    f.publish_owner_packages(&server).await;
    online(&f.client, false).await;
    let cx = super::super::super::page_tool_tests::cx(&f.client, &journal, &item, starts.clone());
    let def = super::super::super::canonical_tool_approval_tests::definition();
    let timeline = f.client.timeline(f.space.clone()).unwrap();
    let index = timeline.iter().position(|e| e.id == f.source).unwrap();
    let before = std::fs::read(&journal).unwrap();
    assert_eq!(
        super::super::super::turn::turn(
            &f.client,
            &cx,
            &def,
            &f.space,
            &timeline[..index],
            &timeline[index]
        )
        .await,
        super::super::super::turn::Outcome::Deferred
    );
    assert_eq!(std::fs::read(&journal).unwrap(), before);
    assert_eq!(f.client.page(item.clone()).unwrap().version, 1);
    assert!(!f
        .client
        .timeline(f.space.clone())
        .unwrap()
        .iter()
        .any(|m| m.reply_to.as_ref().is_some_and(|r| r.id == f.source)));
    server.released.store(true, Ordering::SeqCst);
    server.release.notify_waiters();
    f.client.wait_until_settled(2000).await.unwrap();
    let tools = SpaceTools::new(&f.client, &cx, &f.space, &f.owner, &f.source);
    let ask: ToolAsk =
        serde_json::from_str(&expired.tool_proposal.as_ref().unwrap().continuation_json).unwrap();
    let timeline_after = f.client.timeline(f.space.clone()).unwrap();
    let source_after = timeline_after.iter().find(|e| e.id == f.source).unwrap();
    let inbound = Turn {
        space: f.space.clone(),
        from: f.owner.clone(),
        text: text_of(source_after).unwrap().into(),
    };
    let tc = TurnContext {
        agent: &cx.me,
        device: &cx.device,
        owner: &cx.owner,
        definition: &def,
    };
    assert!(
        JournalState::renewal_safe(&journal, &expired.id, &expired.content_hash, &f.source)
            .unwrap()
    );
    let current = f.client.agent_page_turn(&item, &f.source, &f.owner);
    assert!(
        current.is_ok(),
        "fixture authority after sync: {:?}",
        current.err()
    );
    let check = cx.runtime.validate_tool_review(tc, &inbound, &tools, &ask);
    assert!(
        matches!(check, Ok(true)),
        "fixture held context after sync: {check:?}"
    );
    drop(tools);
    assert_eq!(
        super::super::super::turn::turn(
            &f.client,
            &cx,
            &def,
            &f.space,
            &timeline[..index],
            &timeline[index]
        )
        .await,
        super::super::super::turn::Outcome::Handled
    );
    assert_eq!(
        f.client.tool_approval_requests(&f.space, &f.source).len(),
        3
    );
    assert_eq!(
        JournalState::load(&journal).unwrap().continuations[&expired.id].attempts,
        0
    );
    assert_eq!(f.client.page(item).unwrap().version, 1);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert!(!server
        .calls
        .lock()
        .unwrap()
        .contains(&"forbidden_agent_operation"));
    assert!(!server.calls.lock().unwrap().contains(&"read_receipt"));
    drop(cx);
}

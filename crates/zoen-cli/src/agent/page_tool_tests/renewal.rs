//! Signed fixture and actual host/journal; no worker, relay or inference.
use super::*;
use client_core::SecretVault;
use space_tools::renewal::Preparation;

fn expired_history(
    fixture: &mut crate::deckcraft::tests::Fixture,
    space: &str,
    original: &types::AgentRequest,
) -> (types::AgentRequest, String) {
    let client = std::mem::replace(
        &mut fixture.client,
        Client::open(":memory:".into(), "en".into()).unwrap(),
    );
    client.stop_sync();
    assert_eq!(
        Arc::strong_count(&client),
        1,
        "close the actual fixture Client before signed offline transfer"
    );
    drop(client);
    let db = store::Store::open(fixture.path.join("agent.sqlite").to_str().unwrap()).unwrap();
    let mut log = event_log::SpaceLog::from_events(space, db.events(space).unwrap()).unwrap();
    for row in db
        .outbox()
        .unwrap()
        .into_iter()
        .filter(|row| row.event.space == space)
    {
        if !log
            .events()
            .iter()
            .any(|event| event.client_id == row.event.client_id)
        {
            db.append_event(log.sequence(row.event.clone())).unwrap();
        }
        db.outbox_remove(&row.event.client_id).unwrap();
    }
    let root = event_log::Signer::from_secret(&[72; 32]);
    let device: [u8; 32] = fixture
        .vault
        .load("zoen.device.v1".into())
        .unwrap()
        .try_into()
        .unwrap();
    let agent = event_log::Author::device(&root, event_log::Signer::from_secret(&device));
    assert_eq!(agent.identity, original.agent);
    assert_eq!(
        agent.device.as_deref(),
        Some(original.tool_proposal.as_ref().unwrap().device.as_str())
    );
    let owner: [u8; 32] = fixture
        .owner_vault
        .load("zoen.identity.v1".into())
        .unwrap()
        .try_into()
        .unwrap();
    let owner = event_log::Author::root(event_log::Signer::from_secret(&owner));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let mut expired = original.clone();
    expired.id = types::new_id("expired-host-review");
    expired.tool_proposal.as_mut().unwrap().expires_at_ms = now - 1000;
    expired.content_hash = expired.tool_proposal_hash(space).unwrap();
    for (author, at, body) in [
        (
            &agent,
            now - 2000,
            types::EventBody::RequestOpened {
                request: expired.clone(),
            },
        ),
        (
            &owner,
            now - 1001,
            types::EventBody::RequestResolved {
                request: expired.id.clone(),
                approved: true,
                content_hash: expired.content_hash.clone(),
                resolution: Some(types::RequestResolution {
                    message: "Approved once".into(),
                    created_item: None,
                    ai_cost_cents: 0,
                    standing_grant: None,
                    tool_decision: Some(types::ToolDecision::ApproveOnce),
                }),
            },
        ),
        (
            &owner,
            now,
            types::EventBody::ToolReviewRequested {
                request: expired.id.clone(),
                content_hash: expired.content_hash.clone(),
            },
        ),
    ] {
        let event = author.sign_event(space, &types::new_ulid(at), at, log.head(), body);
        db.append_event(log.sequence(event)).unwrap();
    }
    log.verify().unwrap();
    (expired, log.events().last().unwrap().client_id.clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn actual_page_host_stages_one_new_pending_review_and_resumes_under_original_journal_budget()
{
    let mut fixture = crate::deckcraft::tests::fixture();
    let space = fixture
        .client
        .item(fixture.base.item_id.clone())
        .unwrap()
        .space_id;
    let item = fixture
        .client
        .page_import_markdown(
            space.clone(),
            "renew.md".into(),
            "Public text\n\nKeep this sibling.\n".into(),
        )
        .unwrap()
        .id;
    let journal = fixture.path.join("renewal-journal.json");
    let starts = Arc::new(AtomicUsize::new(0));
    {
        let context = cx(&fixture.client, &journal, &item, starts.clone());
        canonical_tool_approval_tests::handle(&fixture.client, &context, &space, &fixture.source)
            .await;
    }
    let original = fixture
        .client
        .tool_approval_requests(&space, &fixture.source)
        .pop()
        .unwrap();
    let (expired, intent) = expired_history(&mut fixture, &space, &original);
    // Only this newly signed, never-executed fixture request is registered.
    // A missing/legacy production record cannot acquire this witness retroactively.
    JournalState::register_hold(
        &journal,
        &expired.id,
        &expired.content_hash,
        &fixture.source,
    )
    .unwrap();
    let client = canonical_tool_approval_tests::reopen(&fixture.path, fixture.vault.clone());
    let context = cx(&client, &journal, &item, starts.clone());
    let tools = SpaceTools::new(&client, &context, &space, &fixture.owner, &fixture.source);
    let ask: ToolAsk =
        serde_json::from_str(&expired.tool_proposal.as_ref().unwrap().continuation_json).unwrap();
    let timeline = client.timeline(space.clone()).unwrap();
    let source = timeline
        .iter()
        .find(|entry| entry.id == fixture.source)
        .unwrap();
    let inbound = Turn {
        space: space.clone(),
        from: fixture.owner.clone(),
        text: text_of(source).unwrap().into(),
    };
    let def = canonical_tool_approval_tests::definition();
    let tc = TurnContext {
        agent: &context.me,
        device: &context.device,
        owner: &context.owner,
        definition: &def,
    };
    assert!(
        tools
            .prepare_review(&context, tc, &inbound, &expired, &ask)
            .await
            .is_ok_and(|p| matches!(p, Preparation::Deferred)),
        "the production entrypoint defers the offline/unsettled fixture before IO"
    );
    assert_eq!(
        client.tool_approval_requests(&space, &fixture.source).len(),
        2
    );
    // Exercise the same production post-settlement preparation with an actual
    // signed, confirmed offline history; this is not network-settled proof.
    assert!(matches!(
        tools
            .prepare_settled_review(&context, tc, &inbound, &expired, &ask, &intent)
            .unwrap(),
        Preparation::Pending
    ));
    let next = client
        .tool_approval_requests(&space, &fixture.source)
        .pop()
        .unwrap();
    assert_eq!(
        next.tool_proposal
            .as_ref()
            .unwrap()
            .review
            .as_ref()
            .unwrap()
            .root,
        expired.id
    );
    assert!(client.approved_page_tool_turn(&next.id).is_err());
    assert_eq!(client.page(item.clone()).unwrap().version, 1);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(
        tools.journal_binding(&next.id, &next.content_hash).unwrap(),
        (expired.id.clone(), expired.content_hash.clone())
    );
    drop(tools);
    drop(context);
    drop(client);
    canonical_tool_approval_tests::signed_owner_resolution(
        &fixture.path,
        &space,
        &next,
        &fixture.owner,
    );
    let client = canonical_tool_approval_tests::reopen(&fixture.path, fixture.vault.clone());
    let context = cx(&client, &journal, &item, starts.clone());
    canonical_tool_approval_tests::handle(&client, &context, &space, &fixture.source).await;
    assert_eq!(client.page(item.clone()).unwrap().version, 2);
    assert_eq!(
        starts.load(Ordering::SeqCst),
        1,
        "no fresh model proposal was requested"
    );
    let retained = JournalState::load(&journal).unwrap();
    assert_eq!(retained.continuations[&expired.id].attempts, 1);
    assert!(retained.continuations[&expired.id].outcome_unknown);
    assert!(retained.continuations[&expired.id].progress.is_some());
    assert!(
        !retained.continuations.contains_key(&next.id),
        "renewal never resets operational attempts"
    );
    drop(context);
    drop(client);
    let cold = canonical_tool_approval_tests::reopen(&fixture.path, fixture.vault.clone());
    let context = cx(&cold, &journal, &item, starts.clone());
    canonical_tool_approval_tests::handle(&cold, &context, &space, &fixture.source).await;
    assert_eq!(cold.page(item).unwrap().version, 2);
    assert_eq!(
        JournalState::load(&journal).unwrap().continuations[&expired.id].attempts,
        1
    );
    drop(context);
    drop(cold);
    let path = fixture.path.clone();
    drop(fixture);
    std::fs::remove_dir_all(path).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn absent_legacy_or_unknown_journal_cannot_restage_an_expired_page_action_or_call_the_model()
{
    for witness in ["absent", "legacy", "unknown"] {
        let mut fixture = crate::deckcraft::tests::fixture();
        let space = fixture
            .client
            .item(fixture.base.item_id.clone())
            .unwrap()
            .space_id;
        let item = fixture
            .client
            .page_import_markdown(
                space.clone(),
                "reconcile.md".into(),
                "Public text\n\nKeep this sibling.\n".into(),
            )
            .unwrap()
            .id;
        let journal = fixture.path.join("reconcile-journal.json");
        let starts = Arc::new(AtomicUsize::new(0));
        {
            let context = cx(&fixture.client, &journal, &item, starts.clone());
            canonical_tool_approval_tests::handle(
                &fixture.client,
                &context,
                &space,
                &fixture.source,
            )
            .await;
        }
        let original = fixture
            .client
            .tool_approval_requests(&space, &fixture.source)
            .pop()
            .unwrap();
        let (expired, intent) = expired_history(&mut fixture, &space, &original);
        if witness == "unknown" {
            JournalState::register_hold(
                &journal,
                &expired.id,
                &expired.content_hash,
                &fixture.source,
            )
            .unwrap();
        }
        if witness != "absent" {
            JournalState::begin_resume(
                &journal,
                &expired.id,
                &expired.content_hash,
                Some(&fixture.source),
            )
            .unwrap();
        }
        if witness == "unknown" {
            JournalState::entering_executor(
                &journal,
                &expired.id,
                &expired.content_hash,
                &fixture.source,
            )
            .unwrap();
        }
        let before = std::fs::read(&journal).unwrap();
        let client = canonical_tool_approval_tests::reopen(&fixture.path, fixture.vault.clone());
        let context = cx(&client, &journal, &item, starts.clone());
        let tools = SpaceTools::new(&client, &context, &space, &fixture.owner, &fixture.source);
        let ask = serde_json::from_str(&expired.tool_proposal.as_ref().unwrap().continuation_json)
            .unwrap();
        let timeline = client.timeline(space.clone()).unwrap();
        let source = timeline
            .iter()
            .find(|entry| entry.id == fixture.source)
            .unwrap();
        let inbound = Turn {
            space: space.clone(),
            from: fixture.owner.clone(),
            text: text_of(source).unwrap().into(),
        };
        let def = canonical_tool_approval_tests::definition();
        let tc = TurnContext {
            agent: &context.me,
            device: &context.device,
            owner: &context.owner,
            definition: &def,
        };
        assert!(
            matches!(
                tools
                    .prepare_settled_review(&context, tc, &inbound, &expired, &ask, &intent)
                    .unwrap(),
                Preparation::Reconcile
            ),
            "{witness}"
        );
        assert_eq!(
            client.tool_approval_requests(&space, &fixture.source).len(),
            2
        );
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(client.page(item).unwrap().version, 1);
        assert_eq!(std::fs::read(&journal).unwrap(), before);
        drop(tools);
        drop(context);
        drop(client);
        let path = fixture.path.clone();
        drop(fixture);
        std::fs::remove_dir_all(path).unwrap();
    }
}

//! Actual normal-host preflight and the existing post-settlement routine over
//! signed offline history. This is not a joined network/readiness proof.
use super::*;
use fixture::{append, owner_change, rewrite, signer, Fixture};
use space_tools::renewal::Preparation;
use types::EventBody;

type R<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn read_context(
    client: &Arc<Client>,
    journal: &Path,
    item: &str,
    calls: Arc<AtomicUsize>,
) -> R<Ctx> {
    let account = client.account().ok_or("Agent account")?;
    let item = item.to_owned();
    Ok(Ctx {
        me: account.identity_id,
        device: account.device_id,
        owner: client.agent_owner().ok_or("Agent owner")?,
        deckcraft: None,
        journal: journal.into(),
        runtime: Runtime::new(
            Gate {
                on_device: Some(Box::new(page_tool_tests::Allow)),
                ..Gate::default()
            },
            Box::new(ControlledToolBrain {
                evaluate: Box::new(move |steps| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    match steps {
                        [] => Ok(BrainOut::CallTools(vec![ToolCall {
                            id: "read-with-explicit-review".into(),
                            name: scene::model_name("read_scene", &item)
                                .ok_or("Owned read tool name")?,
                            arguments: json!({"artifactId":item}),
                            requires_review: true,
                        }])),
                        [read] => {
                            assert!(!read.is_error, "{}", read.result);
                            let snapshot: SceneSnapshot = serde_json::from_str(&read.result)
                                .map_err(|error| error.to_string())?;
                            assert_eq!(snapshot.artifact_id, item);
                            assert_eq!(snapshot.version, 1);
                            Ok(BrainOut::Say(
                                "Read the separately approved drawing.".into(),
                            ))
                        }
                        _ => Err("Unexpected reviewed-read progress".into()),
                    }
                }),
            }),
        ),
    })
}

async fn normal(f: &Fixture, cx: &Ctx) -> R<turn::Outcome> {
    let timeline = f.client.timeline(f.space.clone())?;
    let index = timeline
        .iter()
        .position(|entry| entry.id == f.source)
        .ok_or("Original signed source")?;
    Ok(turn::turn(
        &f.client,
        cx,
        &canonical_tool_approval_tests::definition(),
        &f.space,
        &timeline[..index],
        &timeline[index],
    )
    .await)
}

fn owner_sources(f: &Fixture) -> R<BTreeSet<String>> {
    Ok(f.client
        .timeline(f.space.clone())?
        .into_iter()
        .filter(|entry| {
            entry.author.id == f.owner && matches!(&entry.kind, EntryKind::Message { .. })
        })
        .map(|entry| entry.id)
        .collect())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn pending_scene_read_owner_intent_reaches_normal_preflight_and_one_separate_review() -> R<()>
{
    let mut f = fixture().await?;
    let journal = f.path.join("pending-read-review.json");
    let calls = Arc::new(AtomicUsize::new(0));
    let before = f.client.app_document_read(f.item.clone())?;
    let sources = owner_sources(&f)?;
    let cx = read_context(&f.client, &journal, &f.item, calls.clone())?;
    assert_eq!(normal(&f, &cx).await?, turn::Outcome::Handled);
    let original = f
        .client
        .tool_approval_requests(&f.space, &f.source)
        .pop()
        .ok_or("Pending read")?;
    let proposal = original.tool_proposal.as_ref().ok_or("Read proposal")?;
    assert_eq!(proposal.schema, 6);
    assert_eq!(proposal.canonical_tool, "read_scene");
    assert_eq!(proposal.gate_rule, "native:review-required");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(f.client.approved_scene_tool_turn(&original.id).is_err());
    drop(cx);

    let mut expired = original;
    expired.id = types::new_id("expired_pending_scene_read")?;
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;
    expired
        .tool_proposal
        .as_mut()
        .ok_or("Read proposal")?
        .expires_at_ms = at - 1000;
    expired.content_hash = expired
        .tool_proposal_hash(&f.space)
        .ok_or("Expired request hash")?;
    let agent = signer(&f.vault);
    let space = f.space.clone();
    // A real signed past RequestOpened, with no RequestResolved/approval event.
    // Production TTL/clock are unchanged. Only this never-started fixture is registered.
    rewrite(&mut f, |store, log| {
        append(
            store,
            log,
            &space,
            &agent,
            at - 2000,
            EventBody::RequestOpened {
                request: expired.clone(),
            },
        )
    })?;
    JournalState::register_hold(&journal, &expired.id, &expired.content_hash, &f.source)?;
    let cx = read_context(&f.client, &journal, &f.item, calls.clone())?;
    assert_eq!(normal(&f, &cx).await?, turn::Outcome::Handled);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(f.client.tool_review_pending_intent(&expired.id).is_none());
    assert_eq!(
        f.client.tool_approval_requests(&f.space, &f.source).len(),
        2
    );
    assert!(JournalState::renewal_safe(
        &journal,
        &expired.id,
        &expired.content_hash,
        &f.source
    )?);
    drop(cx);

    let old = expired.clone();
    owner_change(&mut f, |owner| async move {
        assert!(owner
            .decide_tool_request(
                old.id.clone(),
                old.content_hash.clone(),
                client_core::ToolRequestDecision::ApproveOnce
            )
            .is_err());
        owner.decide_tool_request(
            old.id,
            old.content_hash,
            client_core::ToolRequestDecision::RequestReview,
        )?;
        Ok(())
    })
    .await?;
    let intent = f
        .client
        .tool_review_pending_intent(&expired.id)
        .ok_or("Signed owner intent")?;
    let cx = read_context(&f.client, &journal, &f.item, calls.clone())?;
    // The actual offline host must reach the unchanged synchronization gate.
    // Its predecessor incorrectly returned Handled at Pending/Stale before this gate.
    assert_eq!(normal(&f, &cx).await?, turn::Outcome::Deferred);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.client.tool_approval_requests(&f.space, &f.source).len(),
        2
    );
    assert!(JournalState::renewal_safe(
        &journal,
        &expired.id,
        &expired.content_hash,
        &f.source
    )?);
    assert_eq!(
        f.client.app_document_read(f.item.clone())?.sha256,
        before.sha256
    );

    // Same existing production post-settlement routine as the Page host proof;
    // no readiness flag or journal witness is fabricated to bypass the preflight.
    let tools = SpaceTools::new(&f.client, &cx, &f.space, &f.owner, &f.source);
    let timeline = f.client.timeline(f.space.clone())?;
    let source = timeline
        .iter()
        .find(|entry| entry.id == f.source)
        .ok_or("Source")?;
    let inbound = Turn {
        space: f.space.clone(),
        from: f.owner.clone(),
        text: text_of(source).ok_or("Source text")?.into(),
    };
    let definition = canonical_tool_approval_tests::definition();
    let tc = TurnContext {
        agent: &cx.me,
        device: &cx.device,
        owner: &cx.owner,
        definition: &definition,
    };
    let p = expired.tool_proposal.as_ref().ok_or("Expired proposal")?;
    let ask: ToolAsk = serde_json::from_str(&p.continuation_json)?;
    assert!(matches!(
        tools.prepare_settled_review(&cx, tc, &inbound, &expired, &ask, &intent)?,
        Preparation::Pending
    ));
    let next = f
        .client
        .tool_approval_requests(&f.space, &f.source)
        .pop()
        .ok_or("Successor")?;
    let renewed = next.tool_proposal.as_ref().ok_or("Successor proposal")?;
    assert_eq!(renewed.arguments_json, p.arguments_json);
    assert_eq!(renewed.continuation_json, p.continuation_json);
    assert_eq!(renewed.source, f.source);
    assert_eq!(renewed.task_sha256, p.task_sha256);
    assert_eq!(renewed.artifact, f.item);
    let lineage = renewed.review.as_ref().ok_or("Review lineage")?;
    assert_eq!(lineage.predecessor, expired.id);
    assert_eq!(lineage.root, expired.id);
    assert_eq!(lineage.generation, 1);
    assert!(f.client.approved_scene_tool_turn(&next.id).is_err());
    assert_eq!(
        f.client
            .stage_tool_review_successor(&expired.id, &intent)?
            .ok_or("Existing successor receipt")?
            .id,
        next.id,
        "the original owner intent recovers its single successor"
    );
    assert_eq!(
        f.client.tool_approval_requests(&f.space, &f.source).len(),
        3
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(tools);
    drop(cx);

    // Normal cold reload reconstructs the same successor and still awaits its
    // independent approval. No new source, model proposal or operational budget.
    rewrite(&mut f, |_, _| Ok(()))?;
    let cx = read_context(&f.client, &journal, &f.item, calls.clone())?;
    assert_eq!(normal(&f, &cx).await?, turn::Outcome::Handled);
    assert_eq!(
        f.client
            .tool_approval_requests(&f.space, &f.source)
            .last()
            .map(|r| &r.id),
        Some(&next.id)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(owner_sources(&f)?, sources);
    assert_eq!(
        f.client.app_document_read(f.item.clone())?.sha256,
        before.sha256
    );
    drop(cx);
    let approved = next.clone();
    owner_change(&mut f, |owner| async move {
        owner.decide_tool_request(
            approved.id,
            approved.content_hash,
            client_core::ToolRequestDecision::ApproveOnce,
        )?;
        Ok(())
    })
    .await?;
    let cx = read_context(&f.client, &journal, &f.item, calls.clone())?;
    assert_eq!(normal(&f, &cx).await?, turn::Outcome::Handled);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "only the approved read allows the final Say"
    );
    assert_eq!(
        f.client.app_document_read(f.item.clone())?.sha256,
        before.sha256
    );
    assert_eq!(owner_sources(&f)?, sources);
    assert_eq!(
        f.client.tool_approval_requests(&f.space, &f.source).len(),
        3
    );
    drop(cx);
    rewrite(&mut f, |_, _| Ok(()))?;
    let cx = read_context(&f.client, &journal, &f.item, calls.clone())?;
    assert_eq!(normal(&f, &cx).await?, turn::Outcome::Handled);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "the cold receipt cannot call the model again"
    );
    assert_eq!(
        f.client.tool_approval_requests(&f.space, &f.source).len(),
        3
    );
    assert_eq!(owner_sources(&f)?, sources);
    let retained = JournalState::load(&journal)?;
    let witness = retained
        .continuations
        .get(&expired.id)
        .ok_or("Original journal witness")?;
    assert_eq!(witness.attempts, 1);
    assert!(witness.outcome_unknown && witness.progress.is_some());
    assert!(!retained.continuations.contains_key(&next.id));
    drop(cx);
    let path = f.path.clone();
    drop(f);
    std::fs::remove_dir_all(path)?;
    Ok(())
}

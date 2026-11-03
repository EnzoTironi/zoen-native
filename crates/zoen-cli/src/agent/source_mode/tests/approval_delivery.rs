//! Real signed Core requests and failed SQLite writes; no provider or user data.
use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn reviewed_context(f: &Fixture, calls: &Arc<AtomicUsize>, call: &ToolCall) -> Ctx {
    let mut cx = context(f, calls, true);
    let calls = calls.clone();
    let call = call.clone();
    cx.runtime = Runtime::new(
        Gate {
            on_device: Some(Box::new(crate::agent::page_tool_tests::Allow)),
            ..Gate::default()
        },
        Box::new(ControlledToolBrain {
            evaluate: Box::new(move |steps| {
                calls.fetch_add(1, Ordering::SeqCst);
                match steps {
                    [] => Ok(BrainOut::CallTools(vec![call.clone()])),
                    [read] if read.call == call && !read.is_error => {
                        Ok(BrainOut::Say("Read the exact approved source.".into()))
                    }
                    _ => Err("Unexpected reviewed progress".into()),
                }
            }),
        }),
    );
    cx
}

async fn run(
    f: &Fixture,
    cx: &Ctx,
) -> Result<crate::agent::turn::Outcome, Box<dyn std::error::Error>> {
    let timeline = f.client.timeline(f.space.clone())?;
    let index = timeline
        .iter()
        .position(|entry| entry.id == f.source)
        .ok_or("source missing")?;
    Ok(crate::agent::turn::turn(
        &f.client,
        cx,
        &definition(),
        &f.space,
        &timeline[..index],
        &timeline[index],
    )
    .await)
}

fn known(f: &Fixture, cx: &Ctx) -> Result<TurnOutcome, Box<dyn std::error::Error>> {
    let source = f
        .client
        .timeline(f.space.clone())?
        .into_iter()
        .find(|entry| entry.id == f.source)
        .ok_or("source missing")?;
    JournalState::known_turn(&cx.journal, cx, &f.space, &source, &definition())?
        .ok_or_else(|| "known outcome missing".into())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_approval_write_recovers_once_after_restart_without_repeating_inference(
) -> TestResult {
    approval_delivery(false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reviewed_history_search_survives_failed_delivery_and_restarts_with_one_answer(
) -> TestResult {
    approval_delivery(true).await
}

async fn approval_delivery(history: bool) -> TestResult {
    let mut f = Fixture::new("http://127.0.0.1:1")?;
    let item = if history {
        None
    } else {
        Some(
            f.client
                .page_import_markdown(
                    f.space.clone(),
                    "approval-fixture.md".into(),
                    "Read only after owner approval.".into(),
                )?
                .id,
        )
    };
    f.confirm_publications()?;
    // The owner inbox already exists; the injected failure targets the next
    // publication (RequestOpened), not account setup or model reservation.
    f.client.start_direct(f.owner.clone())?;
    let call = if let Some(item) = &item {
        ToolCall {
            id: "reviewed-read".into(),
            name: client_core::agent_page_tools::model_name("read_page", item)
                .ok_or("read tool missing")?,
            arguments: json!({"artifactId":item}),
            requires_review: true,
        }
    } else {
        ToolCall {
            id: "reviewed-history".into(),
            name: crate::agent::history_search::TOOL.into(),
            arguments: json!({"query":"Other source", "limit":1}),
            requires_review: true,
        }
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let database = f.path.join("agent.sqlite");
    f.close();
    rusqlite::Connection::open(&database)?.execute_batch(
        "CREATE TRIGGER fail_approval_delivery BEFORE INSERT ON outbox
         BEGIN SELECT RAISE(ABORT, 'synthetic publication failure'); END;",
    )?;
    f.reopen();
    let mut preserved = None;
    for _ in 0..4 {
        let cx = reviewed_context(&f, &calls, &call);
        assert_eq!(run(&f, &cx).await?, crate::agent::turn::Outcome::Retry);
        let outcome = known(&f, &cx)?;
        let TurnOutcome::ToolAsk { ask } = &outcome else {
            return Err("the exact held call was not preserved".into());
        };
        assert_eq!(ask.call, call);
        assert!(ask.steps.is_empty(), "no read before approval");
        assert!(f
            .client
            .tool_approval_requests(&f.space, &f.source)
            .is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let bytes = std::fs::read(&cx.journal)?;
        if let Some((old_outcome, old_bytes)) = &preserved {
            assert_eq!(&outcome, old_outcome);
            assert_eq!(
                &bytes, old_bytes,
                "delivery retries consume no model attempt"
            );
        } else {
            preserved = Some((outcome, bytes));
        }
        drop(cx);
        f.reopen();
    }
    f.close();
    rusqlite::Connection::open(&database)?.execute_batch("DROP TRIGGER fail_approval_delivery;")?;
    f.reopen();
    let cx = reviewed_context(&f, &calls, &call);
    assert_eq!(run(&f, &cx).await?, crate::agent::turn::Outcome::Handled);
    let requests = f.client.tool_approval_requests(&f.space, &f.source);
    assert_eq!(requests.len(), 1);
    let request = requests[0].clone();
    let proposal = request.tool_proposal.as_ref().ok_or("proposal missing")?;
    let recovered: ToolAsk = serde_json::from_str(&proposal.continuation_json)?;
    assert_eq!(
        preserved.as_ref().map(|(outcome, _)| outcome),
        Some(&TurnOutcome::ToolAsk { ask: recovered })
    );
    let dto = f
        .client
        .requests()
        .into_iter()
        .find(|r| r.id == request.id)
        .ok_or("approval missing")?;
    assert_eq!(dto.status, client_core::RequestStatus::Pending);
    assert!(JournalState::renewal_safe(
        &cx.journal,
        &request.id,
        &request.content_hash,
        &f.source
    )?);
    drop(cx);
    f.reopen();
    let cx = reviewed_context(&f, &calls, &call);
    assert_eq!(run(&f, &cx).await?, crate::agent::turn::Outcome::Handled);
    assert_eq!(
        f.client.tool_approval_requests(&f.space, &f.source).len(),
        1
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "still awaiting owner approval"
    );
    drop(cx);
    f.approve(&dto).await?;
    let cx = reviewed_context(&f, &calls, &call);
    assert_eq!(run(&f, &cx).await?, crate::agent::turn::Outcome::Handled);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "only the post-approval answer"
    );
    let reply_ids = |f: &Fixture| -> Result<Vec<String>, client_core::CoreError> {
        Ok(f.client
            .timeline(f.space.clone())?
            .into_iter()
            .filter(|entry| entry.reply_to.as_ref().is_some_and(|r| r.id == f.source))
            .map(|entry| entry.id)
            .collect())
    };
    let replies = reply_ids(&f)?;
    assert_eq!(replies.len(), 1);
    drop(cx);
    f.reopen();
    let cx = reviewed_context(&f, &calls, &call);
    assert_eq!(run(&f, &cx).await?, crate::agent::turn::Outcome::Handled);
    assert_eq!(reply_ids(&f)?, replies);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    if let Some(item) = item {
        assert_eq!(
            f.client.page(item)?.version,
            1,
            "read-only review never edits"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_review_stays_recoverable_without_inference_or_effects() -> TestResult {
    let mut f = Fixture::new("http://127.0.0.1:1")?;
    let calls = Arc::new(AtomicUsize::new(0));
    let call = ToolCall {
        id: "held-history-read".into(),
        name: crate::agent::history_search::TOOL.into(),
        arguments: json!({"query":"Other source", "limit":21}),
        requires_review: true,
    };
    for _ in 0..4 {
        let cx = reviewed_context(&f, &calls, &call);
        assert_eq!(run(&f, &cx).await?, crate::agent::turn::Outcome::Retry);
        assert!(
            matches!(known(&f, &cx)?, TurnOutcome::ToolAsk { ask } if ask.call == call && ask.steps.is_empty())
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(f
            .client
            .tool_approval_requests(&f.space, &f.source)
            .is_empty());
        assert!(f
            .client
            .timeline(f.space.clone())?
            .iter()
            .all(|entry| entry.reply_to.as_ref().is_none_or(|r| r.id != f.source)));
        drop(cx);
        f.reopen();
    }
    Ok(())
}

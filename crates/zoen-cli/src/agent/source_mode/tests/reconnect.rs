//! Actual signed source/request + Client/socket auth; no external provider or worker.
use super::*;
use crate::agent::connection::{self, HostConnection};
use client_core::CoreError;
use futures_util::FutureExt;

fn read_context(
    f: &Fixture,
    item: &str,
    starts: &Arc<AtomicUsize>,
    completed: &Arc<AtomicUsize>,
) -> Ctx {
    let mut cx = context(f, starts, false);
    let (item, starts, completed) = (item.to_owned(), starts.clone(), completed.clone());
    cx.runtime = Runtime::new(
        Gate {
            on_device: Some(Box::new(crate::agent::page_tool_tests::Allow)),
            ..Gate::default()
        },
        Box::new(ControlledToolBrain {
            evaluate: Box::new(move |steps| match steps {
                [] => {
                    starts.fetch_add(1, Ordering::SeqCst);
                    Ok(BrainOut::CallTools(vec![ToolCall {
                        id: "held-read".into(),
                        name: client_core::agent_page_tools::model_name("read_page", &item)
                            .unwrap(),
                        arguments: json!({"artifactId":item}),
                        requires_review: true,
                    }]))
                }
                [read] => {
                    assert!(!read.is_error);
                    completed.fetch_add(1, Ordering::SeqCst);
                    Ok(BrainOut::Say("Read the approved Page once.".into()))
                }
                _ => Err("unexpected source recovery".into()),
            }),
        }),
    );
    cx
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quiet_reconnect_waits_for_actual_ready_without_replaying_the_pending_source(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = relay(Mode::Normal).await;
    let mut f = Fixture::new(&server.url)?;
    f.publish_owner_packages(&server).await;
    server.released.store(true, Ordering::SeqCst);
    let item = f
        .client
        .page_import_markdown(f.space.clone(), "source.md".into(), "Public text\n".into())?
        .id;
    f.confirm_publications()?;
    server
        .histories
        .lock()
        .unwrap()
        .insert(f.space.clone(), f.history()?);
    online(&f.client, true).await;
    let (starts, completed) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let mut run = Run::prepare(&f.client, source(&f.source), &[])?;
    {
        let cx = read_context(&f, &item, &starts, &completed);
        assert!(!run.tick(&f.client, &cx, &definition(), deadline()).await?);
        assert!(run.take_activity().is_some());
        // Observing the newly staged request is one real canonical transition.
        // Only a subsequent unchanged pending head is an idle poll.
        assert!(!run.tick(&f.client, &cx, &definition(), deadline()).await?);
        let worked_at = run.take_activity().ok_or("pending head transition")?;
        assert!(!run.tick(&f.client, &cx, &definition(), deadline()).await?);
        assert!(run.take_activity().is_none(), "pending polls are not work");
        let mut quiet = HostConnection::new(worked_at - Duration::from_secs(11));
        quiet.activity(worked_at);
        quiet.maintain(&f.client, &Arc::new(Waker::default()), Instant::now());
        assert_eq!(f.client.connection().state, "online");
    }
    f.confirm_publications()?;
    server
        .histories
        .lock()
        .unwrap()
        .insert(f.space.clone(), f.history()?);
    online(&f.client, true).await;
    let request = f
        .client
        .requests()
        .into_iter()
        .find(|r| r.status == RequestStatus::Pending)
        .ok_or("pending read")?;
    let original_version = f.client.page(item.clone())?.version;
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    let before_state = server
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|c| **c == "agent_state")
        .count();
    server.auth.hold();
    let waker = Arc::new(Waker::default());
    let mut lifecycle = HostConnection::new(Instant::now() - Duration::from_secs(11));
    lifecycle.maintain(&f.client, &waker, Instant::now());
    tokio::time::timeout(Duration::from_secs(3), server.auth.reached()).await?;
    {
        let read = connection::source_state(&f.client, deadline(), f.client.agent_state());
        tokio::pin!(read);
        let early = read.as_mut().now_or_never();
        server.auth.release();
        assert!(
            early.is_none(),
            "protected state completed before authenticated Ready: {early:?}"
        );
        let state = read.await?;
        assert_eq!(state.status, "active");
        assert_eq!(state.kind, "User");
    }
    assert_eq!(
        server
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| **c == "agent_state")
            .count(),
        before_state + 1
    );
    {
        let cx = read_context(&f, &item, &starts, &completed);
        assert!(!run.tick(&f.client, &cx, &definition(), deadline()).await?);
    }
    assert_eq!(
        starts.load(Ordering::SeqCst),
        1,
        "reconnect must not rerun inference"
    );
    assert_eq!(
        f.client
            .requests()
            .iter()
            .filter(|r| r.id == request.id && r.status == RequestStatus::Pending)
            .count(),
        1
    );
    assert_eq!(f.client.page(item.clone())?.version, original_version);
    assert!(server.cursors.lock().unwrap().is_empty());
    assert!(!server.calls.lock().unwrap().contains(&"read_receipt"));

    // The same signed request can now be approved normally; reopening the host
    // never submits the source again or proposes a replacement request.
    f.approve(&request).await?;
    server
        .histories
        .lock()
        .unwrap()
        .insert(f.space.clone(), f.history()?);
    online(&f.client, true).await;
    let journal = f.path.join("source-steps.json");
    {
        let cx = read_context(&f, &item, &starts, &completed);
        assert!(!run.tick(&f.client, &cx, &definition(), deadline()).await?);
        f.client.wait_until_settled(3000).await?;
        assert!(run.tick(&f.client, &cx, &definition(), deadline()).await?);
    }
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(completed.load(Ordering::SeqCst), 1);
    let recorded = JournalState::load(&journal)?;
    assert_eq!(recorded.steps.len(), 1);
    assert_eq!(recorded.steps[0].0, f.source);
    assert_eq!(recorded.steps[0].1.len(), 1);
    assert_eq!(recorded.steps[0].1[0].call.id, "held-read");
    assert!(!recorded.steps[0].1[0].is_error);
    let accepted_journal = std::fs::read(&journal)?;
    let accepted_reply = f
        .client
        .timeline(f.space.clone())?
        .into_iter()
        .find(|entry| entry.reply_to.as_ref().is_some_and(|r| r.id == f.source))
        .ok_or("accepted own reply")?
        .id;
    f.reopen();
    online(&f.client, true).await;
    {
        let mut cold = Run::prepare(&f.client, source(&f.source), &[])?;
        let cx = read_context(&f, &item, &starts, &completed);
        assert!(cold.tick(&f.client, &cx, &definition(), deadline()).await?);
        assert!(cold.take_activity().is_none());
    }
    assert_eq!(std::fs::read(&journal)?, accepted_journal);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(completed.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.client.tool_approval_requests(&f.space, &f.source).len(),
        1
    );
    assert_eq!(
        f.client
            .timeline(f.space.clone())?
            .iter()
            .filter(|entry| entry.reply_to.as_ref().is_some_and(|r| r.id == f.source))
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        vec![accepted_reply.as_str()]
    );
    assert_eq!(f.client.page(item)?.version, original_version);
    assert!(server.cursors.lock().unwrap().is_empty());
    assert!(!server.calls.lock().unwrap().contains(&"read_receipt"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authentication_held_past_source_deadline_never_polls_or_submits_state(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = relay(Mode::Normal).await;
    let f = Fixture::new(&server.url)?;
    online(&f.client, true).await;
    server.auth.hold();
    let waker = Arc::new(Waker::default());
    let mut lifecycle = HostConnection::new(Instant::now() - Duration::from_secs(11));
    lifecycle.maintain(&f.client, &waker, Instant::now());
    tokio::time::timeout(Duration::from_secs(3), server.auth.reached()).await?;
    let polled = AtomicUsize::new(0);
    let end = Instant::now() + Duration::from_millis(100);
    let result = connection::source_state(&f.client, end, async {
        polled.fetch_add(1, Ordering::SeqCst);
        f.client.agent_state().await
    })
    .await;
    server.auth.release();
    assert_eq!(result.unwrap_err(), "agent_host_source_state_unavailable");
    assert_eq!(polled.load(Ordering::SeqCst), 0);
    assert!(!server.calls.lock().unwrap().contains(&"agent_state"));
    f.client.wait_until_settled(3000).await?;
    assert!(!server.calls.lock().unwrap().contains(&"agent_state"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expired_budget_never_polls_even_ready_state_and_refusals_are_not_retried(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = relay(Mode::Normal).await;
    let f = Fixture::new(&server.url)?;
    online(&f.client, true).await;
    let polled = AtomicUsize::new(0);
    let result = connection::source_state(
        &f.client,
        Instant::now() - Duration::from_millis(1),
        async {
            polled.fetch_add(1, Ordering::SeqCst);
            f.client.agent_state().await
        },
    )
    .await;
    assert_eq!(result.unwrap_err(), "agent_host_source_state_unavailable");
    assert_eq!(polled.load(Ordering::SeqCst), 0);
    assert!(!server.calls.lock().unwrap().contains(&"agent_state"));
    for error in [
        CoreError::Forbidden {
            reason: "private refusal".into(),
        },
        CoreError::Stale {
            reason: "private session".into(),
        },
        CoreError::Invalid {
            reason: "complete_read_busy".into(),
        },
    ] {
        let before = polled.load(Ordering::SeqCst);
        let result = connection::source_state(&f.client, deadline(), async {
            polled.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(error)
        })
        .await;
        assert_eq!(result, Err("agent_host_source_state_unavailable".into()));
        assert_eq!(polled.load(Ordering::SeqCst), before + 1);
    }
    Ok(())
}

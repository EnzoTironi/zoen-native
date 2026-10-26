//! Real signed Core + Runtime. Cold clients share only the production journal.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cold_host_restarts_cannot_reset_normal_source_model_attempts(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new("http://127.0.0.1:1")?;
    let calls = Arc::new(AtomicUsize::new(0));
    for attempt in 0..MAX_TRIES + 2 {
        // This adapter explicitly refuses before dispatch. Opaque failures have
        // a separate cold-restart regression and never regain this retry budget.
        let cx = super::uncertain_outcome::not_dispatched_context(&f, &calls);
        let timeline = f.client.timeline(f.space.clone()).unwrap();
        let index = timeline.iter().position(|t| t.id == f.source).unwrap();
        let outcome = super::super::super::turn::turn(
            &f.client,
            &cx,
            &definition(),
            &f.space,
            &timeline[..index],
            &timeline[index],
        )
        .await;
        assert_eq!(
            outcome,
            if attempt < MAX_TRIES {
                super::super::super::turn::Outcome::Retry
            } else {
                super::super::super::turn::Outcome::Handled
            }
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            (attempt + 1).min(MAX_TRIES) as usize
        );
        drop(cx);
        f.reopen();
    }
    let journal = f.path.join("source-steps.json");
    let state = JournalState::load(&journal).unwrap();
    assert_eq!(state.continuations.len(), 1);
    let (key, budget) = state.continuations.iter().next().unwrap();
    assert!(key.starts_with("turn_"));
    assert_eq!(budget.attempts, MAX_TRIES);
    assert_eq!(budget.source.as_deref(), Some(f.source.as_str()));
    assert!(!budget.registered);
    assert!(budget.progress.is_none() && budget.prepared.is_none());
    assert!(
        !JournalState::renewal_safe(&journal, key, &budget.content_hash, &f.source).unwrap(),
        "a source budget never proves a reviewed tool was not executed"
    );
    let encoded = std::fs::read_to_string(journal).unwrap();
    assert!(!encoded.contains("Inspect only the selected source."));
    assert!(!encoded.contains("synthetic unknown outcome"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn corrupt_or_busy_source_journal_refuses_before_model_without_becoming_handled(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for corrupt in [false, true] {
        let f = Fixture::new("http://127.0.0.1:1")?;
        let calls = Arc::new(AtomicUsize::new(0));
        let cx = context(&f, &calls, true);
        let path = if corrupt {
            cx.journal.clone()
        } else {
            cx.journal.with_extension("tmp")
        };
        std::fs::write(&path, b"interrupted or corrupt data").unwrap();
        for _ in 0..MAX_TRIES + 1 {
            let timeline = f.client.timeline(f.space.clone()).unwrap();
            let index = timeline.iter().position(|t| t.id == f.source).unwrap();
            assert_eq!(
                super::super::super::turn::turn(
                    &f.client,
                    &cx,
                    &definition(),
                    &f.space,
                    &timeline[..index],
                    &timeline[index]
                )
                .await,
                super::super::super::turn::Outcome::Retry
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(std::fs::read(path).unwrap(), b"interrupted or corrupt data");
        assert!(f
            .client
            .tool_approval_requests(&f.space, &f.source)
            .is_empty());
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_binding_change_cannot_reset_budget_but_another_source_has_its_own_limit(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = Fixture::new("http://127.0.0.1:1")?;
    let calls = Arc::new(AtomicUsize::new(0));
    let cx = context(&f, &calls, true);
    let timeline = f.client.timeline(f.space.clone()).unwrap();
    let original = timeline.iter().find(|t| t.id == f.source).unwrap();
    assert!(JournalState::begin_turn(&cx.journal, &cx, &f.space, original).unwrap());
    let before = std::fs::read(&cx.journal).unwrap();
    let mut changed = original.clone();
    if let EntryKind::Message { text, .. } = &mut changed.kind {
        *text = "Edited source cannot replace its retained attempt binding".into();
    } else {
        unreachable!();
    }
    assert!(JournalState::begin_turn(&cx.journal, &cx, &f.space, &changed).is_err());
    changed = original.clone();
    changed.author.id = "another-author".into();
    assert!(JournalState::begin_turn(&cx.journal, &cx, &f.space, &changed).is_err());
    assert_eq!(std::fs::read(&cx.journal).unwrap(), before);
    let other = timeline.iter().find(|t| t.id == f.other).unwrap();
    assert!(JournalState::begin_turn(&cx.journal, &cx, &f.space, other).unwrap());
    assert_eq!(
        JournalState::load(&cx.journal).unwrap().continuations.len(),
        2
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn acknowledged_source_retirement_preserves_other_sources_and_review_budgets(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = Fixture::new("http://127.0.0.1:1")?;
    let calls = Arc::new(AtomicUsize::new(0));
    let cx = context(&f, &calls, true);
    let timeline = f.client.timeline(f.space.clone()).unwrap();
    let source = timeline.iter().find(|t| t.id == f.source).unwrap();
    let other = timeline.iter().find(|t| t.id == f.other).unwrap();
    assert!(JournalState::begin_turn(&cx.journal, &cx, &f.space, source).unwrap());
    assert!(JournalState::begin_turn(&cx.journal, &cx, &f.space, other).unwrap());
    JournalState::register_hold(&cx.journal, "signed-review", &"ab".repeat(32), &f.source).unwrap();
    JournalState::retire_turn(&cx.journal, &cx, &f.space, &f.source).unwrap();
    let state = JournalState::load(&cx.journal).unwrap();
    assert_eq!(state.continuations.len(), 2);
    assert_eq!(state.continuations["signed-review"].attempts, 0);
    assert!(state.continuations["signed-review"].registered);
    assert!(state.continuations.iter().any(|(key, entry)| {
        key.starts_with("turn_") && entry.source.as_deref() == Some(f.other.as_str())
    }));
    let before = std::fs::read(&cx.journal).unwrap();
    JournalState::retire_turn(&cx.journal, &cx, &f.space, &f.source).unwrap();
    assert_eq!(std::fs::read(&cx.journal).unwrap(), before);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canonical_cursor_ack_and_cold_selector_do_not_replay_a_retired_silent_turn(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let server = relay(Mode::CursorAck).await;
    let mut f = Fixture::new(&server.url)?;
    f.publish_owner_packages(&server).await;
    server.released.store(true, Ordering::SeqCst);
    online(&f.client, true).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let cx = context(&f, &calls, false);
    let cursors = super::super::admit(&f.client, 1000).await.unwrap();
    let mut run = Run::prepare(&f.client, source(&f.source), &cursors).unwrap();
    assert!(!run
        .tick(&f.client, &cx, &definition(), deadline())
        .await
        .unwrap());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let timeline = f.client.timeline(f.space.clone()).unwrap();
    let seq = timeline.iter().find(|t| t.id == f.source).unwrap().seq;
    assert!(seq > 0);
    assert!(timeline
        .iter()
        .all(|t| t.reply_to.as_ref().is_none_or(|r| r.id != f.source)));
    assert!(owner_note(&f.client, &cx, &f.source).is_none());
    f.client.wait_until_settled(3000).await.unwrap();
    // The ordinary host's order: actual relay ACK first, then retire local budget.
    f.client.agent_advance(f.space.clone(), seq).await.unwrap();
    JournalState::retire_turn(&cx.journal, &cx, &f.space, &f.source).unwrap();
    assert!(JournalState::load(&cx.journal)
        .unwrap()
        .continuations
        .is_empty());
    drop(cx);
    f.reopen();
    online(&f.client, true).await;
    let cursors = super::super::admit(&f.client, 1000).await.unwrap();
    assert_eq!(cursors, vec![(f.space.clone(), seq)]);
    let cx = context(&f, &calls, false);
    let before = std::fs::read(&cx.journal).unwrap();
    for _ in 0..MAX_TRIES + 1 {
        let mut cold = Run::prepare(&f.client, source(&f.source), &cursors).unwrap();
        // A missing later snapshot cannot undo completion already observed.
        cold.observe_cursors(&[]);
        assert_eq!(
            cold.tick(&f.client, &cx, &definition(), deadline()).await,
            Err("agent_host_source_already_handled".into())
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read(&cx.journal).unwrap(), before);
    assert_eq!(
        server
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|&&c| c == "agent_advance")
            .count(),
        1
    );
    assert!(!server
        .calls
        .lock()
        .unwrap()
        .contains(&"forbidden_agent_operation"));
    assert!(!server.calls.lock().unwrap().contains(&"read_receipt"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn account_retirement_during_inference_prevents_result_delivery(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let server = relay(Mode::Normal).await;
    let f = Fixture::new(&server.url)?;
    online(&f.client, true).await;
    let mut run = Run::prepare(&f.client, source(&f.source), &[]).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut cx = context(&f, &calls, false);
    let client = f.client.clone();
    let observed = calls.clone();
    let before_retirement = Arc::new(Mutex::new(None));
    let captured = before_retirement.clone();
    let journal = cx.journal.clone();
    cx.runtime = Runtime::new(
        Gate::default(),
        Box::new(ControlledToolBrain {
            evaluate: Box::new(move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
                *captured.lock().unwrap() = Some(std::fs::read(&journal).unwrap());
                client.reset_demo().unwrap(); // own synthetic account only
                Ok(BrainOut::Say(
                    "Must never be sent after account retirement.".into(),
                ))
            }),
        }),
    );
    assert_eq!(
        run.tick(&f.client, &cx, &definition(), deadline()).await,
        Err("agent_host_source_account_unavailable".into())
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        std::fs::read(&cx.journal).unwrap(),
        before_retirement.lock().unwrap().as_ref().unwrap().clone(),
        "account retirement cannot checkpoint or replace the pre-inference reservation"
    );
    let state = JournalState::load(&cx.journal).unwrap();
    assert!(state.steps.is_empty());
    assert_eq!(state.continuations.len(), 1);
    assert_eq!(state.continuations.values().next().unwrap().attempts, 1);
    Ok(())
}

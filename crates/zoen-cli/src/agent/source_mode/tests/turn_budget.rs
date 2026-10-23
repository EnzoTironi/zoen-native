//! Real signed Core + Runtime. Cold clients share only the production journal.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cold_host_restarts_cannot_reset_normal_source_model_attempts() {
    let mut f = Fixture::new("http://127.0.0.1:1");
    let calls = Arc::new(AtomicUsize::new(0));
    for attempt in 0..MAX_TRIES + 2 {
        let cx = context(&f, &calls, true);
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn corrupt_or_busy_source_journal_refuses_before_model_without_becoming_handled() {
    for corrupt in [false, true] {
        let f = Fixture::new("http://127.0.0.1:1");
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_binding_change_cannot_reset_budget_but_another_source_has_its_own_limit() {
    let f = Fixture::new("http://127.0.0.1:1");
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
}

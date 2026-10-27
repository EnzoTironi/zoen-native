//! An unanswered model invocation is not proof that no inference happened.
use super::*;

pub(super) fn not_dispatched_context(f: &Fixture, calls: &Arc<AtomicUsize>) -> Ctx {
    let mut cx = context(f, calls, false);
    let calls = calls.clone();
    cx.runtime = Runtime::new(
        Gate::default(),
        Box::new(ControlledToolBrain {
            evaluate: Box::new(move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Err(zoen_agentd::BrainError::NotDispatched(
                    "synthetic pre-dispatch refusal".into(),
                ))
            }),
        }),
    );
    cx
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cold_source_retry_does_not_repeat_a_completed_inference_with_a_lost_response(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new("http://127.0.0.1:1")?;
    let completed = Arc::new(AtomicUsize::new(0));
    let mut retained = None;
    for _ in 0..2 {
        // The controlled model counts its completed inference, then loses its
        // response. The caller has no provider proof that this was not dispatched.
        let cx = context(&f, &completed, true);
        let timeline = f.client.timeline(f.space.clone())?;
        let index = timeline
            .iter()
            .position(|entry| entry.id == f.source)
            .unwrap();
        assert_eq!(
            super::super::super::turn::turn(
                &f.client,
                &cx,
                &definition(),
                &f.space,
                &timeline[..index],
                &timeline[index],
            )
            .await,
            super::super::super::turn::Outcome::Retry
        );
        assert_eq!(completed.load(Ordering::SeqCst), 1);
        let bytes = std::fs::read(&cx.journal)?;
        if let Some(prior) = &retained {
            assert_eq!(
                &bytes, prior,
                "a cold retry must retain the original attempt"
            );
        } else {
            retained = Some(bytes);
        }
        assert!(f.client.timeline(f.space.clone())?.iter().all(|entry| {
            entry
                .reply_to
                .as_ref()
                .is_none_or(|reply| reply.id != f.source)
        }));
        drop(cx);
        f.reopen();
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn legacy_source_reservation_is_unknown_until_reconciled(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = Fixture::new("http://127.0.0.1:1")?;
    let calls = Arc::new(AtomicUsize::new(0));
    let cx = context(&f, &calls, true);
    let timeline = f.client.timeline(f.space.clone())?;
    let index = timeline
        .iter()
        .position(|entry| entry.id == f.source)
        .unwrap();
    JournalState::begin_turn(&cx.journal, &cx, &f.space, &timeline[index])?;
    let mut legacy: serde_json::Value = serde_json::from_slice(&std::fs::read(&cx.journal)?)?;
    for entry in legacy["continuations"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        entry.as_object_mut().unwrap().remove("model_outcome");
    }
    let original = serde_json::to_vec(&legacy)?;
    std::fs::write(&cx.journal, &original)?;
    assert_eq!(
        super::super::super::turn::turn(
            &f.client,
            &cx,
            &definition(),
            &f.space,
            &timeline[..index],
            &timeline[index],
        )
        .await,
        super::super::super::turn::Outcome::Retry
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(std::fs::read(&cx.journal)?, original);
    Ok(())
}

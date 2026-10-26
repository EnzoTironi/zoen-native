//! Actual source admission against a mutable authenticated citizen record.
use super::*;
type R = Result<(), Box<dyn std::error::Error>>;

fn stopping_context(
    f: &Fixture,
    calls: &Arc<AtomicUsize>,
    lifecycle: Arc<Mutex<&'static str>>,
    stopped: &'static str,
) -> Ctx {
    let mut cx = context(f, calls, false);
    let count = calls.clone();

    cx.runtime = Runtime::new(
        Gate::default(),
        Box::new(ControlledToolBrain {
            evaluate: Box::new(move |_| {
                if count.fetch_add(1, Ordering::SeqCst) == 0 {
                    *lifecycle.lock().map_err(|_| {
                        zoen_agentd::BrainError::NotDispatched("fixture lock".into())
                    })? = stopped;
                }
                Ok(BrainOut::Silence)
            }),
        }),
    );
    cx
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_change_between_sources_defers_backlog_and_resume_runs_once() -> R {
    for stopped in ["paused", "revoked", "error"] {
        let server = relay(Mode::Normal).await;
        let f = Fixture::new(&server.url)?;
        online(&f.client, true).await;
        let state = f.client.agent_state().await?;
        let def = Definition::decode(&state.definition)?;
        let calls = Arc::new(AtomicUsize::new(0));
        let cx = stopping_context(&f, &calls, server.lifecycle.clone(), stopped);
        let timeline = f.client.timeline(f.space.clone())?;
        let first = timeline
            .iter()
            .position(|entry| entry.id == f.other)
            .ok_or("first source")?;
        let next = timeline
            .iter()
            .position(|entry| entry.id == f.source)
            .ok_or("next source")?;
        assert_eq!(
            crate::agent::turn::admitted(
                &f.client,
                &cx,
                &def,
                &f.space,
                &timeline[..first],
                &timeline[first]
            )
            .await,
            crate::agent::turn::Outcome::Handled
        );
        let before = std::fs::read(&cx.journal)?;
        assert_eq!(
            crate::agent::turn::admitted(
                &f.client,
                &cx,
                &def,
                &f.space,
                &timeline[..next],
                &timeline[next]
            )
            .await,
            crate::agent::turn::Outcome::Deferred
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(std::fs::read(&cx.journal)?, before);
        assert!(server.cursors.lock().map_err(|_| "cursor lock")?.is_empty());
        if stopped != "revoked" {
            *server.lifecycle.lock().map_err(|_| "lifecycle lock")? = "active";
            for _ in 0..2 {
                assert_eq!(
                    crate::agent::turn::admitted(
                        &f.client,
                        &cx,
                        &def,
                        &f.space,
                        &timeline[..next],
                        &timeline[next]
                    )
                    .await,
                    crate::agent::turn::Outcome::Handled
                );
            }
            assert_eq!(calls.load(Ordering::SeqCst), 2);
        }
        f.client.stop_sync();
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_pause_and_revoke_do_not_consume_an_approved_continuation() -> R {
    for stopped in ["paused", "revoked", "error"] {
        let server = relay(Mode::Normal).await;
        let mut f = Fixture::new(&server.url)?;
        online(&f.client, true).await;
        f.client.stop_sync();
        // Existing inert fixture: real signed approval, no editorial worker or tool IO.
        let base = deck(&f);
        let request = stage_review(&f, &base.item_id, &f.source, true);
        f.approve(&request).await?;
        online(&f.client, true).await;
        let state = f.client.agent_state().await?;
        let def = Definition::decode(&state.definition)?;
        let calls = Arc::new(AtomicUsize::new(0));
        let cx = stopping_context(&f, &calls, server.lifecycle.clone(), stopped);
        let timeline = f.client.timeline(f.space.clone())?;
        let index = timeline
            .iter()
            .position(|entry| entry.id == f.source)
            .ok_or("approved source")?;
        let requests = serde_json::to_value(f.client.requests())?;
        let first = timeline
            .iter()
            .position(|entry| entry.id == f.other)
            .ok_or("first source")?;
        assert_eq!(
            crate::agent::turn::admitted(
                &f.client,
                &cx,
                &def,
                &f.space,
                &timeline[..first],
                &timeline[first]
            )
            .await,
            crate::agent::turn::Outcome::Handled
        );
        let journal_before = std::fs::read(&cx.journal)?;
        assert_eq!(
            crate::agent::turn::admitted(
                &f.client,
                &cx,
                &def,
                &f.space,
                &timeline[..index],
                &timeline[index]
            )
            .await,
            crate::agent::turn::Outcome::Deferred
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(std::fs::read(&cx.journal)?, journal_before);
        assert_eq!(serde_json::to_value(f.client.requests())?, requests);
        assert!(f.client.approved_tool_sources(&f.space).contains(&f.source));
        assert_eq!(
            f.client.app_binary_read(base.item_id, None, None)?.version,
            base.version
        );
        assert!(server.cursors.lock().map_err(|_| "cursor lock")?.is_empty());
        f.client.stop_sync();
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_admission_rejects_stale_definition_and_actor_without_reservation() -> R {
    let server = relay(Mode::Normal).await;
    let f = Fixture::new(&server.url)?;
    online(&f.client, true).await;
    let state = f.client.agent_state().await?;
    let mut def = Definition::decode(&state.definition)?;
    def.version += 1;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut cx = context(&f, &calls, false);
    let timeline = f.client.timeline(f.space.clone())?;
    let index = timeline
        .iter()
        .position(|entry| entry.id == f.source)
        .ok_or("source")?;
    assert_eq!(
        crate::agent::turn::admitted(
            &f.client,
            &cx,
            &def,
            &f.space,
            &timeline[..index],
            &timeline[index]
        )
        .await,
        crate::agent::turn::Outcome::Deferred
    );
    assert!(!cx.journal.exists());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let before = server.calls.lock().map_err(|_| "calls lock")?.len();
    cx.device = "f".repeat(64);
    assert!(!crate::agent::turn::current(&f.client, &cx, &def).await);
    assert_eq!(server.calls.lock().map_err(|_| "calls lock")?.len(), before);
    f.client.stop_sync();
    Ok(())
}

//! A process dies at the existing before-send fault after one completed inference.
use super::*;
use std::process::Command;

const CHILD: &str = "ZOEN_KNOWN_RESULT_CHILD";
const EXACT: &str = "Known answer: preserve these exact words after a cold restart.";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn known_result_fault_child() -> Result<(), Box<dyn std::error::Error>> {
    let Some(config) = std::env::var_os(CHILD) else {
        return Ok(());
    };
    let input: serde_json::Value = serde_json::from_slice(&std::fs::read(config)?)?;
    let path = Path::new(input["path"].as_str().ok_or("missing path")?);
    let vault = Arc::new(relay::Vault::default());
    *vault.0.lock().map_err(|_| "vault poisoned")? =
        serde_json::from_value(input["vault"].clone())?;
    let e = Client::open(
        path.join("agent.sqlite").to_string_lossy().into_owned(),
        "en".into(),
    )?;
    assert!(e.unlock(vault)?);
    let account = e.account().ok_or("account absent")?;
    let count = path.join("brain-count");
    let cx = Ctx {
        me: account.identity_id,
        device: account.device_id,
        owner: input["owner"].as_str().ok_or("missing owner")?.into(),
        journal: path.join("source-steps.json"),
        deckcraft: None,
        runtime: Runtime::new(
            Gate::default(),
            Box::new(ControlledToolBrain {
                evaluate: Box::new(move |_| {
                    use std::io::Write;
                    let mut file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&count)
                        .map_err(|e| zoen_agentd::BrainError::OutcomeUnknown(e.to_string()))?;
                    file.write_all(b"1")
                        .map_err(|e| zoen_agentd::BrainError::OutcomeUnknown(e.to_string()))?;
                    Ok(BrainOut::Say(EXACT.into()))
                }),
            }),
        ),
    };
    let space = input["space"].as_str().ok_or("missing space")?;
    let source = input["source"].as_str().ok_or("missing source")?;
    let timeline = e.timeline(space.into())?;
    let index = timeline
        .iter()
        .position(|entry| entry.id == source)
        .ok_or("source absent")?;
    super::super::super::turn::turn(
        &e,
        &cx,
        &definition(),
        space,
        &timeline[..index],
        &timeline[index],
    )
    .await;
    Err("the before-send fault did not stop the child".into())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cold_restart_delivers_exact_known_result_without_reinference(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new("http://127.0.0.1:1")?;
    let config = f.path.join("child.json");
    std::fs::write(
        &config,
        serde_json::to_vec(&json!({
            "path":f.path, "space":f.space, "source":f.source, "owner":f.owner,
            "vault":*f.vault.0.lock().map_err(|_| "vault poisoned")?
        }))?,
    )?;
    f.close();
    let status = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "agent::source_mode::tests::known_result::known_result_fault_child",
            "--nocapture",
        ])
        .env(CHILD, &config)
        .env("ZOEN_AGENT_FAULT", "before-send")
        .env("ZOEN_AGENT_FAULT_MARKER", f.path.join("fault-fired"))
        .status()?;
    assert_eq!(status.code(), Some(86));
    assert_eq!(std::fs::read(f.path.join("brain-count"))?, b"1");
    std::fs::remove_file(config)?;
    f.reopen();
    let calls = Arc::new(AtomicUsize::new(0));
    let cx = context(&f, &calls, true);
    let timeline = f.client.timeline(f.space.clone())?;
    assert!(timeline.iter().all(|entry| entry
        .reply_to
        .as_ref()
        .is_none_or(|reply| reply.id != f.source)));

    let index = timeline
        .iter()
        .position(|entry| entry.id == f.source)
        .ok_or("source absent")?;
    assert_eq!(
        JournalState::known_turn(&cx.journal, &cx, &f.space, &timeline[index], &definition())?,
        Some(TurnOutcome::Say { text: EXACT.into() })
    );
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
        super::super::super::turn::Outcome::Handled
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let replies: Vec<_> = f
        .client
        .timeline(f.space.clone())?
        .into_iter()
        .filter(|entry| {
            entry
                .reply_to
                .as_ref()
                .is_some_and(|reply| reply.id == f.source)
        })
        .collect();
    assert_eq!(replies.len(), 1);
    assert_eq!(text_of(&replies[0]), Some(EXACT));
    assert_eq!(std::fs::read(f.path.join("brain-count"))?, b"1");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn known_result_is_bound_and_cannot_be_replaced_or_used_as_tool_recovery(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = Fixture::new("http://127.0.0.1:1")?;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut cx = context(&f, &calls, false);
    let timeline = f.client.timeline(f.space.clone())?;
    let original = timeline
        .iter()
        .find(|entry| entry.id == f.source)
        .ok_or("source absent")?;
    let outcome = TurnOutcome::Say { text: EXACT.into() };
    assert!(JournalState::begin_turn(
        &cx.journal,
        &cx,
        &f.space,
        original
    )?);
    assert!(
        JournalState::known_turn(&cx.journal, &cx, &f.space, original, &definition())?.is_none()
    );
    JournalState::model_known(
        &cx.journal,
        &cx,
        &f.space,
        original,
        &outcome,
        &definition(),
    )?;
    let saved = std::fs::read(&cx.journal)?;
    let mut changed_policy = definition();
    changed_policy.persona.push_str(" changed policy");
    assert!(
        JournalState::known_turn(&cx.journal, &cx, &f.space, original, &changed_policy).is_err()
    );
    assert_eq!(
        super::super::super::turn::turn(&f.client, &cx, &changed_policy, &f.space, &[], original)
            .await,
        super::super::super::turn::Outcome::Retry
    );
    assert!(f.client.timeline(f.space.clone())?.iter().all(|entry| entry
        .reply_to
        .as_ref()
        .is_none_or(|reply| reply.id != f.source)));
    let prior_threshold = cx.runtime.gate.thresholds.min_confidence;
    cx.runtime.gate.thresholds.min_confidence = prior_threshold + 0.01;
    assert!(JournalState::known_turn(&cx.journal, &cx, &f.space, original, &definition()).is_err());
    cx.runtime.gate.thresholds.min_confidence = prior_threshold;
    let mut legacy: serde_json::Value = serde_json::from_slice(&saved)?;
    let reservations = legacy["continuations"]
        .as_object_mut()
        .ok_or("missing reservations")?;
    for reservation in reservations.values_mut() {
        reservation["model_outcome"]["known"]
            .as_object_mut()
            .ok_or("missing known result")?
            .remove("policy_hash");
    }
    let legacy_bytes = serde_json::to_vec(&legacy)?;
    std::fs::write(&cx.journal, &legacy_bytes)?;
    assert_eq!(
        super::super::super::turn::turn(&f.client, &cx, &definition(), &f.space, &[], original)
            .await,
        super::super::super::turn::Outcome::Retry
    );
    assert_eq!(std::fs::read(&cx.journal)?, legacy_bytes);
    std::fs::write(&cx.journal, &saved)?;

    assert!(JournalState::begin_turn(&cx.journal, &cx, &f.space, original).is_err());
    assert!(JournalState::model_known(
        &cx.journal,
        &cx,
        &f.space,
        original,
        &TurnOutcome::Silence,
        &definition()
    )
    .is_err());
    let mut changed = original.clone();
    changed.author.id.push_str("-other");
    assert!(JournalState::known_turn(&cx.journal, &cx, &f.space, &changed, &definition()).is_err());
    changed = original.clone();
    let EntryKind::Message { text, .. } = &mut changed.kind else {
        return Err("source is not a message".into());
    };
    text.push_str(" changed");
    assert!(JournalState::known_turn(&cx.journal, &cx, &f.space, &changed, &definition()).is_err());
    changed = original.clone();
    changed.id.push_str("-other");
    assert!(
        JournalState::known_turn(&cx.journal, &cx, &f.space, &changed, &definition())?.is_none()
    );
    assert!(
        JournalState::known_turn(&cx.journal, &cx, "other-space", original, &definition())?
            .is_none()
    );
    for field in 0..3 {
        let value = match field {
            0 => &mut cx.me,
            1 => &mut cx.device,
            _ => &mut cx.owner,
        };
        let previous = value.clone();
        value.push_str("-other");
        assert!(
            JournalState::known_turn(&cx.journal, &cx, &f.space, original, &definition())?
                .is_none()
        );
        match field {
            0 => cx.me = previous,
            1 => cx.device = previous,
            _ => cx.owner = previous,
        };
    }
    let state = JournalState::load(&cx.journal)?;
    let (key, budget) = state
        .continuations
        .iter()
        .next()
        .ok_or("missing reservation")?;
    assert!(!JournalState::renewal_safe(
        &cx.journal,
        key,
        &budget.content_hash,
        &f.source
    )?);
    assert_eq!(std::fs::read(&cx.journal)?, saved);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_known_checkpoint_cannot_deliver_or_clear_unknown_after_restart(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new("http://127.0.0.1:1")?;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut cx = context(&f, &calls, false);
    let temporary = cx.journal.with_extension("tmp");
    let blocked = temporary.clone();
    let observed = calls.clone();
    cx.runtime = Runtime::new(
        Gate::default(),
        Box::new(ControlledToolBrain {
            evaluate: Box::new(move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
                std::fs::write(&blocked, b"synthetic competing writer")
                    .map_err(|e| zoen_agentd::BrainError::OutcomeUnknown(e.to_string()))?;
                Ok(BrainOut::Say(EXACT.into()))
            }),
        }),
    );
    for cold in [false, true] {
        if cold {
            drop(cx);
            std::fs::remove_file(&temporary)?;
            f.reopen();
            cx = context(&f, &calls, true);
        }
        let timeline = f.client.timeline(f.space.clone())?;
        let index = timeline
            .iter()
            .position(|entry| entry.id == f.source)
            .ok_or("source absent")?;
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
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(JournalState::known_turn(
            &cx.journal,
            &cx,
            &f.space,
            &timeline[index],
            &definition()
        )?
        .is_none());
        assert!(f.client.timeline(f.space.clone())?.iter().all(|entry| entry
            .reply_to
            .as_ref()
            .is_none_or(|reply| reply.id != f.source)));
    }
    Ok(())
}

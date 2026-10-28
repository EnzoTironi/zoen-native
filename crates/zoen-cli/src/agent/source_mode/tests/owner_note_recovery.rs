//! Recovery receipts come from a structured, signed note, not a text substring.
use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

async fn handle(f: &Fixture, cx: &Ctx) -> super::super::super::turn::Outcome {
    let timeline = f.client.timeline(f.space.clone()).expect("signed timeline");
    let index = timeline
        .iter()
        .position(|entry| entry.id == f.source)
        .expect("source");
    super::super::super::turn::turn(
        &f.client,
        cx,
        &definition(),
        &f.space,
        &timeline[..index],
        &timeline[index],
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unrelated_owner_messages_cannot_mark_a_source_handled() -> TestResult {
    for variant in [
        "malformed",
        "unfenced",
        "unknown",
        "other-space",
        "ambiguous",
        "bad-proposal",
        "other-author",
        "wrong-source-kind",
    ] {
        let f = Fixture::new("http://127.0.0.1:1")?;
        let calls = Arc::new(AtomicUsize::new(0));
        let cx = context(&f, &calls, false);
        let note = json!({"ask_owner":{"space":f.space,"source":f.source,"from":f.owner,"draft":"A held draft","rule":"synthetic rule"}});
        let text = match variant {
            "malformed" => format!("```json\n{{\"source\":\"{}\", broken\n```", f.source),
            "unfenced" => note.to_string(),
            "unknown" => format!(
                "```json\n{}\n```",
                json!({"ordinary_data":{"source":f.source}})
            ),
            "other-space" => {
                let mut wrong = note.clone();
                wrong["ask_owner"]["space"] = "different-conversation".into();
                format!("```json\n{wrong}\n```")
            }
            "ambiguous" => {
                let mut wrong = note.clone();
                wrong["ordinary_data"] = json!({});
                format!("```json\n{wrong}\n```")
            }
            "other-author" => {
                let mut wrong = note.clone();
                wrong["ask_owner"]["from"] = "different-author".into();
                format!("```json\n{wrong}\n```")
            }
            "wrong-source-kind" => format!(
                "```json\n{}\n```",
                json!({"booking":{"space":f.space,"source":f.source}})
            ),
            "bad-proposal" => format!(
                "```json\n{}\n```",
                json!({"agent_proposal":{"space":f.space,"source":f.source,"spec":{},"confirmation":"Not an actual proposal"}})
            ),
            _ => unreachable!("bounded variants"),
        };
        let dm = f.client.start_direct(f.owner.clone())?;
        let id = f.client.send_message(dm.clone(), text.clone())?.id;
        assert_eq!(
            handle(&f, &cx).await,
            super::super::super::turn::Outcome::Handled
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "{variant} must not suppress the source"
        );
        let note_after = f
            .client
            .timeline(dm)?
            .into_iter()
            .find(|entry| entry.id == id)
            .expect("preserved note");
        assert_eq!(text_of(&note_after), Some(text.as_str()));
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exact_owner_note_recovers_after_reopen_without_model_or_duplicate_note() -> TestResult {
    let mut f = Fixture::new("http://127.0.0.1:1")?;
    let calls = Arc::new(AtomicUsize::new(0));
    let cx = context(&f, &calls, false);
    let note = json!({"ask_owner":{"space":f.space,"source":f.source,"from":f.owner,"draft":"Exact café🦊 draft","rule":"synthetic rule"}});
    assert!(tell_owner(&f.client, &cx, note, &f.space, &f.source));
    let dm = f.client.start_direct(f.owner.clone())?;
    let before = serde_json::to_value(f.client.timeline(dm.clone())?)?;
    drop(cx);
    f.reopen();
    let cx = context(&f, &calls, false);
    assert_eq!(
        handle(&f, &cx).await,
        super::super::super::turn::Outcome::Handled
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!cx.journal.exists());
    assert_eq!(serde_json::to_value(f.client.timeline(dm)?)?, before);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pretty_proposal_recovers_its_exact_confirmation_only_once() -> TestResult {
    let mut f = Fixture::new("http://127.0.0.1:1")?;
    let calls = Arc::new(AtomicUsize::new(0));
    let confirmation = "Proposta café🦊 preservada.";
    let spec = AgentSpec {
        name: "Proposed agent".into(),
        handle: "proposed_agent".into(),
        glyph: "🦊".into(),
        tint_hex: "#19B5A5".into(),
        definition: definition(),
    };
    let note = json!({"agent_proposal":{"space":f.space,"source":f.source,"spec":spec,"confirmation":confirmation}});
    let dm = f.client.start_direct(f.owner.clone())?;
    let original = f.client.send_message(
        dm.clone(),
        format!("```json\n{}\n```", serde_json::to_string_pretty(&note)?),
    )?;
    f.reopen();
    let cx = context(&f, &calls, false);
    assert_eq!(
        handle(&f, &cx).await,
        super::super::super::turn::Outcome::Handled
    );
    let first = f.client.timeline(f.space.clone())?;
    let replies: Vec<_> = first
        .iter()
        .filter(|entry| entry.reply_to.as_ref().is_some_and(|q| q.id == f.source))
        .collect();
    assert_eq!(replies.len(), 1);
    assert_eq!(text_of(replies[0]), Some(confirmation));
    drop(cx);
    f.reopen();
    let cx = context(&f, &calls, false);
    assert_eq!(
        handle(&f, &cx).await,
        super::super::super::turn::Outcome::Handled
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!cx.journal.exists());
    assert_eq!(
        serde_json::to_value(f.client.timeline(f.space.clone())?)?,
        serde_json::to_value(first)?
    );
    let notes = f.client.timeline(dm)?;
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].id, original.id);
    assert_eq!(text_of(&notes[0]), text_of(&original));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unavailable_owner_history_is_retry_without_inference_or_journal() -> TestResult {
    let f = Fixture::new("http://127.0.0.1:1")?;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut cx = context(&f, &calls, false);
    cx.owner = "unavailable-owner-profile".into();
    let source = f
        .client
        .timeline(f.space.clone())?
        .into_iter()
        .find(|entry| entry.id == f.source)
        .ok_or("missing source")?;
    assert!(owner_note(&f.client, &cx, &f.space, &source).is_err());
    assert_eq!(
        handle(&f, &cx).await,
        super::super::super::turn::Outcome::Retry
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!cx.journal.exists());
    Ok(())
}

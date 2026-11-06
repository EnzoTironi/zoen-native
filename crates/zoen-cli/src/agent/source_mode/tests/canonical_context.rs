//! Revisions survive host recovery; a stale wake is never fresh model input.
use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn edited_then_restored_source_cannot_deliver_an_old_retained_response() -> TestResult {
    let mut f = Fixture::new("http://127.0.0.1:1")?;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut cx = context(&f, &calls, true);
    let original = f
        .client
        .timeline(f.space.clone())?
        .into_iter()
        .find(|entry| entry.id == f.source)
        .ok_or("source")?;
    let original_text = text_of(&original).ok_or("text")?.to_owned();
    assert!(JournalState::begin_turn(
        &cx.journal,
        &cx,
        &f.space,
        &original
    )?);
    JournalState::model_known(
        &cx.journal,
        &cx,
        &f.space,
        &original,
        &TurnOutcome::Say {
            text: "Retained old answer".into(),
        },
        &definition(),
    )?;
    for text in ["Changed input".to_owned(), original_text] {
        f.append(
            types::EventBody::MessageAnnotated {
                target: f.source.clone(),
                note: types::Annotation::Edit { text },
            },
            true,
        )?;
    }
    cx = context(&f, &calls, true);
    let revised = f
        .client
        .timeline(f.space.clone())?
        .into_iter()
        .find(|entry| entry.id == f.source)
        .ok_or("source")?;
    assert_eq!(text_of(&original), text_of(&revised));
    assert_ne!(original.text_revision, revised.text_revision);
    assert!(JournalState::known_turn(&cx.journal, &cx, &f.space, &revised, &definition()).is_err());
    for selected in [&original, &revised] {
        assert_eq!(
            crate::agent::turn::turn(&f.client, &cx, &definition(), &f.space, &[], selected).await,
            crate::agent::turn::Outcome::Retry
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(f.client.timeline(f.space.clone())?.iter().all(|entry| entry
        .reply_to
        .as_ref()
        .is_none_or(|reply| reply.id != f.source)));
    Ok(())
}

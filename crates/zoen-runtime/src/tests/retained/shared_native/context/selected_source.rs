//! Selected requests never discover or dispatch unrelated backlog.
use super::*;
use crate::{
    ReplyHostConfig, ReplyHostProgress, ReplyRuntimeHost, ReplySource, SelectedReplyWorker,
};

async fn source(f: &mut Fixture, text: &str) -> Result<ReplySource, Box<dyn std::error::Error>> {
    let message = types::new_id("selected")?;
    f.encrypted_named(
        EventBody::MessagePosted {
            message: types::new_id("business-message")?,
            text: text.into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
            forwarded_author: None,
        },
        &message,
    )
    .await?;
    Ok(ReplySource {
        space: f.space.clone(),
        message,
        author: f.owner.id.clone(),
        text_sha256: crate::hash(text.as_bytes()),
    })
}
fn host(f: &Fixture, relay: &Relay) -> Result<ReplyRuntimeHost, RuntimeError> {
    ReplyRuntimeHost::configured(
        f.world.runtime.clone(),
        ReplyHostConfig {
            agent: f.agent.id.clone(),
            device: f.world.agents[0]
                .device
                .clone()
                .ok_or(RuntimeError::InvalidBinding)?,
            target: relay.target(f, relay.address),
        },
        gate::Gate::default(),
    )
}
pub(in crate::tests) async fn run() -> TestResult {
    Box::pin(one_source_and_cold_retry()).await?;
    Box::pin(refuse_substitution()).await?;
    Ok(())
}
async fn one_source_and_cold_retry() -> TestResult {
    let (mut f, relay, earlier) = Box::pin(ready()).await?;
    f.world.http.body.lock().map_err(|_| "body")?["choices"][0]["message"]["content"] =
        "Aqui está a resposta à mensagem escolhida.".into();
    f.message("Unrelated old task").await?;
    let selected = source(&mut f, "Chosen source for one answer").await?;
    f.message("Unrelated later task").await?;
    let consumer = host(&f, &relay)?;
    let sync = consumer.discover_source(&selected).await?;
    assert!(sync.caught_up && sync.discovery_complete);
    let [run] = sync.runs.as_slice() else {
        return Err("one source".into());
    };
    let root = run.clone();
    let generation = sync.native.generation;
    let repeated = consumer.discover_source(&selected).await?;
    assert_eq!(repeated.native.generation, generation);
    assert_eq!(repeated.runs[0].run, root.run);
    assert_eq!(consumer.pending_page(None).await?.runs.len(), 2);
    assert!(consumer
        .advance_source(&earlier.run, &selected)
        .await
        .is_err());
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    let mut worker = SelectedReplyWorker::new(selected.clone())?;
    let (events, complete) = Box::pin(worker.tick(&consumer)).await?;
    let observed: Vec<_> = events
        .iter()
        .map(|event| match event {
            crate::ReplyWorkerEvent::Progress(progress) => format!("{progress:?}"),
            crate::ReplyWorkerEvent::Deferred { error, .. }
            | crate::ReplyWorkerEvent::ConversationDeferred { error, .. } => format!("{error:?}"),
        })
        .collect();
    assert!(complete, "{observed:?}");
    assert!(events.into_iter().any(|event| matches!(event,
        crate::ReplyWorkerEvent::Progress(progress) if matches!(*progress, ReplyHostProgress::Output {
            delivery: crate::ReplyReceiptProgress::Accepted { .. }, .. }))));
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    assert_eq!(f.world.balance().await.held_units, 0);
    drop(consumer);
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    let consumer = host(&f, &relay)?;
    let cold = consumer.discover_source(&selected).await?;
    assert_eq!(cold.runs[0].run, root.run);
    let mut worker = SelectedReplyWorker::new(selected)?;
    assert!(Box::pin(worker.tick(&consumer)).await?.1);
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    // Normal discovery still sees both untouched tasks, and reuses this source.
    let normal = consumer.discover(&f.space).await?;
    assert_eq!(normal.runs.len(), 3);
    assert!(normal.runs.iter().any(|run| run.run == root.run));
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    drop(consumer);
    relay.stop().await;
    f.finish().await;
    println!("selected durable source: one exact paid reply, cold reuse, no scan advancement or unrelated dispatch PASS");
    Ok(())
}
async fn refuse_substitution() -> TestResult {
    let (mut f, relay, _) = Box::pin(ready()).await?;
    let selected = source(&mut f, "Source bound before changes").await?;
    let consumer = host(&f, &relay)?;
    let original = consumer.discover_source(&selected).await?.runs.remove(0);
    for field in ["space", "message", "author", "digest"] {
        let mut wrong = selected.clone();
        match field {
            "space" => wrong.space = types::new_id("other")?,
            "message" => wrong.message = types::new_id("other")?,
            "author" => wrong.author = f.agent.id.clone(),
            _ => wrong.text_sha256 = "00".repeat(32),
        }
        assert!(consumer.discover_source(&wrong).await.is_err(), "{field}");
        assert!(
            consumer
                .advance_source(&original.run, &wrong)
                .await
                .is_err(),
            "{field}"
        );
    }
    drop(consumer);
    f.encrypted(EventBody::GrantRevoked {
        grant: "claimed-context-owner".into(),
    })
    .await?;
    let consumer = host(&f, &relay)?;
    assert!(consumer.discover_source(&selected).await.is_err());
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(f.world.balance().await.spent_units, 0);
    drop(consumer);
    relay.stop().await;
    f.finish().await;
    println!("selected source substitutions and revoked grant: no fallback or provider call PASS");
    Ok(())
}

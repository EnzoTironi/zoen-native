//! A selector can reach the host before its source/grant reach the relay.
//! Retry discovery; never infer from a selector without authenticated history.
use super::*;
use crate::{
    ReplyHostConfig, ReplyHostProgress, ReplyReceiptProgress, ReplyRuntimeHost, ReplySource,
    ReplyWorkerEvent, SelectedReplyWorker,
};
use std::time::{Duration, Instant};

pub(in crate::tests) async fn run() -> TestResult {
    let mut f = Box::pin(claimed()).await?;
    f.world.http.body.lock().map_err(|_| "fixture response")?["choices"][0]["message"]["content"] =
        "Aqui está a resposta à mensagem autorizada.".into();
    let source = ReplySource {
        space: f.space.clone(),
        message: types::new_id("late-grant-source")?,
        author: f.owner.id.clone(),
        text_sha256: crate::hash(ORIGINAL.as_bytes()),
    };
    output_route::configured(&f, true).await?;
    let relay = Relay::start(&f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    let host = ReplyRuntimeHost::configured(
        f.world.runtime.clone(),
        ReplyHostConfig {
            agent: f.agent.id.clone(),
            device: f.world.agents[0].device.clone().ok_or("device")?,
            target: relay.target(&f, relay.address),
        },
        gate::Gate::default(),
    )?;
    let mut selected_worker = SelectedReplyWorker::new(source.clone())?;
    let events = Box::pin(selected_worker.tick(&host)).await?.0;
    assert!(events.iter().any(|event| matches!(event,
        ReplyWorkerEvent::ConversationDeferred { space, error: RuntimeError::Denied } if space == &f.space)),
        "initial discovery must refuse an absent authenticated source");
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(f.world.balance().await.spent_units, 0);
    assert_eq!(f.world.balance().await.held_units, 0);
    // Preserve the actual contract: permission precedes the signed message.
    // A later grant must not retroactively authorize an earlier message.
    f.grant("grant-before-late-source").await?;
    f.encrypted_named(
        EventBody::MessagePosted {
            message: types::new_id("business-message")?,
            text: ORIGINAL.into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
            forwarded_author: None,
        },
        &source.message,
    )
    .await?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let events = Box::pin(selected_worker.tick(&host)).await?.0;
        for event in &events {
            match event {
                ReplyWorkerEvent::Progress(progress) => {
                    if matches!(
                        **progress,
                        ReplyHostProgress::Held { .. }
                            | ReplyHostProgress::AwaitingRecovery { .. }
                            | ReplyHostProgress::Review { .. }
                    ) {
                        return Err(format!(
                            "late authorized source unexpectedly held: {progress:?}"
                        )
                        .into());
                    }
                }
                ReplyWorkerEvent::Deferred { error, .. }
                | ReplyWorkerEvent::ConversationDeferred { error, .. } => {
                    eprintln!("late authorized source deferred: {error:?}");
                }
            }
        }
        if events.iter().any(|event| matches!(event, ReplyWorkerEvent::Progress(progress)
            if matches!(**progress, ReplyHostProgress::Output { delivery: ReplyReceiptProgress::Accepted { .. }, .. }))) {
            break;
        }
        if Instant::now() >= deadline {
            return Err("worker did not rediscover the actual authorized owner source".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    assert_eq!(f.world.balance().await.held_units, 0);
    drop(host);
    relay.stop().await;
    f.finish().await;
    println!("selected worker discovery: absent source refused without paid effect; delayed real grant/message yielded one answer on the same worker PASS");
    Ok(())
}

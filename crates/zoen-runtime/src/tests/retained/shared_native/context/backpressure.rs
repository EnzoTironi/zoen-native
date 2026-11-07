//! Actual capacity pressure keeps both schedulers alive until normal FDB GC.
use super::*;
use crate::{
    ReplyHostConfig, ReplyHostProgress, ReplyRuntimeHost, ReplySource, ReplyWorker,
    ReplyWorkerEvent, SelectedReplyWorker,
};
use std::time::{Duration, Instant};

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
    let mut f = Box::pin(claimed()).await?;
    f.grant("backpressure-owner").await?;
    let source = ReplySource {
        space: f.space.clone(),
        message: types::new_id("pressure-source")?,
        author: f.owner.id.clone(),
        text_sha256: crate::hash(ORIGINAL.as_bytes()),
    };
    f.encrypted_named(
        EventBody::MessagePosted {
            message: types::new_id("pressure-message")?,
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
    let original = f
        .sync(false)
        .await?
        .runs
        .into_iter()
        .next()
        .ok_or("original")?;
    output_route::configured(&f, true).await?;
    let relay = Relay::start(&f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    f.world.http.body.lock().map_err(|_| "fixture response")?["choices"][0]["message"]["content"] =
        "Resposta preservada após liberar capacidade.".into();
    let mut full = false;
    for _ in 0..32 {
        f.clear(EventBody::DeviceJoining {
            device: f.world.owner.device.clone().ok_or("owner device")?,
        })
        .await?;
        match f.sync(false).await {
            Ok(sync) => assert!(sync.runs.is_empty()),
            Err(RuntimeError::NativeBackpressure) => {
                full = true;
                break;
            }
            Err(error) => return Err(error.into()),
        }
    }
    assert!(full, "the actual native retention window must be saturated");
    let consumer = host(&f, &relay)?;
    let mut all = ReplyWorker::default();
    let events = Box::pin(all.tick(&consumer)).await?;
    assert!(events.iter().any(|event| matches!(
        event,
        ReplyWorkerEvent::Deferred {
            error: RuntimeError::NativeBackpressure,
            ..
        } | ReplyWorkerEvent::ConversationDeferred {
            error: RuntimeError::NativeBackpressure,
            ..
        }
    )));
    let mut selected = SelectedReplyWorker::new(source.clone())?;
    let (events, complete) = Box::pin(selected.tick(&consumer)).await?;
    assert!(!complete);
    assert!(events.iter().any(|event| matches!(
        event,
        ReplyWorkerEvent::ConversationDeferred {
            error: RuntimeError::NativeBackpressure,
            ..
        }
    )));
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    // Let the production version clock and normal GC run unchanged. No fixture
    // expiry, stage clearing, lease takeover or parent/service restart is used.
    let until = Instant::now() + Duration::from_secs(90);
    let accepted = loop {
        let (events, complete) = Box::pin(selected.tick(&consumer)).await?;
        if complete {
            break delivered(&events)?;
        }
        if Instant::now() >= until {
            return Err("capacity did not recover through normal GC".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(accepted.0, original.run);
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    assert_eq!(f.world.balance().await.held_units, 0);
    assert_eq!(f.world.runtime.pending_reply_runs(64).await?.len(), 0);
    drop(consumer);
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    let cold = host(&f, &relay)?;
    let mut selected = SelectedReplyWorker::new(source)?;
    let (events, complete) = Box::pin(selected.tick(&cold)).await?;
    assert!(complete);
    assert_eq!(delivered(&events)?, accepted);
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    assert_eq!(f.world.runtime.pending_reply_runs(64).await?.len(), 0);
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_attempts").await,
        1
    );
    drop(cold);
    relay.stop().await;
    f.finish().await;
    println!("native backpressure: both workers defer actual full capacity; same process resumes after normal GC; cold replay preserves one attempt/charge/output PASS");
    Ok(())
}

fn delivered(events: &[ReplyWorkerEvent]) -> Result<(String, String), Box<dyn std::error::Error>> {
    events
        .iter()
        .find_map(|event| {
            let ReplyWorkerEvent::Progress(progress) = event else {
                return None;
            };
            match progress.as_ref() {
                ReplyHostProgress::Output { run, output, .. }
                | ReplyHostProgress::Completed { run, output, .. } => {
                    Some((run.clone(), output.client_id.clone()))
                }
                _ => None,
            }
        })
        .ok_or_else(|| "accepted original output unavailable".into())
}

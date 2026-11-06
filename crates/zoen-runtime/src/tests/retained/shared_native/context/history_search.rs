//! Real data cannot escape after actor, definition or source changes mid-read.
use super::*;
use crate::{ReplyHostConfig, ReplyRuntimeHost};

pub(super) async fn run() -> TestResult {
    for change in ["pause", "definition", "source"] {
        Box::pin(race(change)).await?;
    }
    Ok(())
}
async fn race(change: &str) -> TestResult {
    let (mut f, relay, _) = Box::pin(ready()).await?;
    f.message("Search data needle").await?;
    let run = f
        .sync(false)
        .await?
        .runs
        .into_iter()
        .last()
        .ok_or("search run")?;
    let host = ReplyRuntimeHost::configured(
        f.world.runtime.clone(),
        ReplyHostConfig {
            agent: f.agent.id.clone(),
            device: f.world.agents[0].device.clone().ok_or("device")?,
            target: relay.target(&f, relay.address),
        },
        gate::Gate::default(),
    )?;
    let scope = f.world.runtime.production_output_scope()?;
    let native = f.world.runtime.native.as_ref().ok_or("native custody")?;
    assert!(matches!(
        native
            .reply_host_state(
                &f.world.runtime,
                &run.run,
                scope,
                (
                    &f.owner.id,
                    f.world.agents[0].device.as_deref().ok_or("device")?
                )
            )
            .await,
        Err(RuntimeError::Denied)
    ));
    assert!(matches!(
        native
            .reply_host_state(
                &f.world.runtime,
                &run.run,
                scope,
                (
                    &f.agent.id,
                    f.world.owner.device.as_deref().ok_or("owner device")?
                )
            )
            .await,
        Err(RuntimeError::Denied)
    ));
    let arguments = serde_json::json!({"query":"needle","limit":1});
    assert_eq!(
        host.search_history(&run.run, &arguments).await?.hits.len(),
        1
    );
    f.world.runtime.fault.store(80, Ordering::SeqCst);
    let mutation = async {
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            f.world.runtime.native_cut_entered.notified(),
        )
        .await?;
        let result = match change {
            "pause" => zoen_relay::agents::control(
                &f.world.runtime.finance.pool,
                &f.owner.id,
                &f.agent.id,
                protocol::AgentAction::Pause,
            )
            .await
            .map(|_| ())
            .map_err(Into::into),
            "definition" => define(&f, &definition(2)).await,
            "source" => f
                .message("Source changed while a history page was being read")
                .await
                .map(|_| ()),
            _ => Err("change".into()),
        };
        f.world.runtime.fault.store(0, Ordering::SeqCst);
        f.world.runtime.native_cut_resume.notify_one();
        result
    };
    let (read, changed): (_, TestResult) = tokio::join!(
        Box::pin(host.search_history(&run.run, &arguments)),
        mutation
    );
    changed?;
    assert!(
        matches!(read, Err(RuntimeError::Denied)),
        "changed search must not return data: {read:?}"
    );
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(f.world.balance().await.held_units, 0);
    assert_eq!(f.world.balance().await.spent_units, 0);
    drop(host);
    relay.stop().await;
    f.finish().await;
    println!("native history search {change}: real in-flight authority loss refuses data without model call or charge PASS");
    Ok(())
}

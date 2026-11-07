//! Claimed citizens use canonical definitions and authenticated role history.
use super::*;
use crate::tests::retained::image_tasks::delivery::relay::Relay;
use types::agent_behavior::{AgentKind, Definition, ModelTier};
pub(in crate::tests) mod backpressure;
mod conversation;
mod history_search;
pub(in crate::tests) mod policy_host;
mod search_turn;
pub(in crate::tests) mod selected_source;
pub(in crate::tests) mod tool_reviews;
mod tool_rounds;
pub(in crate::tests) mod worker_discovery;

fn definition(version: u32) -> Definition {
    Definition {
        version,
        kind: AgentKind::User,
        persona: format!("Careful assistant version {version}"),
        rules: vec!["Preserve the author's intent".into()],
        state: serde_json::json!({"language":"pt-BR"}),
        model_tier: ModelTier::Cheap,
    }
}
async fn define(f: &Fixture, value: &Definition) -> TestResult {
    zoen_relay::agents::define(
        &f.world.runtime.finance.pool,
        &f.owner.id,
        f.world.owner.device.as_deref().ok_or("owner device")?,
        &f.agent.id,
        value.version,
        value.kind.relay_kind(),
        &value.encode()?,
    )
    .await?;
    Ok(())
}
async fn claim_only() -> Result<Fixture, Box<dyn std::error::Error>> {
    let f = Fixture::with_shared_runtime().await?;
    let actor = &f.world.agents[0];
    let mut tx = f.world.runtime.finance.pool.begin().await?;
    zoen_relay::agents::claim_citizen(
        &mut tx,
        &zoen_relay::agents::CitizenClaim {
            agent: actor.identity.clone(),
            device: actor.device.clone().ok_or("device")?,
            avatar_style: "glyph".into(),
            avatar_glyph: Some("sparkles".into()),
            avatar_tint: None,
            avatar_asset: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(f)
}
async fn claimed() -> Result<Fixture, Box<dyn std::error::Error>> {
    let f = Box::pin(claim_only()).await?;
    define(&f, &definition(1)).await?;
    Ok(f)
}
async fn ready() -> Result<(Fixture, Relay, crate::ReplyRun), Box<dyn std::error::Error>> {
    let mut f = Box::pin(claimed()).await?;
    f.grant("claimed-context-owner").await?;
    f.message(ORIGINAL).await?;
    let run = f.sync(false).await?.runs.into_iter().next().ok_or("run")?;
    output_route::configured(&f, true).await?;
    let relay = Relay::start(&f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    Ok((f, relay, run))
}
pub(in crate::tests) async fn history_turns() -> TestResult {
    Box::pin(tool_rounds::run()).await?;
    Box::pin(search_turn::run()).await?;
    Ok(())
}
pub(in crate::tests) async fn run() -> TestResult {
    Box::pin(backpressure::run()).await?;
    Box::pin(selected_source::run()).await?;
    Box::pin(tool_reviews::run()).await?;
    Box::pin(tool_reviews::boundaries()).await?;
    Box::pin(tool_reviews::sequential()).await?;
    Box::pin(tool_rounds::run()).await?;
    Box::pin(search_turn::run()).await?;
    Box::pin(history_search::run()).await?;
    Box::pin(policy_host::run()).await?;
    Box::pin(conversation::run()).await?;
    Box::pin(role_history_and_cold_recovery()).await?;
    for change in ["definition", "pause", "revoke"] {
        Box::pin(refuse_changed_before_call(change)).await?;
    }
    for claimed in [false, true] {
        Box::pin(pause_between_financial_phases(claimed)).await?;
    }
    for change in ["missing", "tier", "kind", "malformed"] {
        Box::pin(refuse_invalid_definition(change)).await?;
    }
    Ok(())
}
async fn role_history_and_cold_recovery() -> TestResult {
    let (mut f, relay, first) = Box::pin(ready()).await?;
    Box::pin(f.world.runtime.run_model(&first.run)).await?;
    f.world.runtime.prepare_reply_output(&first.run).await?;
    assert!(matches!(
        f.world
            .runtime
            .deliver_reply_output(&first.run, relay.target(&f, relay.address))
            .await?,
        crate::ReplyReceiptProgress::Accepted { .. }
    ));
    // Use the actual accepted agent echo as the next assistant history entry.
    f.head = f
        .log
        .read(&f.space, 1, 256)
        .await?
        .last()
        .map(|event| Seen {
            seq: event.seq,
            hash: event.hash.clone(),
        });
    f.message("Continue the same conversation").await?;
    let next = f
        .sync(false)
        .await?
        .runs
        .into_iter()
        .next()
        .ok_or("next run")?;
    let result = Box::pin(f.world.runtime.run_model(&next.run)).await?;
    let requests = f
        .world
        .http
        .requests
        .lock()
        .map_err(|_| "requests")?
        .clone();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["messages"][0]["role"], "system");
    assert_eq!(
        text(&requests[0]["messages"][0])?,
        definition(1).system_prompt()
    );
    let history = requests[1]["messages"].as_array().ok_or("messages")?;
    assert_eq!(history.len(), 4);
    assert_eq!(history[1]["role"], "user");
    assert_eq!(text(&history[1])?, ORIGINAL);
    assert_eq!(history[2]["role"], "assistant");
    assert_eq!(text(&history[2])?, crate::tests::BODY);
    assert_eq!(history[3]["role"], "user");
    assert_eq!(text(&history[3])?, "Continue the same conversation");
    // Editing after a completed call must not discard an incurred result.
    define(&f, &definition(2)).await?;
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    assert_eq!(
        Box::pin(f.world.runtime.run_model(&next.run))
            .await?
            .financial,
        result.financial
    );
    f.world.runtime.prepare_reply_output(&next.run).await?;
    assert!(matches!(
        f.world
            .runtime
            .deliver_reply_output(&next.run, relay.target(&f, relay.address))
            .await?,
        crate::ReplyReceiptProgress::Accepted { .. }
    ));
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 2);
    assert_eq!(f.world.balance().await.spent_units, 12);
    relay.stop().await;
    f.finish().await;
    println!("canonical agent context: actual claim/definition, system+authenticated user/assistant roles at HTTP ingress, cold incurred result after edit, one charge per call PASS");
    Ok(())
}
async fn refuse_changed_before_call(change: &str) -> TestResult {
    let (f, relay, run) = Box::pin(ready()).await?;
    match change {
        "definition" => define(&f, &definition(2)).await?,
        "pause" | "revoke" => {
            zoen_relay::agents::control(
                &f.world.runtime.finance.pool,
                &f.owner.id,
                &f.agent.id,
                if change == "pause" {
                    protocol::AgentAction::Pause
                } else {
                    protocol::AgentAction::Revoke
                },
            )
            .await?;
        }
        _ => return Err("change".into()),
    }
    assert!(matches!(
        Box::pin(f.world.runtime.run_model(&run.run)).await,
        Err(RuntimeError::Denied)
    ));
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(f.world.balance().await.held_units, 0);
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_claims").await,
        0
    );
    relay.stop().await;
    f.finish().await;
    println!("canonical agent context {change}: changed current authority refuses new hold/claim/provider invocation PASS");
    Ok(())
}
async fn refuse_invalid_definition(change: &str) -> TestResult {
    let mut f = Box::pin(claimed()).await?;
    let mut value = definition(2);
    match change {
        "missing" => {
            zoen_relay::agents::define(
                &f.world.runtime.finance.pool,
                &f.owner.id,
                f.world.owner.device.as_deref().ok_or("owner device")?,
                &f.agent.id,
                2,
                "User",
                b"",
            )
            .await?;
        }
        "tier" => {
            value.model_tier = ModelTier::Premium;
            define(&f, &value).await?;
        }
        "kind" => {
            zoen_relay::agents::define(
                &f.world.runtime.finance.pool,
                &f.owner.id,
                f.world.owner.device.as_deref().ok_or("owner device")?,
                &f.agent.id,
                2,
                "Creator",
                &value.encode()?,
            )
            .await?;
        }
        "malformed" => {
            zoen_relay::agents::define(
                &f.world.runtime.finance.pool,
                &f.owner.id,
                f.world.owner.device.as_deref().ok_or("owner device")?,
                &f.agent.id,
                2,
                "User",
                b"not-json",
            )
            .await?;
        }
        _ => return Err("change".into()),
    }
    f.grant("invalid-context-owner").await?;
    f.message(ORIGINAL).await?;
    assert!(f.sync(false).await.is_err());
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(f.world.balance().await.held_units, 0);
    f.finish().await;
    println!("canonical agent context {change}: malformed/missing/unsupported definition never falls back to bare text/default persona PASS");
    Ok(())
}

fn text(message: &serde_json::Value) -> Result<&str, Box<dyn std::error::Error>> {
    let content = &message["content"];
    if let Some(text) = content.as_str() {
        return Ok(text);
    }
    let blocks = content.as_array().ok_or("text content")?;
    let [block] = blocks.as_slice() else {
        return Err("one text block".into());
    };
    if block["type"] != "text" {
        return Err("text block type".into());
    }
    block["text"]
        .as_str()
        .ok_or_else(|| "text block body".into())
}

async fn pause_between_financial_phases(claimed: bool) -> TestResult {
    let (f, relay, run) = Box::pin(ready()).await?;
    let binding = native::testing::reserve_retained_reply(&f.world.runtime, &run.run).await?;
    let claim = if claimed {
        Some(
            native::testing::claim_retained_reply(&f.world.runtime, &run.run)
                .await?
                .ok_or("fresh claim")?,
        )
    } else {
        None
    };
    zoen_relay::agents::control(
        &f.world.runtime.finance.pool,
        &f.owner.id,
        &f.agent.id,
        protocol::AgentAction::Pause,
    )
    .await?;
    if let Some(claim) = claim {
        assert!(matches!(
            native::testing::dispatch_retained_reply(&f.world.runtime, &run.run, claim).await,
            Err(RuntimeError::Denied)
        ));
    } else {
        assert!(matches!(
            native::testing::claim_retained_reply(&f.world.runtime, &run.run).await,
            Err(RuntimeError::Denied)
        ));
    }
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(f.world.balance().await.held_units, binding.hold_units);
    assert_eq!(f.world.balance().await.spent_units, 0);
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_claims").await,
        i64::from(claimed)
    );
    assert_eq!(
        f.world
            .count("SELECT count(*) FROM runtime_admission_witnesses")
            .await,
        0
    );
    relay.stop().await;
    f.finish().await;
    println!("canonical context pause claimed={claimed}: next claim/witness denied, original hold retained, no invented dispatch/usage/refund PASS");
    Ok(())
}

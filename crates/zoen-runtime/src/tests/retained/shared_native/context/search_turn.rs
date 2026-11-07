//! Two real paid calls, one native read and a durable, nonduplicated answer.
use super::*;
use crate::{ReplyHostConfig, ReplyHostProgress, ReplyRuntimeHost};

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
fn proposal(f: &Fixture) -> TestResult {
    let mut body = f.world.http.body.lock().map_err(|_| "body")?;
    body["choices"][0]["message"] = serde_json::json!({"role":"assistant","content":null,
        "tool_calls":[{"id":"history-call-1","type":"function","function":{
            "name":client_core::agent_conversation_tools::TOOL,"arguments":"{\"query\":\"needle\",\"limit\":1}"}}]});
    body["choices"][0]["finish_reason"] = "tool_calls".into();
    Ok(())
}
async fn freeze(f: &Fixture, run: &str) -> Result<String, Box<dyn std::error::Error>> {
    f.world
        .runtime
        .native
        .as_ref()
        .ok_or("native")?
        .continue_search_reply(
            &f.world.runtime,
            run,
            f.world.runtime.production_output_scope()?,
        )
        .await?
        .ok_or_else(|| "expected frozen continuation".into())
}
pub(super) async fn run() -> TestResult {
    for cut in [0, 91, 92] {
        Box::pin(recover(cut)).await?;
    }
    for change in ["pause", "definition", "source", "grant", "cancel"] {
        Box::pin(refuse_changed(change)).await?;
    }
    Ok(())
}
async fn setup() -> Result<(Fixture, Relay, crate::ReplyRun), Box<dyn std::error::Error>> {
    let (mut f, relay, _) = Box::pin(ready()).await?;
    f.message("needle: the original source remains attributable")
        .await?;
    let run = f.sync(false).await?.runs.into_iter().last().ok_or("run")?;
    proposal(&f)?;
    Box::pin(f.world.runtime.run_model(&run.run)).await?;
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    Ok((f, relay, run))
}
async fn recover(cut: u8) -> TestResult {
    let (mut f, relay, run) = Box::pin(setup()).await?;
    // A process dies after the first capture, before search preparation.
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    if cut != 0 {
        f.world.runtime.fault.store(cut, Ordering::SeqCst);
        assert!(matches!(
            f.world
                .runtime
                .native
                .as_ref()
                .ok_or("native")?
                .continue_search_reply(
                    &f.world.runtime,
                    &run.run,
                    f.world.runtime.production_output_scope()?,
                )
                .await,
            Err(RuntimeError::Unavailable)
        ));
        f.world.runtime.fault.store(0, Ordering::SeqCst);
        reopen(&mut f).await?;
        f = Box::pin(execution::qualify(f, &relay)).await?;
    }
    let next = Box::pin(freeze(&f, &run.run)).await?;
    assert_ne!(next, run.run);
    assert_eq!(Box::pin(freeze(&f, &run.run)).await?, next);
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    let consumer = host(&f, &relay)?;
    let pending = consumer.pending_page(None).await?;
    assert!(!pending.runs.iter().any(|item| item.run == run.run));
    assert!(pending.runs.iter().any(|item| item.run == next));
    drop(consumer);
    // Another process dies after the exact tool result is durably frozen.
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    let mut answer = crate::tests::http::reply();
    answer["choices"][0]["message"]["content"] =
        "Encontrei a mensagem anterior na conversa.".into();
    *f.world.http.body.lock().map_err(|_| "body")? = answer;
    let consumer = host(&f, &relay)?;
    println!("native search turn cut={cut}: advancing retained child");
    let client_id = match Box::pin(consumer.advance(&run.run)).await? {
        ReplyHostProgress::Output {
            run: actual,
            output,
            delivery: crate::ReplyReceiptProgress::Accepted { .. },
            ..
        } => {
            assert_eq!(actual, next);
            output.client_id
        }
        other => return Err(format!("expected accepted search answer: {other:?}").into()),
    };
    {
        let requests = f.world.http.requests.lock().map_err(|_| "requests")?;
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0]["tools"][0]["function"]["name"],
            client_core::agent_conversation_tools::TOOL
        );
        let messages = requests[1]["messages"].as_array().ok_or("messages")?;
        let tool = messages.last().ok_or("tool result")?;
        assert_eq!(tool["role"], "tool");
        assert_eq!(tool["tool_call_id"], "history-call-1");
        let page: client_core::MessageSearchPage =
            serde_json::from_str(tool["content"].as_str().ok_or("result")?)?;
        assert_eq!(page.hits.len(), 1);
        assert_eq!(page.hits[0].space_id.as_deref(), Some(f.space.as_str()));
        assert!(page.hits[0].snippet.contains("needle"));
        assert_ne!(requests[1]["tools"], serde_json::Value::Null);
    }
    assert_eq!(f.world.balance().await.spent_units, 12);
    println!("native search turn cut={cut}: accepted, checking receipt and collection proof");
    assert!(native::testing::output::reply_completed(&f.world.runtime, &run.run).await?);
    drop(consumer);
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    let consumer = host(&f, &relay)?;
    assert!(matches!(
        Box::pin(consumer.advance(&run.run)).await?,
        ReplyHostProgress::Completed { .. }
    ));
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 2);
    assert_eq!(f.world.balance().await.spent_units, 12);
    assert_eq!(
        f.log
            .read(&f.space, 1, 256)
            .await?
            .iter()
            .filter(|event| event.env.client_id() == client_id)
            .count(),
        1
    );
    assert!(!consumer
        .pending_page(None)
        .await?
        .runs
        .iter()
        .any(|item| item.run == run.run || item.run == next));
    drop(consumer);
    relay.stop().await;
    f.finish().await;
    println!("native search turn: real tool proposal, attributed source page, two independently settled calls, two cold boundaries, one accepted answer, no duplicate charge PASS");
    Ok(())
}
async fn refuse_changed(change: &str) -> TestResult {
    let (mut f, relay, run) = Box::pin(setup()).await?;
    let next = Box::pin(freeze(&f, &run.run)).await?;
    match change {
        "pause" => {
            zoen_relay::agents::control(
                &f.world.runtime.finance.pool,
                &f.owner.id,
                &f.agent.id,
                protocol::AgentAction::Pause,
            )
            .await?;
        }
        "definition" => define(&f, &definition(2)).await?,
        "cancel" => {
            f.world.runtime.cancel_model(&run.run).await?;
        }
        "grant" => {
            f.grant("replacement-search-authority").await?;
            f.sync(false).await?;
        }
        "source" => {
            f.message("needle changed after the search result was frozen")
                .await?;
            f.sync(false).await?;
        }
        _ => return Err("change".into()),
    }
    assert!(Box::pin(f.world.runtime.run_model(&next)).await.is_err());
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    relay.stop().await;
    f.finish().await;
    println!("native frozen search {change}: current authority loss prevents a second provider call PASS");
    Ok(())
}

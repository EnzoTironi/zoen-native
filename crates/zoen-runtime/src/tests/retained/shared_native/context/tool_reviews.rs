//! Real paid capture, shared journal, MLS review, cold owner decision and resume.
use super::*;
use crate::{ReplyHostConfig, ReplyHostProgress, ReplyRuntimeHost};
use gate::{Answer, DecisionProvider, DecisionRequest, DecisionResponse, GateError};
use std::sync::atomic::AtomicUsize;

struct Judge {
    ask: bool,
    calls: Arc<AtomicUsize>,
}
impl DecisionProvider for Judge {
    fn name(&self) -> String {
        format!("captured-tool-review-{}", self.ask)
    }
    fn on_device(&self) -> bool {
        true
    }
    fn decide(&self, request: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        let mut result = gate::Mock.decide(request)?;
        if request
            .questions
            .iter()
            .any(|question| question.name() == "severity")
        {
            self.calls.fetch_add(1, Ordering::SeqCst);
            result
                .answers
                .insert("matches_user_intent".into(), Answer::Predicate { p: 0.99 });
            result.answers.insert(
                "severity".into(),
                Answer::Score {
                    score: 0.1,
                    confidence: if self.ask { 0.1 } else { 0.99 },
                },
            );
        }
        Ok(result)
    }
}
pub(super) fn policy(ask: bool, calls: Arc<AtomicUsize>) -> gate::Gate {
    gate::Gate {
        on_device: Some(Box::new(Judge { ask, calls })),
        ..gate::Gate::default()
    }
}
fn host(
    f: &Fixture,
    relay: &Relay,
    calls: Arc<AtomicUsize>,
) -> Result<ReplyRuntimeHost, RuntimeError> {
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
        policy(true, calls),
    )
}
fn response(input: &serde_json::Value) -> serde_json::Value {
    let mut body = crate::tests::http::reply();
    if input["messages"]
        .as_array()
        .is_some_and(|messages| messages.iter().any(|message| message["role"] == "tool"))
    {
        body["choices"][0]["message"]["content"] = "Encontrei a decisão anterior.".into();
    } else {
        body["choices"][0]["message"] = serde_json::json!({"role":"assistant","content":null,
            "tool_calls":[{"id":"review-history","type":"function","function":{
                "name":client_core::agent_conversation_tools::TOOL,"arguments":"{\"query\":\"needle\",\"limit\":1}"}}]});
        body["choices"][0]["finish_reason"] = "tool_calls".into();
    }
    body
}
async fn decide(f: &mut Fixture, request: &str, hash: &str, approved: bool) -> TestResult {
    f.head = f
        .log
        .read(&f.space, 1, 256)
        .await?
        .last()
        .map(|event| Seen {
            seq: event.seq,
            hash: event.hash.clone(),
        });
    // Actual owner-signed MLS application, not a database status mutation.
    f.encrypted(EventBody::RequestResolved {
        request: request.into(),
        approved,
        content_hash: hash.into(),
        resolution: Some(RequestResolution {
            message: "Owner decision".into(),
            created_item: None,
            ai_cost_cents: 0,
            standing_grant: None,
            tool_decision: Some(if approved {
                ToolDecision::ApproveOnce
            } else {
                ToolDecision::Deny
            }),
        }),
    })
    .await?;
    Ok(())
}
pub(in crate::tests) async fn run() -> TestResult {
    for (approved, cut) in [(true, 0), (false, 0), (true, 181), (true, 182), (true, 183)] {
        Box::pin(cycle(approved, cut)).await?;
    }
    Ok(())
}
async fn cycle(approved: bool, cut: u8) -> TestResult {
    let (mut f, relay, _) = Box::pin(ready()).await?;
    f.message("needle: retained decision from our history")
        .await?;
    let run = f.sync(false).await?.runs.into_iter().last().ok_or("run")?;
    *f.world.http.responder.lock().map_err(|_| "responder")? = Some(Arc::new(response));
    let calls = Arc::new(AtomicUsize::new(0));
    let consumer = host(&f, &relay, calls.clone())?;
    if cut != 0 {
        f.world.runtime.fault.store(cut, Ordering::SeqCst);
        assert!(matches!(
            Box::pin(consumer.advance(&run.run)).await,
            Err(RuntimeError::Unavailable)
        ));
        f.world.runtime.fault.store(0, Ordering::SeqCst);
        drop(consumer);
        reopen(&mut f).await?;
        f = Box::pin(execution::qualify(f, &relay)).await?;
    } else {
        drop(consumer);
    }
    let consumer = host(&f, &relay, calls.clone())?;
    let progress = Box::pin(consumer.advance(&run.run)).await?;
    let ReplyHostProgress::Review {
        request,
        content_hash,
        status: client_core::RequestStatus::Pending,
        ..
    } = progress
    else {
        return Err(format!("expected native review: {progress:?}").into());
    };
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    assert_eq!(f.world.balance().await.held_units, 0);
    let judged = calls.load(Ordering::SeqCst);
    let retry = Box::pin(consumer.advance(&run.run)).await?;
    assert!(
        matches!(retry, ReplyHostProgress::Review { request: ref other, .. } if *other == request)
    );
    assert_eq!(calls.load(Ordering::SeqCst), judged);
    drop(consumer);
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    decide(&mut f, &request, &content_hash, approved).await?;
    let consumer = host(&f, &relay, calls.clone())?;
    let progress = Box::pin(consumer.advance(&run.run)).await?;
    if approved {
        assert!(
            matches!(
                progress,
                ReplyHostProgress::Output {
                    delivery: crate::ReplyReceiptProgress::Accepted { .. },
                    ..
                }
            ),
            "{progress:?}"
        );
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 2);
        assert_eq!(f.world.balance().await.spent_units, 12);
        assert!(matches!(
            Box::pin(consumer.advance(&run.run)).await?,
            ReplyHostProgress::Completed { .. }
        ));
    } else {
        assert!(
            matches!(
                progress,
                ReplyHostProgress::Review {
                    status: client_core::RequestStatus::Denied,
                    ..
                }
            ),
            "{progress:?}"
        );
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
        assert_eq!(f.world.balance().await.spent_units, 6);
    }
    assert_eq!(calls.load(Ordering::SeqCst), judged);
    assert_eq!(f.world.balance().await.held_units, 0);
    drop(consumer);
    relay.stop().await;
    f.finish().await;
    println!("journaled tool review approved={approved} cut={cut}: cold exact owner decision, paid capture reuse and bounded resume PASS");
    Ok(())
}

pub(in crate::tests) async fn boundaries() -> TestResult {
    for change in ["cancel", "revoke", "policy", "missing"] {
        Box::pin(refuse_change(change)).await?;
    }
    Box::pin(reviewed_chain(false)).await?;
    Ok(())
}
async fn refuse_change(change: &str) -> TestResult {
    let (mut f, relay, run) = Box::pin(ready()).await?;
    *f.world.http.responder.lock().map_err(|_| "responder")? = Some(Arc::new(response));
    let calls = Arc::new(AtomicUsize::new(0));
    let consumer = host(&f, &relay, calls.clone())?;
    let ReplyHostProgress::Review {
        request,
        content_hash,
        ..
    } = Box::pin(consumer.advance(&run.run)).await?
    else {
        return Err("expected pending review".into());
    };
    drop(consumer);
    decide(&mut f, &request, &content_hash, true).await?;
    match change {
        "cancel" => {
            f.world.runtime.cancel_model(&run.run).await?;
        }
        "revoke" => {
            f.encrypted(EventBody::GrantRevoked {
                grant: "claimed-context-owner".into(),
            })
            .await?;
        }
        "missing" => {
            let tx = f.world.runtime.execution.transaction().await?;
            tx.clear(
                &f.world
                    .runtime
                    .execution
                    .run(&run.run, "native-tool-decision/1/0"),
            );
            tx.commit().await?;
        }
        "policy" => {}
        _ => return Err("unknown test change".into()),
    }
    let consumer = if change == "policy" {
        ReplyRuntimeHost::configured(
            f.world.runtime.clone(),
            ReplyHostConfig {
                agent: f.agent.id.clone(),
                device: f.world.agents[0].device.clone().ok_or("device")?,
                target: relay.target(&f, relay.address),
            },
            policy(false, calls.clone()),
        )?
    } else {
        host(&f, &relay, calls.clone())?
    };
    assert!(
        Box::pin(consumer.advance(&run.run)).await.is_err(),
        "{change}"
    );
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(f.world.balance().await.spent_units, 6);
    assert_eq!(f.world.balance().await.held_units, 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(consumer);
    relay.stop().await;
    f.finish().await;
    println!("journaled tool review {change}: approved data cannot bypass current authority or lost continuity PASS");
    Ok(())
}
pub(in crate::tests) async fn sequential() -> TestResult {
    Box::pin(reviewed_chain(true)).await
}
async fn reviewed_chain(sequential: bool) -> TestResult {
    let (mut f, relay, run) = Box::pin(ready()).await?;
    *f.world.http.responder.lock().map_err(|_| "responder")? = Some(Arc::new(move |input| {
        let mut body = response(input);
        let done = input["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|message| message["role"] == "tool")
            .count();
        if sequential && done < 3 {
            body["choices"][0]["message"] = serde_json::json!({"role":"assistant","content":null,
                "tool_calls":[{"id":format!("review-{done}"),"type":"function","function":{
                    "name":client_core::agent_conversation_tools::TOOL,
                    "arguments":serde_json::json!({"query":format!("query-{done}")}).to_string()}}]});
            body["choices"][0]["finish_reason"] = "tool_calls".into();
        }
        if !sequential && body["choices"][0]["finish_reason"] == "tool_calls" {
            body["choices"][0]["message"]["tool_calls"] = serde_json::json!([
                {"id":"review-a","type":"function","function":{"name":client_core::agent_conversation_tools::TOOL,"arguments":"{\"query\":\"alpha\"}"}},
                {"id":"review-b","type":"function","function":{"name":client_core::agent_conversation_tools::TOOL,"arguments":"{\"query\":\"beta\"}"}},
                {"id":"review-c","type":"function","function":{"name":client_core::agent_conversation_tools::TOOL,"arguments":"{\"query\":\"gamma\"}"}}
            ]);
        }
        body
    }));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut ids = std::collections::BTreeSet::new();
    for count in 1..=3 {
        let consumer = host(&f, &relay, calls.clone())?;
        let progress = Box::pin(consumer.advance(&run.run)).await?;
        let ReplyHostProgress::Review {
            request,
            content_hash,
            status: client_core::RequestStatus::Pending,
            ..
        } = progress
        else {
            return Err(format!("batch review {count}: {progress:?}").into());
        };
        assert!(ids.insert(request.clone()));
        assert_eq!(calls.load(Ordering::SeqCst), count);
        assert_eq!(
            f.world.http.sends.load(Ordering::SeqCst),
            if sequential { count } else { 1 }
        );
        drop(consumer);
        // Every call requires its own signed decision, including after restart.
        reopen(&mut f).await?;
        f = Box::pin(execution::qualify(f, &relay)).await?;
        decide(&mut f, &request, &content_hash, true).await?;
    }
    let consumer = host(&f, &relay, calls.clone())?;
    let result = Box::pin(consumer.advance(&run.run)).await?;
    assert!(
        matches!(
            result,
            ReplyHostProgress::Output {
                delivery: crate::ReplyReceiptProgress::Accepted { .. },
                ..
            }
        ),
        "{result:?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(
        f.world.http.sends.load(Ordering::SeqCst),
        if sequential { 4 } else { 2 }
    );
    assert_eq!(
        f.world.balance().await.spent_units,
        if sequential { 24 } else { 12 }
    );
    assert_eq!(f.world.balance().await.held_units, 0);
    let tools = {
        let requests = f.world.http.requests.lock().map_err(|_| "requests")?;
        requests.last().ok_or("final request")?["messages"]
            .as_array()
            .ok_or("messages")?
            .iter()
            .filter(|message| message["role"] == "tool")
            .count()
    };
    assert_eq!(tools, 3);
    drop(consumer);
    relay.stop().await;
    f.finish().await;
    println!("journaled reviews sequential={sequential}: three independent cold owner approvals, exact captures and one final answer PASS");
    Ok(())
}

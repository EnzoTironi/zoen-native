//! Genuine HTTP tool turns read older pages and survive a concurrent new message.
use super::*;
use crate::{ReplyHostConfig, ReplyHostProgress, ReplyRuntimeHost};
use serde_json::{json, Value};

fn response(input: &Value, overrun: bool) -> Value {
    let mut body = crate::tests::http::reply();
    let tools: Vec<_> = input["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|message| message["role"] == "tool")
        .collect();
    if tools.len() < 3 || overrun {
        let mut arguments = json!({"query":"needle","limit":2});
        if let Some(last) = tools.last() {
            let page: Value = serde_json::from_str(last["content"].as_str().unwrap_or("null"))
                .unwrap_or(Value::Null);
            arguments["cursor"] = page["next_cursor"].clone();
        }
        body["choices"][0]["message"] = json!({"role":"assistant","content":null,"tool_calls":[{
            "id":format!("history-round-{}", tools.len()), "type":"function", "function":{
                "name":client_core::agent_conversation_tools::TOOL, "arguments":arguments.to_string()
            }
        }]});
        body["choices"][0]["finish_reason"] = "tool_calls".into();
    } else {
        body["choices"][0]["message"]["content"] =
            "Encontrei as mensagens anteriores na conversa.".into();
    }
    body
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
        .ok_or_else(|| "expected continuation".into())
}
pub(super) async fn run() -> TestResult {
    for overrun in [false, true] {
        Box::pin(paged_answer(overrun)).await?;
    }
    Box::pin(cancel_descendant()).await?;
    Box::pin(batched_queries_and_segmented_text()).await?;
    Ok(())
}
async fn setup() -> Result<(Fixture, Relay, crate::ReplyRun), Box<dyn std::error::Error>> {
    let (mut f, relay, _) = Box::pin(ready()).await?;
    for number in 0..7 {
        f.message(&format!("needle historical detail {number}"))
            .await?;
    }
    for number in 0..18 {
        f.message(&format!("Recent unrelated discussion {number}"))
            .await?;
    }
    f.message("Find our older details").await?;
    let mut runs = Vec::new();
    for _ in 0..16 {
        let page = f.sync(false).await?;
        runs.extend(page.runs);
        if page.caught_up && page.discovery_complete {
            break;
        }
    }
    let run = runs.pop().ok_or("run")?;
    Ok((f, relay, run))
}
async fn paged_answer(overrun: bool) -> TestResult {
    let (mut f, relay, root) = Box::pin(setup()).await?;
    *f.world.http.responder.lock().map_err(|_| "responder")? =
        Some(Arc::new(move |input| response(input, overrun)));
    Box::pin(f.world.runtime.run_model(&root.run)).await?;
    let child = Box::pin(freeze(&f, &root.run)).await?;
    Box::pin(f.world.runtime.run_model(&child)).await?;
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 2);
    // A later owner message does not force resubmission of an unaffected search.
    f.message("A new unrelated message while the model is working")
        .await?;
    f.sync(false).await?;
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    let consumer = host(&f, &relay)?;
    let progress = Box::pin(consumer.advance(&root.run)).await?;
    let client_id = if overrun {
        assert!(matches!(
            progress,
            ReplyHostProgress::AwaitingRecovery { .. }
        ));
        None
    } else {
        match progress {
            ReplyHostProgress::Output {
                output,
                delivery: crate::ReplyReceiptProgress::Accepted { .. },
                ..
            } => Some(output.client_id),
            other => return Err(format!("expected paged answer: {other:?}").into()),
        }
    };
    {
        let requests = f.world.http.requests.lock().map_err(|_| "requests")?;
        assert_eq!(requests.len(), 4);
        let mut ids = std::collections::BTreeSet::new();
        for (round, request) in requests.iter().enumerate() {
            let messages = request["messages"].as_array().ok_or("messages")?;
            let tools: Vec<_> = messages
                .iter()
                .filter(|message| message["role"] == "tool")
                .collect();
            assert_eq!(tools.len(), round);
            if round == 0 {
                assert!(!serde_json::to_string(messages)?.contains("needle historical detail"));
            } else {
                let page: client_core::MessageSearchPage = serde_json::from_str(
                    tools.last().ok_or("tool")?["content"]
                        .as_str()
                        .ok_or("content")?,
                )?;
                assert_eq!(page.hits.len(), 2);
                for hit in page.hits {
                    assert!(ids.insert(hit.ref_id));
                }
            }
        }
        assert_eq!(ids.len(), 6);
        assert_eq!(requests[3]["tool_choice"], "none");
    }
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 4);
    assert_eq!(f.world.balance().await.spent_units, 24);
    assert_eq!(f.world.balance().await.held_units, 0);
    drop(consumer);
    reopen(&mut f).await?;
    f = Box::pin(execution::qualify(f, &relay)).await?;
    let consumer = host(&f, &relay)?;
    let retry = Box::pin(consumer.advance(&root.run)).await?;
    if overrun {
        assert!(matches!(retry, ReplyHostProgress::AwaitingRecovery { .. }));
    } else {
        assert!(matches!(retry, ReplyHostProgress::Completed { .. }));
        let client_id = client_id.ok_or("published reply")?;
        assert_eq!(
            f.log
                .read(&f.space, 1, 256)
                .await?
                .iter()
                .filter(|event| event.env.client_id() == client_id)
                .count(),
            1
        );
        assert!(native::testing::output::reply_completed(&f.world.runtime, &root.run).await?);
    }
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 4);
    assert_eq!(f.world.balance().await.spent_units, 24);
    drop(consumer);
    relay.stop().await;
    f.finish().await;
    println!("native paged tools overrun={overrun}: older-than-context pages, concurrent message, cold continuation, four bounded paid originals, no duplicate cost/output PASS");
    Ok(())
}
async fn cancel_descendant() -> TestResult {
    let (f, relay, root) = Box::pin(setup()).await?;
    *f.world.http.responder.lock().map_err(|_| "responder")? =
        Some(Arc::new(|input| response(input, false)));
    Box::pin(f.world.runtime.run_model(&root.run)).await?;
    let first = Box::pin(freeze(&f, &root.run)).await?;
    Box::pin(f.world.runtime.run_model(&first)).await?;
    let next = Box::pin(freeze(&f, &first)).await?;
    f.world.runtime.cancel_model(&root.run).await?;
    assert!(Box::pin(f.world.runtime.run_model(&next)).await.is_err());
    let consumer = host(&f, &relay)?;
    let mut after = None;
    let mut pending = Vec::new();
    loop {
        let page = consumer.pending_page(after.as_deref()).await?;
        pending.extend(page.runs);
        match page.next {
            Some(cursor) => after = Some(cursor),
            None => break,
        }
    }
    assert!(!pending
        .iter()
        .any(|item| item.run == next || item.run == first || item.run == root.run));
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 2);
    assert_eq!(f.world.balance().await.spent_units, 12);
    drop(consumer);
    relay.stop().await;
    f.finish().await;
    println!("native cancellation: original stop blocks later descendants and removes runnable work without refunding incurred calls PASS");
    Ok(())
}

async fn batched_queries_and_segmented_text() -> TestResult {
    let (f, relay, root) = Box::pin(setup()).await?;
    *f.world.http.responder.lock().map_err(|_| "responder")? = Some(Arc::new(|input| {
        let mut body = crate::tests::http::reply();
        let count = input["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|message| message["role"] == "tool")
            .count();
        if count == 0 {
            body["choices"][0]["message"] = json!({"role":"assistant","content":"Vou consultar as duas partes.","tool_calls":[
                {"id":"batch-old","type":"function","function":{"name":client_core::agent_conversation_tools::TOOL,"arguments":"{\"query\":\"needle\",\"limit\":2}"}},
                {"id":"batch-new","type":"function","function":{"name":client_core::agent_conversation_tools::TOOL,"arguments":"{\"query\":\"Recent unrelated\",\"limit\":2}"}}
            ]});
            body["choices"][0]["finish_reason"] = "tool_calls".into();
        } else {
            body["choices"][0]["message"]["content"] = json!([
                {"type":"text","text":"As mensagens "}, {"type":"text","text":"estão prontas."}
            ]);
        }
        body
    }));
    let consumer = host(&f, &relay)?;
    let leaf = match Box::pin(consumer.advance(&root.run)).await? {
        ReplyHostProgress::Output {
            run,
            delivery: crate::ReplyReceiptProgress::Accepted { .. },
            ..
        } => run,
        other => return Err(format!("expected batched answer: {other:?}").into()),
    };
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 2);
    assert_eq!(f.world.balance().await.spent_units, 12);
    assert_eq!(
        native::testing::output::retained_result(&f.world.runtime, &leaf)
            .await?
            .1,
        "As mensagens estão prontas."
    );
    {
        let requests = f.world.http.requests.lock().map_err(|_| "requests")?;
        let messages = requests[1]["messages"].as_array().ok_or("messages")?;
        let tools: Vec<_> = messages
            .iter()
            .filter(|message| message["role"] == "tool")
            .collect();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["tool_call_id"], "batch-old");
        assert_eq!(tools[1]["tool_call_id"], "batch-new");
        for tool in tools {
            let page: client_core::MessageSearchPage =
                serde_json::from_str(tool["content"].as_str().ok_or("content")?)?;
            assert_eq!(page.hits.len(), 2);
            assert!(page
                .hits
                .iter()
                .all(|hit| hit.space_id.as_deref() == Some(f.space.as_str())));
        }
    }
    drop(consumer);
    relay.stop().await;
    f.finish().await;
    println!("native batched queries: mixed text/tool proposals, two individually authorized reads, segmented final text, two paid calls and one accepted reply PASS");
    Ok(())
}

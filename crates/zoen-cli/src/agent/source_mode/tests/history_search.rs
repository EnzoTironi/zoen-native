//! Existing certified source/ToolHost/runtime/journal; no service or real model.
use super::*;
use crate::agent::{history_search as history, source_recovery::SourceTools};
use client_core::MessageSearchPage;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn search_call() -> ToolCall {
    ToolCall {
        id: "history-read-1".into(),
        name: history::TOOL.into(),
        arguments: json!({"query":"Other source", "limit":1}),
        requires_review: false,
    }
}

fn search_context(f: &Fixture, calls: &Arc<AtomicUsize>) -> Ctx {
    let mut cx = context(f, calls, true);
    let calls = calls.clone();
    let expected = f.other.clone();
    cx.runtime = Runtime::new(
        Gate {
            on_device: Some(Box::new(crate::agent::page_tool_tests::Allow)),
            ..Gate::default()
        },
        Box::new(ControlledToolBrain {
            evaluate: Box::new(move |steps| {
                calls.fetch_add(1, Ordering::SeqCst);
                match steps {
                    [] => Ok(BrainOut::CallTools(vec![search_call()])),
                    [read] => {
                        assert!(!read.is_error, "{}", read.result);
                        let page: MessageSearchPage = serde_json::from_str(&read.result).unwrap();
                        assert_eq!(page.hits.len(), 1);
                        assert_eq!(page.hits[0].ref_id, expected);
                        Ok(BrainOut::Say(format!(
                            "Found message {}",
                            page.hits[0].ref_id
                        )))
                    }
                    _ => Err("Unexpected extra tool execution".into()),
                }
            }),
        }),
    );
    cx
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn normal_source_turn_searches_older_than_context_and_reconciles_one_linked_reply(
) -> TestResult {
    let mut f = Fixture::new("http://127.0.0.1:1")?;
    for n in 0..25 {
        f.append(message(&format!("Recent filler {n}"), None)?, true)?;
    }
    f.source = f.append(
        message("Find what we said in the older source", None)?,
        true,
    )?;
    let calls = Arc::new(AtomicUsize::new(0));
    let cx = search_context(&f, &calls);
    let tools = SpaceTools::new(&f.client, &cx, &f.space, &f.owner, &f.source);
    assert!(tools.tools().iter().any(|tool| tool.name == history::TOOL));
    let timeline = f.client.timeline(f.space.clone())?;
    let index = timeline
        .iter()
        .position(|entry| entry.id == f.source)
        .ok_or("source absent")?;
    assert!(!timeline[..index]
        .iter()
        .rev()
        .take(HISTORY)
        .any(|entry| entry.id == f.other));
    assert_eq!(
        super::super::super::turn::turn(
            &f.client,
            &cx,
            &definition(),
            &f.space,
            &timeline[..index],
            &timeline[index],
        )
        .await,
        crate::agent::turn::Outcome::Handled
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "one proposal plus one answer"
    );
    assert!(f
        .client
        .tool_approval_requests(&f.space, &f.source)
        .is_empty());
    let timeline = f.client.timeline(f.space.clone())?;
    let replies: Vec<_> = timeline
        .iter()
        .filter(|entry| entry.reply_to.as_ref().is_some_and(|r| r.id == f.source))
        .collect();
    assert_eq!(replies.len(), 1);
    assert_eq!(
        text_of(replies[0]),
        Some(format!("Found message {}", f.other).as_str())
    );
    let reply = replies[0].id.clone();
    drop(cx);
    f.reopen();
    let cx = search_context(&f, &calls);
    let timeline = f.client.timeline(f.space.clone())?;
    let index = timeline
        .iter()
        .position(|entry| entry.id == f.source)
        .ok_or("source absent")?;
    assert_eq!(
        super::super::super::turn::turn(
            &f.client,
            &cx,
            &definition(),
            &f.space,
            &timeline[..index],
            &timeline[index],
        )
        .await,
        crate::agent::turn::Outcome::Handled
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "restart must not infer again"
    );
    let timeline = f.client.timeline(f.space.clone())?;
    assert_eq!(
        timeline
            .iter()
            .filter(|entry| entry.reply_to.as_ref().is_some_and(|r| r.id == f.source))
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>(),
        vec![reply]
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn known_search_recovery_requeries_edits_and_refuses_deleted_source_without_model_or_effect(
) -> TestResult {
    let mut f = Fixture::new("http://127.0.0.1:1")?;
    let calls = Arc::new(AtomicUsize::new(0));
    let cx = search_context(&f, &calls);
    let def = definition();
    prepare_read_checkpoint(&f, &cx, &def).await?;
    drop(cx);
    f.append(
        types::EventBody::MessageAnnotated {
            target: f.other.clone(),
            note: types::Annotation::Edit {
                text: "No longer matches the previous words".into(),
            },
        },
        true,
    )?;
    let cx = search_context(&f, &calls);
    let tools = SpaceTools::new(&f.client, &cx, &f.space, &f.owner, &f.source);
    let timeline = f.client.timeline(f.space.clone())?;
    let source = timeline
        .iter()
        .find(|entry| entry.id == f.source)
        .ok_or("source absent")?;
    let host = SourceTools::new(&tools, &cx, &def, source);
    let before = std::fs::read(&cx.journal)?;
    let (_, progress) = host.recover()?.ok_or("known plan not recovered")?;
    let page: MessageSearchPage = serde_json::from_str(&progress.steps[0].result)?;
    assert!(
        page.hits.is_empty(),
        "retained pre-edit plaintext must not become current evidence"
    );
    assert_eq!(std::fs::read(&cx.journal)?, before, "recovery only reads");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "only the original controlled proposal ran"
    );
    assert!(f
        .client
        .tool_approval_requests(&f.space, &f.source)
        .is_empty());
    drop(cx);
    f.append(
        types::EventBody::MessageAnnotated {
            target: f.source.clone(),
            note: types::Annotation::Delete,
        },
        true,
    )?;
    let cx = search_context(&f, &calls);
    let tools = SpaceTools::new(&f.client, &cx, &f.space, &f.owner, &f.source);
    let host = SourceTools::new(&tools, &cx, &def, source);
    assert!(host.recover().is_err());
    assert!(!tools.tools().iter().any(|tool| tool.name == history::TOOL));
    assert!(tools.call(&search_call()).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}

/// Stop at the production preparation boundary, retaining Runtime's genuine plan.
struct StopBeforeRead<'a, 'b, 'c> {
    host: &'a SourceTools<'b, 'c>,
    prepared: Mutex<Option<ToolAsk>>,
}
impl ToolHost for StopBeforeRead<'_, '_, '_> {
    fn tools(&self) -> Vec<ToolDef> {
        self.host.tools()
    }
    fn call(&self, call: &ToolCall) -> Result<String, String> {
        self.host.call(call)
    }
    fn begin_model(&self) -> Result<(), String> {
        self.host.begin_model()
    }
    fn model_calls(
        &self,
        calls: &[ToolCall],
        steps: &[ToolStep],
        round: usize,
    ) -> Result<(), String> {
        self.host.model_calls(calls, steps, round)
    }
    fn prepare(&self, ask: &ToolAsk) -> Result<(), String> {
        self.host.prepare(ask)?;
        *self.prepared.lock().map_err(|_| "capture poisoned")? = Some(ask.clone());
        Err("fixture interrupted after durable preparation, before read".into())
    }
}

async fn prepare_read_checkpoint(f: &Fixture, cx: &Ctx, def: &Definition) -> TestResult {
    let timeline = f.client.timeline(f.space.clone())?;
    let source = timeline
        .iter()
        .find(|entry| entry.id == f.source)
        .ok_or("source absent")?;
    let tools = SpaceTools::new(&f.client, cx, &f.space, &f.owner, &f.source);
    let host = SourceTools::new(&tools, cx, def, source);
    assert!(JournalState::begin_turn(&cx.journal, cx, &f.space, source)?);
    let stopped = StopBeforeRead {
        host: &host,
        prepared: Mutex::new(None),
    };
    let inbound = Turn {
        space: f.space.clone(),
        from: f.owner.clone(),
        text: text_of(source).ok_or("not text")?.into(),
    };
    let outcome = cx
        .runtime
        .turn_with_tools(
            TurnContext {
                agent: &cx.me,
                device: &cx.device,
                owner: &cx.owner,
                definition: def,
            },
            &[],
            &inbound,
            &stopped,
            vec![],
        )
        .await;
    assert!(
        outcome.is_err(),
        "fixture stops after the actual Runtime preparation"
    );
    let ask = stopped
        .prepared
        .lock()
        .map_err(|_| "capture poisoned")?
        .clone()
        .ok_or("no actual prepared read")?;
    assert_eq!(ask.call, search_call());
    let call = ask.call;

    let result = host.call(&call)?;
    assert_eq!(
        serde_json::from_str::<MessageSearchPage>(&result)?.hits[0].ref_id,
        f.other
    );
    host.checkpoint(&ToolProgress {
        round: 0,
        steps: vec![ToolStep {
            call,
            result,
            is_error: false,
        }],
        remaining: vec![],
    })?;
    Ok(())
}

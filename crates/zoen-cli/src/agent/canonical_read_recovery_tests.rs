//! A controlled offline read Ask followed by an accepted write and interrupted journal.
//! The only executor is the opt-in signed local WASI component; no model/provider login.
use super::canonical_tool_approval_tests::{definition, handle, reopen, signed_owner_resolution};
use super::*;
use futures_util::FutureExt;
use gate::{Answer, DecisionProvider, DecisionRequest, DecisionResponse, GateError, Question};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use zoen_agentd::citizen::BrainOut;

struct AskReadOnly(Arc<Mutex<Vec<DecisionRequest>>>);
impl DecisionProvider for AskReadOnly {
    fn name(&self) -> String {
        "canonical-read-recovery-offline-fixture".into()
    }
    fn on_device(&self) -> bool {
        true
    }
    fn decide(&self, request: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        self.0.lock().unwrap().push(request.clone());
        Ok(DecisionResponse {
            answers: request
                .questions
                .iter()
                .map(|question| {
                    let answer = match question {
                        Question::Predicate { name, .. } => Answer::Predicate {
                            p: if name == "matches_user_intent" {
                                1.0
                            } else {
                                0.0
                            },
                        },
                        Question::Score { name, .. } => Answer::Score {
                            score: 0.0,
                            confidence: if name == "severity"
                                && request.input.contains("read_deckcraft")
                            {
                                0.1
                            } else {
                                1.0
                            },
                        },
                        Question::Choice { .. } => unreachable!("turn gates only"),
                    };
                    (question.name().into(), answer)
                })
                .collect(),
        })
    }
}

struct Counters {
    expected_read: String,
    read_context_checks: AtomicUsize,
    initial: AtomicUsize,
    write_proposal: AtomicUsize,
    confirmation: AtomicUsize,
    writes: AtomicUsize,
}
impl Counters {
    fn new(expected_read: &Value) -> Arc<Self> {
        Arc::new(Self {
            expected_read: expected_read.to_string(),
            read_context_checks: AtomicUsize::new(0),
            initial: AtomicUsize::new(0),
            write_proposal: AtomicUsize::new(0),
            confirmation: AtomicUsize::new(0),
            writes: AtomicUsize::new(0),
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn context(
    client: &Arc<Client>,
    worker: Arc<zoen_agentd::deckcraft::DeckcraftWorker>,
    journal: &Path,
    read: &ToolCall,
    write: &ToolCall,
    counters: &Arc<Counters>,
    checks: &Arc<Mutex<Vec<DecisionRequest>>>,
    must_resume: bool,
) -> Ctx {
    let account = client.account().unwrap();
    let read = read.clone();
    let write = write.clone();
    let counters = counters.clone();
    Ctx {
        me: account.identity_id,
        device: account.device_id,
        owner: client.agent_owner().unwrap(),
        runtime: Runtime::new(
            Gate {
                on_device: Some(Box::new(AskReadOnly(checks.clone()))),
                ..Gate::default()
            },
            Box::new(zoen_agentd::citizen::testing::ControlledToolBrain {
                evaluate: Box::new(move |steps| match steps.len() {
                    0 => {
                        assert!(
                            !must_resume,
                            "cold recovery must not propose a replacement read"
                        );
                        counters.initial.fetch_add(1, Ordering::SeqCst);
                        Ok(BrainOut::CallTools(vec![read.clone()]))
                    }
                    1 => {
                        assert_eq!(steps[0].call, read);
                        assert!(!steps[0].is_error);
                        assert_eq!(steps[0].result, counters.expected_read);
                        counters.read_context_checks.fetch_add(1, Ordering::SeqCst);
                        counters.write_proposal.fetch_add(1, Ordering::SeqCst);
                        Ok(BrainOut::CallTools(vec![write.clone()]))
                    }
                    2 => {
                        assert_eq!(steps[0].call, read);
                        assert_eq!(steps[0].result, counters.expected_read);
                        counters.read_context_checks.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(steps[1].call, write);
                        assert!(!steps[1].is_error);
                        let result: Value = serde_json::from_str(&steps[1].result).unwrap();
                        assert_eq!(result["version"], 2);
                        assert_eq!(result["replayed"], true);
                        counters.confirmation.fetch_add(1, Ordering::SeqCst);
                        Ok(BrainOut::Say("Recovered the exact reviewed notes.".into()))
                    }
                    count => panic!("unexpected completed-step count: {count}"),
                }),
            }),
        ),
        deckcraft: Some(worker),
        journal: journal.into(),
    }
}

struct CrashAfterAcceptedWrite<'a, 'b> {
    tools: &'a SpaceTools<'b>,
    counters: &'a Counters,
}
impl ToolHost for CrashAfterAcceptedWrite<'_, '_> {
    fn tools(&self) -> Vec<ToolDef> {
        self.tools.tools()
    }
    fn gate_public_references(
        &self,
        call: &ToolCall,
    ) -> Option<zoen_agentd::PublicArtifactReferences> {
        self.tools.gate_public_references(call)
    }
    fn prepare(&self, prepared: &ToolAsk) -> Result<(), String> {
        self.tools.prepare(prepared)
    }
    fn checkpoint(&self, progress: &ToolProgress) -> Result<(), String> {
        self.tools.checkpoint(progress)
    }
    fn call(&self, call: &ToolCall) -> Result<String, String> {
        let (_, item, canonical) = self
            .tools
            .names
            .iter()
            .find(|(definition, _, _)| definition.name == call.name)
            .unwrap();
        assert_eq!(canonical, "deckcraft_slide_notes");
        let worker = self.tools.deckcraft.unwrap();
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(crate::deckcraft::call(
                self.tools.e,
                worker,
                item,
                self.tools.source,
                self.tools.serving,
                canonical,
                &call.arguments,
            ))
        })
        .unwrap();
        let result: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(result["version"], 2);
        assert_eq!(result["replayed"], false);
        self.counters.writes.fetch_add(1, Ordering::SeqCst);
        // The native atomic receipt and v2 exist, while the prepared journal entry is
        // intentionally retained. No SpaceTools::record or post-effect checkpoint ran.
        panic!("controlled interruption after accepted write before journal checkpoint")
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "explicit offline signed WASI component; no inference, provider credentials or services"]
async fn offline_read_ask_accepted_write_and_cold_recovery_never_repeat_the_effect() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let worker = crate::deckcraft::tests::worker();
    let fixture = crate::deckcraft::tests::fixture();
    let inspected = crate::deckcraft::tests::inspect(&fixture, &worker).await;
    assert!(!inspected["slides"].as_array().unwrap().is_empty());
    assert!(inspected["slides"][0]["id"].is_u64());
    assert!(!inspected["slides"][0]["shapes"]
        .as_array()
        .unwrap()
        .is_empty());
    let space = fixture
        .client
        .item(fixture.base.item_id.clone())
        .unwrap()
        .space_id;
    let read = ToolCall {
        id: "held_synthetic_read".into(),
        name: crate::deckcraft::model_name("read_deckcraft", &fixture.base.item_id).unwrap(),
        arguments: json!({"artifactId":fixture.base.item_id}),
    };
    let write = ToolCall {
        id: "prepared_synthetic_notes".into(),
        name: crate::deckcraft::model_name("deckcraft_slide_notes", &fixture.base.item_id).unwrap(),
        arguments: json!({"artifactId":fixture.base.item_id,"version":fixture.base.version,"sha256":fixture.base.sha256,"slideId":inspected["slides"][0]["id"],"text":"Accepted once, recovered without another edit."}),
    };
    let counters = Counters::new(&inspected);
    let checks = Arc::new(Mutex::new(Vec::new()));
    let journal = fixture.path.join("canonical-read-recovery-steps.json");
    let cx = context(
        &fixture.client,
        worker.clone(),
        &journal,
        &read,
        &write,
        &counters,
        &checks,
        false,
    );
    handle(&fixture.client, &cx, &space, &fixture.source).await;
    assert_eq!(counters.initial.load(Ordering::SeqCst), 1);
    assert_eq!(checks.lock().unwrap().len(), 2);
    let requests = fixture
        .client
        .tool_approval_requests(&space, &fixture.source);
    assert_eq!(requests.len(), 1);
    let request = requests[0].clone();
    assert_eq!(
        request.tool_proposal.as_ref().unwrap().canonical_tool,
        "read_deckcraft"
    );
    assert_eq!(
        fixture.client.requests()[0].status,
        client_core::RequestStatus::Pending
    );
    let path = fixture.path.clone();
    let vault = fixture.vault.clone();
    let item = fixture.base.item_id.clone();
    let source = fixture.source.clone();
    let owner = fixture.owner.clone();
    drop(cx);
    drop(fixture);
    signed_owner_resolution(&path, &space, &request, &owner);

    let accepted;
    {
        let client = reopen(&path, vault.clone());
        let cx = context(
            &client,
            worker.clone(),
            &journal,
            &read,
            &write,
            &counters,
            &checks,
            true,
        );
        let tools = SpaceTools::new(&client, &cx, &space, &owner, &source);
        assert!(tools
            .begin_resume(&request.id, &request.content_hash)
            .unwrap());
        let permit = client
            .approved_tool_turn(&request.id, worker.binding_sha256())
            .unwrap();
        let original: ToolAsk = serde_json::from_str(&permit.proposal().continuation_json).unwrap();
        let inbound = Turn {
            space: space.clone(),
            from: owner.clone(),
            text: client
                .timeline(space.clone())
                .unwrap()
                .iter()
                .find(|entry| entry.id == source)
                .and_then(text_of)
                .unwrap()
                .into(),
        };
        let definition = definition();
        let tc = TurnContext {
            agent: &cx.me,
            device: &cx.device,
            owner: &cx.owner,
            definition: &definition,
        };
        let admission = CanonicalToolAdmission {
            tools: &tools,
            permit,
        };
        let interrupted = CrashAfterAcceptedWrite {
            tools: &tools,
            counters: &counters,
        };
        let future =
            cx.runtime
                .resume_tool_ask(tc, &[], &inbound, &interrupted, original, &admission);
        assert!(std::panic::AssertUnwindSafe(future)
            .catch_unwind()
            .await
            .is_err());
        assert_eq!(counters.writes.load(Ordering::SeqCst), 1);
        assert_eq!(counters.write_proposal.load(Ordering::SeqCst), 1);
        assert_eq!(counters.confirmation.load(Ordering::SeqCst), 0);
        accepted = client.app_binary_read(item.clone(), None, None).unwrap();
        assert_eq!(accepted.version, 2);
        let state = JournalState::load(&journal).unwrap();
        let budget = &state.continuations[&request.id];
        assert_eq!(budget.attempts, 1);
        assert_eq!(budget.source.as_deref(), Some(source.as_str()));
        let progress = budget.progress.as_ref().unwrap();
        assert_eq!(progress.round, 0);
        assert_eq!(progress.steps.len(), 1);
        assert_eq!(progress.steps[0].call, read);
        assert_eq!(progress.steps[0].result, counters.expected_read);
        let prepared = budget.prepared.as_ref().unwrap();
        assert_eq!(prepared.round, 1);
        assert_eq!(prepared.call, write);
        assert_eq!(prepared.steps, progress.steps);
        assert_eq!(
            checks.lock().unwrap().len(),
            3,
            "write gate ran exactly once before the accepted effect"
        );
    }
    let client = reopen(&path, vault.clone());
    let cx = context(
        &client,
        worker.clone(),
        &journal,
        &read,
        &write,
        &counters,
        &checks,
        true,
    );
    handle(&client, &cx, &space, &source).await;
    assert_eq!(counters.initial.load(Ordering::SeqCst), 1);
    assert_eq!(counters.write_proposal.load(Ordering::SeqCst), 1);
    assert_eq!(counters.writes.load(Ordering::SeqCst), 1);
    assert_eq!(counters.confirmation.load(Ordering::SeqCst), 1);
    assert_eq!(counters.read_context_checks.load(Ordering::SeqCst), 2);
    assert_eq!(
        checks.lock().unwrap().len(),
        4,
        "recovery only checks the final outbound reply"
    );
    assert_eq!(
        client.app_binary_read(item.clone(), None, None).unwrap(),
        accepted
    );
    assert_eq!(client.item(item.clone()).unwrap().versions.len(), 2);
    let state = JournalState::load(&journal).unwrap();
    let budget = &state.continuations[&request.id];
    assert_eq!(budget.attempts, 2);
    assert!(budget.prepared.is_none());
    let progress = budget.progress.as_ref().unwrap();
    assert_eq!(progress.round, 1);
    assert_eq!(progress.steps.len(), 2);
    assert_eq!(progress.steps[0].call, read);
    assert_eq!(progress.steps[1].call, write);
    let replay: Value = serde_json::from_str(&progress.steps[1].result).unwrap();
    assert_eq!(replay["replayed"], true);
    let replies = client
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .filter(|entry| {
            entry.author.id == cx.me
                && entry
                    .reply_to
                    .as_ref()
                    .is_some_and(|reply| reply.id == source)
        })
        .count();
    assert_eq!(replies, 1);
    handle(&client, &cx, &space, &source).await;
    assert_eq!(checks.lock().unwrap().len(), 4);
    assert_eq!(counters.confirmation.load(Ordering::SeqCst), 1);
    assert_eq!(
        JournalState::load(&journal).unwrap().continuations[&request.id].attempts,
        2
    );
    drop(cx);
    drop(client);
    let cold = reopen(&path, vault);
    assert_eq!(
        cold.app_binary_read(item.clone(), None, None).unwrap(),
        accepted
    );
    assert_eq!(cold.item(item).unwrap().versions.len(), 2);
    drop(cold);
    std::fs::remove_dir_all(path).unwrap();
}

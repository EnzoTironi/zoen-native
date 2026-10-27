//! Offline, controlled policy/brain with the real signed core and WASI executor.
//! Explicit component opt-in only; this never reads a provider login or makes inference.
use super::*;
use gate::{Answer, DecisionProvider, DecisionRequest, DecisionResponse, GateError, Question};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use zoen_agentd::citizen::BrainOut;

struct AskTools(Arc<AtomicUsize>);
impl DecisionProvider for AskTools {
    fn name(&self) -> String {
        "canonical-approval-offline-fixture".into()
    }
    fn on_device(&self) -> bool {
        true
    }
    fn decide(&self, request: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(DecisionResponse {
            answers: request
                .questions
                .iter()
                .map(|q| {
                    let answer = match q {
                        Question::Predicate { name, .. } => Answer::Predicate {
                            p: if name == "matches_user_intent" {
                                1.0
                            } else {
                                0.0
                            },
                        },
                        Question::Score { name, .. } => Answer::Score {
                            score: 0.0,
                            confidence: if name == "severity" { 0.1 } else { 1.0 },
                        },
                        Question::Choice { .. } => unreachable!("turn gates only"),
                    };
                    (q.name().into(), answer)
                })
                .collect(),
        })
    }
}
struct ReviewedBrain {
    call: ToolCall,
    initial: Arc<AtomicUsize>,
    completed: Arc<AtomicUsize>,
    must_resume: bool,
}
impl ReviewedBrain {
    fn evaluate(&self, steps: &[ToolStep]) -> Result<BrainOut, String> {
        if steps.is_empty() {
            assert!(
                !self.must_resume,
                "cold restart must not request a fresh proposal"
            );
            self.initial.fetch_add(1, Ordering::SeqCst);
            Ok(BrainOut::CallTools(vec![self.call.clone()]))
        } else {
            assert_eq!(steps.len(), 1);
            assert_eq!(steps[0].call, self.call);
            assert!(!steps[0].is_error);
            self.completed.fetch_add(1, Ordering::SeqCst);
            Ok(BrainOut::Say("Applied the exact reviewed notes.".into()))
        }
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "explicit independent offline fixture counters"
)]
pub(super) fn context(
    e: &Arc<Client>,
    worker: Arc<zoen_agentd::deckcraft::DeckcraftWorker>,
    journal: &Path,
    call: &ToolCall,
    initial: &Arc<AtomicUsize>,
    completed: &Arc<AtomicUsize>,
    decisions: &Arc<AtomicUsize>,
    must_resume: bool,
) -> Ctx {
    let account = e.account().unwrap();
    Ctx {
        me: account.identity_id,
        device: account.device_id,
        owner: e.agent_owner().unwrap(),
        runtime: Runtime::new(
            Gate {
                on_device: Some(Box::new(AskTools(decisions.clone()))),
                ..Gate::default()
            },
            {
                let controlled = ReviewedBrain {
                    call: call.clone(),
                    initial: initial.clone(),
                    completed: completed.clone(),
                    must_resume,
                };
                Box::new(zoen_agentd::citizen::testing::ControlledToolBrain {
                    evaluate: Box::new(move |steps| controlled.evaluate(steps).map_err(Into::into)),
                })
            },
        ),
        deckcraft: Some(worker),
        journal: journal.into(),
    }
}
pub(super) fn definition() -> Definition {
    serde_json::from_value(json!({"version":1,"kind":"user","persona":"Offline synthetic reviewed presentation fixture","rules":[],"state":{}})).unwrap()
}
pub(super) async fn handle(e: &Arc<Client>, cx: &Ctx, space: &str, source: &str) {
    let timeline = e.timeline(space.into()).unwrap();
    let index = timeline
        .iter()
        .position(|entry| entry.id == source)
        .unwrap();
    assert_eq!(
        turn(
            e,
            cx,
            &definition(),
            space,
            &timeline[..index],
            &timeline[index]
        )
        .await,
        turn::Outcome::Handled
    );
}
pub(super) fn reopen(path: &Path, vault: Arc<dyn client_core::SecretVault>) -> Arc<Client> {
    let client = Client::open(
        path.join("agent.sqlite").to_str().unwrap().into(),
        "en".into(),
    )
    .unwrap();
    assert!(client.unlock(vault).unwrap());
    client
}
/// Sequence only signed synthetic fixture events, as a relay would. No second
/// state authority: the reopened native core verifies and projects the same log.
pub(super) fn signed_owner_resolution(
    path: &Path,
    space: &str,
    request: &types::AgentRequest,
    owner: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let db = store::Store::open(path.join("agent.sqlite").to_str().unwrap()).unwrap();
    let key = db
        .identities()
        .unwrap()
        .into_iter()
        .find(|(identity, _)| identity.id == owner)
        .unwrap()
        .1
        .expect("synthetic owner root retained by fixture");
    let signer = event_log::Signer::from_secret(&key);
    let mut log = event_log::SpaceLog::from_events(space, db.events(space).unwrap()).unwrap();
    let existing: BTreeSet<_> = log
        .events()
        .iter()
        .map(|event| event.client_id.clone())
        .collect();
    for pending in db
        .outbox()
        .unwrap()
        .into_iter()
        .filter(|pending| pending.event.space == space)
    {
        if !existing.contains(&pending.event.client_id) {
            db.append_event(log.sequence(pending.event.clone())?)
                .unwrap();
        }
        db.outbox_remove(&pending.event.client_id).unwrap();
    }
    log.verify().unwrap();
    let event = log.append(
        &signer,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64,
        types::EventBody::RequestResolved {
            request: request.id.clone(),
            approved: true,
            content_hash: request.content_hash.clone(),
            resolution: Some(types::RequestResolution {
                message: "Approved the exact synthetic notes once.".into(),
                created_item: None,
                ai_cost_cents: 0,
                standing_grant: None,
                tool_decision: Some(types::ToolDecision::ApproveOnce),
            }),
        },
    )?;
    db.append_event(event).unwrap();
    log.verify().unwrap();
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "explicit offline signed WASI component; no providers, credentials or services"]
async fn offline_canonical_ask_owner_resume_receipt_and_cold_reopen(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let worker = crate::deckcraft::tests::worker();
    let fixture = crate::deckcraft::tests::fixture()?;
    let space = fixture
        .client
        .item(fixture.base.item_id.clone())
        .unwrap()
        .space_id;
    let inspected = crate::deckcraft::tests::inspect(&fixture, &worker).await;
    let slide = inspected["slides"][0]["id"].clone();
    let call = ToolCall {
        requires_review: false,
        id: "held_synthetic_notes".into(),
        name: crate::deckcraft::model_name("deckcraft_slide_notes", &fixture.base.item_id).unwrap(),
        arguments: json!({"artifactId":fixture.base.item_id,"version":fixture.base.version,"sha256":fixture.base.sha256,"slideId":slide,"text":"Exact canonical approval notes."}),
    };
    let initial = Arc::new(AtomicUsize::new(0));
    let completed = Arc::new(AtomicUsize::new(0));
    let decisions = Arc::new(AtomicUsize::new(0));
    let journal = fixture.path.join("canonical-tool-steps.json");
    let cx = context(
        &fixture.client,
        worker.clone(),
        &journal,
        &call,
        &initial,
        &completed,
        &decisions,
        false,
    );
    handle(&fixture.client, &cx, &space, &fixture.source).await;
    assert_eq!(initial.load(Ordering::SeqCst), 1);
    assert_eq!(completed.load(Ordering::SeqCst), 0);
    assert_eq!(
        decisions.load(Ordering::SeqCst),
        2,
        "one inbound and one held tool check"
    );
    let requests = fixture
        .client
        .tool_approval_requests(&space, &fixture.source);
    assert_eq!(requests.len(), 1);
    let request = requests[0].clone();
    assert_eq!(
        fixture.client.requests()[0].status,
        client_core::RequestStatus::Pending
    );
    for _ in 0..2 {
        handle(&fixture.client, &cx, &space, &fixture.source).await;
    }
    assert_eq!(initial.load(Ordering::SeqCst), 1);
    assert_eq!(
        decisions.load(Ordering::SeqCst),
        2,
        "pending wakes do no classification"
    );
    assert_eq!(
        fixture
            .client
            .app_binary_read(fixture.base.item_id.clone(), None, None)
            .unwrap(),
        fixture.base
    );
    let path = fixture.path.clone();
    let vault = fixture.vault.clone();
    let item = fixture.base.item_id.clone();
    let source = fixture.source.clone();
    let owner = fixture.owner.clone();
    drop(cx);
    drop(fixture);
    signed_owner_resolution(&path, &space, &request, &owner)?;
    let client = reopen(&path, vault.clone());
    assert_eq!(client.approved_tool_sources(&space), vec![source.clone()]);
    // Reserve the same persistent attempt before execution, as the host does.
    let execution_context = context(
        &client,
        worker.clone(),
        &journal,
        &call,
        &initial,
        &completed,
        &decisions,
        true,
    );
    let execution_tools = SpaceTools::new(&client, &execution_context, &space, &owner, &source);
    assert!(execution_tools
        .begin_resume(&request.id, &request.content_hash)
        .unwrap());
    drop(execution_tools);
    drop(execution_context);
    // Restart after owner approval, then stop after canonical effect/before journal.
    let permit = client
        .approved_tool_turn(&request.id, worker.binding_sha256())
        .unwrap();
    let actual: serde_json::Value = serde_json::from_str(
        &crate::deckcraft::call_approved(&client, &worker, &permit)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(actual["version"], 2);
    assert_eq!(actual["replayed"], false);
    let accepted = client.app_binary_read(item.clone(), None, None).unwrap();
    assert_eq!(accepted.version, 2);
    assert!(JournalState::load(&journal).unwrap().steps.is_empty());
    assert_eq!(
        JournalState::load(&journal).unwrap().continuations[&request.id].attempts,
        1
    );
    drop(permit);
    drop(client);
    let client = reopen(&path, vault.clone());
    let cx = context(
        &client,
        worker.clone(),
        &journal,
        &call,
        &initial,
        &completed,
        &decisions,
        true,
    );
    handle(&client, &cx, &space, &source).await;
    assert_eq!(
        initial.load(Ordering::SeqCst),
        1,
        "no replacement proposal after restart"
    );
    assert_eq!(completed.load(Ordering::SeqCst), 1);
    assert_eq!(
        decisions.load(Ordering::SeqCst),
        3,
        "only subsequent outbound checked; held call not reclassified"
    );
    assert_eq!(
        client.app_binary_read(item.clone(), None, None).unwrap(),
        accepted
    );
    assert_eq!(client.item(item.clone()).unwrap().versions.len(), 2);
    let steps = JournalState::load(&journal).unwrap().steps;
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].0, source);
    assert_eq!(steps[0].1.len(), 1);
    assert_eq!(steps[0].1[0].call, call);
    assert_eq!(
        JournalState::load(&journal).unwrap().continuations[&request.id].attempts,
        2
    );
    let result: Value = serde_json::from_str(&steps[0].1[0].result).unwrap();
    assert_eq!(result["replayed"], true);
    handle(&client, &cx, &space, &source).await;
    assert_eq!(completed.load(Ordering::SeqCst), 1);
    assert_eq!(decisions.load(Ordering::SeqCst), 3);
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
    Ok(())
}

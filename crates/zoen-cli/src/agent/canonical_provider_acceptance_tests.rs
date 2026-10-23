//! Explicit operator acceptance only. Ordinary tests exercise the guards without
//! providers, credentials, network, operator workers or shared application state.
// Existing private fixture implementations; all state/types and the operator entry stay here.
#[path = "canonical_provider_acceptance/contract.rs"]
mod contract;

use super::*;
use gate::{DecisionProvider, DecisionRequest, DecisionResponse, GateError};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicUsize, Ordering};
use zoen_agentd::citizen::{testing::GuardedBrain, BrainOut};

const CLEF_CAP: usize = 4;
const CODEX_CAP: usize = 3;
const WALL: Duration = Duration::from_secs(720);
const NOTES: &str = "Synthetic provider QA notes.";
const REPLY: &str = "Applied the reviewed notes.";

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
#[derive(Default)]
struct Counts {
    clef: usize,
    codex: usize,
    refused: usize,
    trace: Vec<Value>,
}
struct Budget {
    deadline: Instant,
    counts: Mutex<Counts>,
    evidence: Option<std::path::PathBuf>,
}

struct BoundedClef {
    inner: Box<dyn DecisionProvider>,
    budget: Arc<Budget>,
}
impl DecisionProvider for BoundedClef {
    fn name(&self) -> String {
        self.inner.name()
    }
    fn on_device(&self) -> bool {
        self.inner.on_device()
    }
    fn decide(&self, request: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        let index = self
            .budget
            .reserve("clef")
            .map_err(GateError::Unavailable)?;
        let result = self.inner.decide(request);
        let response = match &result {
            Ok(response) => json!({"state":"completed","answers":response.answers}),
            Err(_) => json!({"state":"failed","errorCategory":"provider-or-transport"}),
        };
        let mut state = self.budget.counts.lock().unwrap();
        state.trace[index] = json!({"kind":"clef","provider":self.inner.name(),
            "inputSha256":sha(request.input.as_bytes()),"questions":request.questions.iter().map(|q|q.name()).collect::<Vec<_>>(),"response":response});
        self.budget.persist(&state);
        result
    }
}

#[derive(Clone)]
struct Expected {
    item: String,
    version: u32,
    head: String,
    worker_binding: String,
}

fn guarded(
    inner: Box<dyn Brain>,
    budget: &Arc<Budget>,
    expected: Expected,
    worker_binding: String,
) -> Box<dyn Brain> {
    let before = budget.clone();
    let after = budget.clone();
    Box::new(GuardedBrain {
        inner,
        before: Arc::new(move || before.reserve("codex").map(|_| ())),
        after: Arc::new(move |steps, output| {
            let decision = expected.admit(&worker_binding, steps, output);
            after.note(json!({"kind":"codex-result","completedSteps":steps.len(),
                "state":if decision.is_ok(){"admitted-synthetic-shape"}else{"rejected-synthetic-shape"},
                "reason":decision.as_ref().err(),"shape":expected.shape(&worker_binding,steps,output)}));
            decision
        }),
    })
}
fn context(
    client: &Client,
    worker: Arc<zoen_agentd::deckcraft::DeckcraftWorker>,
    journal: &Path,
    budget: &Arc<Budget>,
    expected: Expected,
) -> Ctx {
    let mut gate = gate().expect("explicit production Workers AI Gate required");
    let remote = gate.remote.take().expect("remote Clef required");
    assert_eq!(remote.name(), "cloudflare:clef-flash");
    gate.remote = Some(Box::new(BoundedClef {
        inner: remote,
        budget: budget.clone(),
    }));
    let account = client.account().unwrap();
    Ctx {
        me: account.identity_id,
        device: account.device_id,
        owner: client.agent_owner().unwrap(),
        runtime: Runtime::new(
            gate,
            guarded(
                brain().expect("explicit production Codex selector required"),
                budget,
                expected,
                worker.binding_sha256().into(),
            ),
        ),
        deckcraft: Some(worker),
        journal: journal.into(),
    }
}
fn definition() -> Definition {
    serde_json::from_value(json!({"version":1,"kind":"user","persona":format!("Validate only this synthetic presentation. First propose exactly one read_deckcraft call for the offered artifact. From its successful result, propose exactly one deckcraft_slide_notes call for the first slide and exact observed version/sha256, setting notes to {NOTES:?}. Then say exactly {REPLY:?}. Do not repeat completed steps or propose any other operation. Calls use unique ids, offered names and arguments_json as serialized JSON. You only propose; the Zoen Runtime owns every action and approval."),"rules":[],"state":{}})).unwrap()
}
async fn drive(client: &Arc<Client>, cx: &Ctx, space: &str, source: &str) -> bool {
    let timeline = client.timeline(space.into()).unwrap();
    let index = timeline
        .iter()
        .position(|entry| entry.id == source)
        .unwrap();
    turn(
        client,
        cx,
        &definition(),
        space,
        &timeline[..index],
        &timeline[index],
    )
    .await
}

/// Only a relay simulation for signed test events. Never copies secret material,
/// edits event bodies, impersonates an account, or opens a concurrently owned DB.
fn sequence_and_transfer(path: &Path, from: &str, to: &str, space: &str) {
    let source = store::Store::open(path.join(from).to_str().unwrap()).unwrap();
    let mut log = event_log::SpaceLog::from_events(space, source.events(space).unwrap()).unwrap();
    for pending in source
        .outbox()
        .unwrap()
        .into_iter()
        .filter(|p| p.event.space == space)
    {
        if !log
            .events()
            .iter()
            .any(|e| e.client_id == pending.event.client_id)
        {
            event_log::verify_author(&pending.event).unwrap();
            source
                .append_event(log.sequence(pending.event.clone()))
                .unwrap();
        }
        source.outbox_remove(&pending.event.client_id).unwrap();
    }
    log.verify().unwrap();
    let events = source.events(space).unwrap();
    let public: Vec<_> = source
        .identities()
        .unwrap()
        .into_iter()
        .map(|(profile, _)| profile)
        .collect();
    drop(source);
    let destination = store::Store::open(path.join(to).to_str().unwrap()).unwrap();
    for profile in public {
        destination.put_identity(&profile, None).unwrap();
    }
    let existing = destination.events(space).unwrap();
    assert!(existing.len() <= events.len());
    assert_eq!(
        serde_json::to_value(&existing).unwrap(),
        serde_json::to_value(&events[..existing.len()]).unwrap()
    );
    for event in &events[existing.len()..] {
        destination.append_event(event).unwrap();
    }
    event_log::SpaceLog::from_events(space, destination.events(space).unwrap())
        .unwrap()
        .verify()
        .unwrap();
}
async fn approve_as_owner(
    path: &Path,
    vault: Arc<dyn client_core::SecretVault>,
    space: &str,
    request: &client_core::AgentRequestDto,
) {
    sequence_and_transfer(path, "agent.sqlite", "owner.sqlite", space);
    let owner = Client::open(
        path.join("owner.sqlite").to_str().unwrap().into(),
        "en".into(),
    )
    .unwrap();
    assert!(owner.unlock(vault).unwrap());
    let hash = request.tool_approval.as_ref().unwrap().content_hash.clone();
    // Both probes are local core decisions, never additional model calls.
    assert!(owner
        .client_request(
            json!({"op":"decide_request","request_id":request.id,"decision":"Approve"}).to_string()
        )
        .await
        .is_err());
    assert!(owner.client_request(json!({"op":"decide_tool_request","request_id":request.id,"observed_content_hash":"00".repeat(32),"decision":"ApproveOnce"}).to_string()).await.is_err());
    let command=json!({"op":"decide_tool_request","request_id":request.id,"observed_content_hash":hash,"decision":"ApproveOnce"}).to_string();
    let first: Value =
        serde_json::from_str(&owner.client_request(command.clone()).await.unwrap()).unwrap();
    let repeated: Value =
        serde_json::from_str(&owner.client_request(command).await.unwrap()).unwrap();
    assert_eq!(first, repeated);
    assert_eq!(first["request"]["status"], "Approved");
    assert_eq!(first["standing_grant_id"], Value::Null);
    drop(owner);
    sequence_and_transfer(path, "owner.sqlite", "agent.sqlite", space);
}
fn private_write(path: &Path, value: &Value) {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).unwrap();
    file.write_all(&serde_json::to_vec_pretty(value).unwrap())
        .unwrap();
    file.sync_all().unwrap();
}
struct Evidence {
    directory: std::path::PathBuf,
    budget: Arc<Budget>,
}
impl Drop for Evidence {
    fn drop(&mut self) {
        if !self.directory.join("outcome.json").exists() {
            private_write(
                &self.directory.join("outcome.json"),
                &json!({"status":"PARTIAL_STOPPED_BEFORE_STATE_OBSERVATION","budgets":self.budget.snapshot(),"automaticRetryForbidden":true}),
            );
        }
        private_write(
            &self.directory.join("provider-trace.json"),
            &self.budget.snapshot(),
        );
    }
}
fn mark_once(directory: &Path) -> std::io::Result<()> {
    let mut once = std::fs::OpenOptions::new();
    once.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        once.mode(0o600);
    }
    let mut file = once.open(directory.join("one-attempt.marker"))?;
    file.write_all(b"One explicitly dispatched attempt; never repeat automatically.\n")?;
    file.sync_all()?;
    std::fs::File::open(directory)?.sync_all()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit operator-only real Codex+Clef; one attempt, four Clef dispatches, three Codex turns; never ordinary QA"]
async fn operator_codex_clef_canonical_approval_replay_and_cold_reopen() {
    assert_eq!(std::env::var("ZOEN_TEST_REAL_CODEX").as_deref(), Ok("1"));
    assert_eq!(std::env::var("ZOEN_LIVE_CLEF_QA").as_deref(), Ok("1"));
    assert_eq!(std::env::var("ZOEN_BRAIN").as_deref(), Ok("codex-local"));
    assert_ne!(std::env::var("ZOEN_AGENT_GATE").as_deref(), Ok("offline"));
    assert!(
        std::env::var_os("ZOEN_AGENT_SCRIPT").is_none()
            && std::env::var_os("ZOEN_AGENT_FAULT").is_none()
    );
    let directory = std::path::PathBuf::from(
        std::env::var_os("ZOEN_CANONICAL_PROVIDER_QA_OUT")
            .expect("explicit private evidence directory required"),
    );
    assert!(directory.is_absolute() && directory.is_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&directory).unwrap().permissions().mode() & 0o077,
            0
        );
    }
    let source_head = std::env::var("ZOEN_CANONICAL_PROVIDER_QA_HEAD")
        .expect("explicit source head binding required");
    assert!(source_head.len() == 40 && source_head.bytes().all(|b| b.is_ascii_hexdigit()));
    mark_once(&directory).expect("never repeat a provider attempt");
    let budget = Budget::with_evidence(&directory);
    let evidence = Evidence {
        directory: directory.clone(),
        budget: budget.clone(),
    };
    private_write(
        &directory.join("setup-plan.json"),
        &json!({"sourceHead":source_head,"compiledTestSha256":sha(include_bytes!("canonical_provider_acceptance_tests.rs")),"syntheticOnly":true,
        "limits":budget.snapshot()["limits"],"expectedTools":["read_deckcraft","deckcraft_slide_notes"],"automaticRetries":0,
        "requiredEnvironmentNames":["ZOEN_BRAIN","ZOEN_CODEX_BIN","ZOEN_CODEX_MODEL","CLOUDFLARE_ACCOUNT_ID","CLOUDFLARE_WORKERS_AI_TOKEN","ZOEN_DECKCRAFT_WORKER_COMPONENT","ZOEN_DECKCRAFT_WORKER_MANIFEST","ZOEN_DECKCRAFT_WORKER_PUBLISHER"],
        "credentialAcquisition":"Root wrapper only: existing Wrangler whoami/auth token --json captured in memory, no raw output; Codex owns managed login via stdio account/read; never copy auth.json",
        "noAccountTokenOrRpcLogs":true,"providerBillingAndInternalRetriesUnmeasured":true}),
    );
    let _ = rustls::crypto::ring::default_provider().install_default();
    let worker = crate::deckcraft::configured()
        .unwrap()
        .expect("explicit operator-signed worker required");
    let profile = directory.join("synthetic-profile");
    std::fs::create_dir(&profile).unwrap();
    let crate::deckcraft::tests::Fixture {
        path,
        mut client,
        vault,
        owner_vault,
        owner,
        source,
        base,
    } = crate::deckcraft::tests::fixture_at(profile, [93; 32]);
    let space = client.item(base.item_id.clone()).unwrap().space_id;
    let expected = Expected {
        item: base.item_id.clone(),
        version: base.version,
        head: base.sha256.clone(),
        worker_binding: worker.binding_sha256().into(),
    };
    let journal = path.join("agent-tool-steps.json");
    let mut approvals = Vec::new();
    for _ in 0..=2 {
        let cx = context(&client, worker.clone(), &journal, &budget, expected.clone());
        let handled = tokio::time::timeout_at(
            tokio::time::Instant::from_std(budget.deadline),
            drive(&client, &cx, &space, &source),
        )
        .await;
        if !matches!(handled, Ok(true)) || !budget.intact() {
            break;
        }
        let pending = client
            .requests()
            .into_iter()
            .find(|r| r.status == client_core::RequestStatus::Pending && r.tool_approval.is_some());
        let Some(request) = pending else {
            break;
        };
        if approvals.len() >= 2 {
            break;
        }
        let proposal = client
            .tool_approval_requests(&space, &source)
            .into_iter()
            .find(|r| r.id == request.id)
            .unwrap();
        approvals.push(json!({"request":request.id,"contentHash":request.tool_approval.as_ref().unwrap().content_hash,"proposal":proposal.tool_proposal}));
        drop(cx);
        drop(client);
        approve_as_owner(&path, owner_vault.clone(), &space, &request).await;
        client = canonical_tool_approval_tests::reopen(&path, vault.clone());
    }
    let document = client
        .app_binary_read(base.item_id.clone(), None, None)
        .unwrap();
    let steps = JournalState::load(&journal)
        .unwrap()
        .steps
        .into_iter()
        .find(|(id, _)| id == &source)
        .map(|(_, steps)| steps)
        .unwrap_or_default();
    let reply = client
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .any(|m| {
            m.author.id == client.account().unwrap().identity_id
                && m.reply_to.as_ref().is_some_and(|q| q.id == source)
                && text_of(&m) == Some(REPLY)
        });
    let complete = document.version == 2
        && steps.len() == 2
        && steps.iter().all(|s| !s.is_error)
        && reply
        && !approvals.is_empty()
        && budget.intact();
    let mut outcome = json!({"status":if complete {"REAL_EDIT_AND_APPROVAL_PENDING_REPLAY_PROOF"}else{"PARTIAL_PRESERVED_PROVIDER_OR_GATE_OUTCOME"},
        "syntheticOnly":true,"baseVersion":base.version,"baseSha256":base.sha256,"version":document.version,"sha256":document.sha256,"toolSteps":steps,"approvals":approvals,"replyPublished":reply,"budgets":budget.snapshot(),"workerBinding":worker.binding_sha256(),"approvalPort":"actual owner Client.client_request decide_tool_request; signed fixture relay projection only"});
    private_write(&directory.join("outcome.json"), &outcome);
    assert!(
        complete,
        "real acceptance incomplete; preserve outcome.json, do not reclassify or repeat"
    );
    let repeated: Value = serde_json::from_str(
        &crate::deckcraft::call(
            &client,
            &worker,
            &base.item_id,
            &source,
            &owner,
            "deckcraft_slide_notes",
            &expected.write(&steps[0]).unwrap().arguments,
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(repeated["replayed"], true);
    let before = budget.snapshot();
    drop(client);
    let cold = canonical_tool_approval_tests::reopen(&path, vault);
    assert_eq!(
        cold.app_binary_read(base.item_id.clone(), None, None)
            .unwrap(),
        document
    );
    assert_eq!(cold.item(base.item_id).unwrap().versions.len(), 2);
    assert_eq!(before, budget.snapshot());
    private_write(
        &directory.join("accepted-replay-and-cold-reopen.json"),
        &json!({"status":"PASS","version":document.version,"sha256":document.sha256,"replay":repeated,"budgetsUnchanged":true}),
    );
    outcome["status"] = json!("COMPLETE_REAL_CANONICAL_APPROVAL");
    outcome["acceptedReplayAndColdReopen"] = json!(true);
    private_write(&directory.join("outcome.json"), &outcome);
    drop(evidence);
}

#[cfg(test)]
#[path = "canonical_provider_acceptance/guards.rs"]
mod guards;

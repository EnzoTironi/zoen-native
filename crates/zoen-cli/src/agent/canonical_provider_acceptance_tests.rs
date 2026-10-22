//! Explicit operator acceptance only. Ordinary tests exercise the guards without
//! providers, credentials, network, operator workers or shared application state.
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
impl Budget {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            deadline: Instant::now() + WALL,
            counts: Mutex::default(),
            evidence: None,
        })
    }
    fn with_evidence(directory: &Path) -> Arc<Self> {
        Arc::new(Self {
            deadline: Instant::now() + WALL,
            counts: Mutex::default(),
            evidence: Some(directory.join("provider-trace.json")),
        })
    }
    fn value(state: &Counts) -> Value {
        json!({"clefDispatches":state.clef,"codexTurnsStarted":state.codex,"refusedBeforeDelegation":state.refused,
            "trace":state.trace,"limits":{"clefDispatches":CLEF_CAP,"codexTurns":CODEX_CAP,"wallSeconds":WALL.as_secs()},
            "providerInternalRetriesAndBillingUnmeasured":true})
    }
    fn persist(&self, state: &Counts) {
        if let Some(path) = &self.evidence {
            private_write(path, &Self::value(state));
        }
    }
    fn reserve(&self, kind: &str) -> Result<usize, String> {
        let mut state = self.counts.lock().unwrap();
        let full = match kind {
            "clef" => state.clef >= CLEF_CAP,
            "codex" => state.codex >= CODEX_CAP,
            _ => true,
        };
        if Instant::now() >= self.deadline || full {
            state.refused += 1;
            self.persist(&state);
            return Err("synthetic acceptance dispatch/time budget exhausted".into());
        }
        match kind {
            "clef" => state.clef += 1,
            "codex" => state.codex += 1,
            _ => unreachable!(),
        }
        let index = state.trace.len();
        state.trace.push(json!({"kind":kind,"state":"started"}));
        // This is proof metadata, never production authority. A crash/timeout
        // retains the reservation before a delegate can receive a request.
        self.persist(&state);
        Ok(index)
    }
    fn note(&self, value: Value) {
        let mut state = self.counts.lock().unwrap();
        state.trace.push(value);
        self.persist(&state);
    }
    fn snapshot(&self) -> Value {
        let state = self.counts.lock().unwrap();
        Self::value(&state)
    }
    fn intact(&self) -> bool {
        self.counts.lock().unwrap().refused == 0 && Instant::now() < self.deadline
    }
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
impl Expected {
    fn read(&self) -> ToolCall {
        ToolCall {
            id: "synthetic-read".into(),
            name: crate::deckcraft::model_name("read_deckcraft", &self.item).unwrap(),
            arguments: json!({"artifactId":self.item}),
        }
    }
    fn write(&self, read: &ToolStep) -> Result<ToolCall, String> {
        if read.is_error
            || read.call.name != self.read().name
            || read.call.arguments != self.read().arguments
        {
            return Err("acceptance guard: verified-read-required".into());
        }
        let result: Value = serde_json::from_str(&read.result)
            .map_err(|_| "acceptance guard: invalid-read-result")?;
        if result["scope"]["artifactId"] != self.item {
            return Err("acceptance guard: read-artifact".into());
        }
        if result["engineRevision"] != zoen_agentd::deckcraft::ENGINE_REVISION {
            return Err("acceptance guard: read-engine".into());
        }
        if result["head"]["version"]
            .as_str()
            .and_then(|v| v.parse::<u32>().ok())
            != Some(self.version)
        {
            return Err("acceptance guard: read-version".into());
        }
        if result["head"]["sha256"] != self.head {
            return Err("acceptance guard: read-hash".into());
        }
        let slide = result["slides"][0]["id"]
            .as_u64()
            .filter(|id| *id <= u32::MAX as u64)
            .ok_or("acceptance guard: native-slide-id")?;
        Ok(ToolCall {
            id: "synthetic-write".into(),
            name: crate::deckcraft::model_name("deckcraft_slide_notes", &self.item).unwrap(),
            arguments: json!({"artifactId":self.item,"version":self.version,"sha256":self.head,"slideId":slide,"text":NOTES}),
        })
    }
    fn admit(
        &self,
        worker_binding: &str,
        steps: &[ToolStep],
        output: &BrainOut,
    ) -> Result<(), String> {
        if worker_binding != self.worker_binding {
            return Err("acceptance guard: worker-binding".into());
        }
        if steps.iter().any(|s| s.is_error) {
            return Err("acceptance guard: failed-step".into());
        }
        match (steps, output) {
            ([], BrainOut::CallTools(calls))
                if calls.len() == 1
                    && calls[0].name == self.read().name
                    && calls[0].arguments == self.read().arguments =>
            {
                Ok(())
            }
            ([read], BrainOut::CallTools(calls)) => {
                let expected = self.write(read)?;
                if calls.len() != 1 {
                    return Err("acceptance guard: write-call-count".into());
                }
                if calls[0].id == read.call.id || calls[0].name != expected.name {
                    return Err("acceptance guard: write-call-identity".into());
                }
                if calls[0].arguments != expected.arguments {
                    return Err("acceptance guard: write-arguments".into());
                }
                Ok(())
            }
            ([read, write], BrainOut::Say(text)) => {
                let expected = self.write(read)?;
                if text != REPLY
                    || write.call.id == read.call.id
                    || write.call.name != expected.name
                    || write.call.arguments != expected.arguments
                {
                    return Err("acceptance guard: final-reply-or-step".into());
                }
                Ok(())
            }
            _ => Err("acceptance guard: route".into()),
        }
    }
    fn shape(&self, worker_binding: &str, steps: &[ToolStep], output: &BrainOut) -> Value {
        let (kind, calls) = match output {
            BrainOut::CallTools(calls) => ("call", calls.as_slice()),
            BrainOut::Say(_) => ("say", &[][..]),
            BrainOut::Silence => ("quiet", &[][..]),
            BrainOut::CreateAgent { .. } => ("create-agent", &[][..]),
        };
        let read = steps
            .first()
            .and_then(|s| serde_json::from_str::<Value>(&s.result).ok());
        json!({"kind":kind,"callCount":calls.len(),"completedStepCount":steps.len(),
            "workerBindingMatches":worker_binding==self.worker_binding,
            "readFirstNativeSlideId":read.as_ref().and_then(|r|r["slides"][0]["id"].as_u64()),
            "calls":calls.iter().map(|call|json!({
                "readAliasMatches":call.name==self.read().name,
                "writeAliasMatches":call.name==crate::deckcraft::model_name("deckcraft_slide_notes",&self.item).unwrap(),
                "argumentKeys":call.arguments.as_object().map(|a|a.keys().filter(|k|["artifactId","version","sha256","slideId","text"].contains(&k.as_str())).cloned().collect::<Vec<_>>()),
                "argumentKeyCount":call.arguments.as_object().map(|a|a.len()),
                "artifactMatches":call.arguments["artifactId"]==self.item,
                "versionMatches":call.arguments["version"].as_u64()==Some(self.version as u64),
                "hashMatches":call.arguments["sha256"]==self.head,
                "nativeSlideId":call.arguments["slideId"].as_u64(),
                "textMatches":call.arguments["text"]==NOTES
            })).collect::<Vec<_>>()})
    }
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
mod guards {
    use super::*;
    struct Synthetic {
        calls: Arc<AtomicUsize>,
        fail: bool,
    }
    impl DecisionProvider for Synthetic {
        fn name(&self) -> String {
            "synthetic-no-io".into()
        }
        fn on_device(&self) -> bool {
            true
        }
        fn decide(&self, _: &DecisionRequest) -> Result<DecisionResponse, GateError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(GateError::Unavailable("synthetic".into()))
            } else {
                Ok(DecisionResponse::default())
            }
        }
    }
    fn request() -> DecisionRequest {
        DecisionRequest {
            input: "synthetic".into(),
            questions: vec![],
        }
    }
    fn provider(budget: Arc<Budget>, calls: Arc<AtomicUsize>, fail: bool) -> BoundedClef {
        BoundedClef {
            inner: Box::new(Synthetic { calls, fail }),
            budget,
        }
    }
    fn expected() -> Expected {
        Expected {
            item: "synthetic-deck".into(),
            version: 1,
            head: "11".repeat(32),
            worker_binding: "88".repeat(32),
        }
    }
    #[test]
    fn one_attempt_marker_refuses_reentry_before_any_provider_setup() {
        let path = std::env::temp_dir().join(types::new_id("canonical-provider-one-attempt"));
        std::fs::create_dir(&path).unwrap();
        mark_once(&path).unwrap();
        assert_eq!(
            mark_once(&path).unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn reservation_is_persisted_before_the_delegate_observes_the_request() {
        struct ObservesReservation(std::path::PathBuf);
        impl DecisionProvider for ObservesReservation {
            fn name(&self) -> String {
                "synthetic-observer".into()
            }
            fn on_device(&self) -> bool {
                true
            }
            fn decide(&self, _: &DecisionRequest) -> Result<DecisionResponse, GateError> {
                let saved: Value =
                    serde_json::from_slice(&std::fs::read(&self.0).unwrap()).unwrap();
                assert_eq!(saved["clefDispatches"], 1);
                assert_eq!(saved["trace"][0]["state"], "started");
                Ok(DecisionResponse::default())
            }
        }
        let path = std::env::temp_dir().join(types::new_id("canonical-provider-reservation"));
        std::fs::create_dir(&path).unwrap();
        let budget = Budget::with_evidence(&path);
        BoundedClef {
            inner: Box::new(ObservesReservation(path.join("provider-trace.json"))),
            budget,
        }
        .decide(&request())
        .unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
    #[tokio::test]
    async fn signed_fixture_transfer_uses_the_real_owner_command_and_recovers_the_same_approved_request(
    ) {
        let crate::deckcraft::tests::Fixture {
            path,
            client,
            vault,
            owner_vault,
            owner,
            source,
            base,
        } = crate::deckcraft::tests::fixture();
        let account = client.account().unwrap();
        let space = client.item(base.item_id.clone()).unwrap().space_id;
        let turn = client
            .agent_binary_turn(&base.item_id, &source, &owner, None, None)
            .unwrap();
        let inbound = client
            .timeline(space.clone())
            .unwrap()
            .into_iter()
            .find(|e| e.id == source)
            .unwrap();
        let mut definition = crate::deckcraft::tools()
            .into_iter()
            .find(|t| t.name == "read_deckcraft")
            .unwrap();
        definition.name = crate::deckcraft::model_name("read_deckcraft", &base.item_id).unwrap();
        let arguments = json!({"artifactId":base.item_id});
        let continuation = json!({"version":1,"policy_sha256":"77".repeat(32),
            "envelope":{"agent":account.identity_id,"device":account.device_id,"owner":owner,"space":space,"from":owner,"inbound_sha256":sha(text_of(&inbound).unwrap().as_bytes())},
            "call":{"id":"synthetic-owner-api-read","name":definition.name,"arguments":arguments},"definition":definition,
            "steps":[],"remaining":[],"round":0,"rule":"model:low-confidence"});
        let request = client
            .stage_tool_approval(
                &turn,
                client_core::AgentToolApprovalInput {
                    canonical_tool: "read_deckcraft".into(),
                    arguments,
                    worker_binding: "88".repeat(32),
                    continuation_json: continuation.to_string(),
                    gate_rule: "model:low-confidence".into(),
                },
            )
            .unwrap();
        drop(client);
        approve_as_owner(&path, owner_vault, &space, &request).await;
        let reopened = canonical_tool_approval_tests::reopen(&path, vault);
        let approved = reopened
            .requests()
            .into_iter()
            .find(|r| r.id == request.id)
            .unwrap();
        assert_eq!(approved.status, client_core::RequestStatus::Approved);
        assert_eq!(
            approved.tool_approval.unwrap().content_hash,
            request.tool_approval.unwrap().content_hash
        );
        assert_eq!(
            reopened
                .app_binary_read(base.item_id, None, None)
                .unwrap()
                .version,
            1
        );
        drop(reopened);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn clef_concurrent_reservation_refuses_before_a_fifth_delegate() {
        let budget = Budget::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(provider(budget.clone(), calls.clone(), false));
        let threads: Vec<_> = (0..12)
            .map(|_| {
                let p = provider.clone();
                std::thread::spawn(move || p.decide(&request()).is_ok())
            })
            .collect();
        assert_eq!(
            threads
                .into_iter()
                .map(|t| t.join().unwrap())
                .filter(|ok| *ok)
                .count(),
            4
        );
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        assert_eq!(budget.snapshot()["clefDispatches"], 4);
    }
    #[test]
    fn provider_failures_and_wrapper_recreation_never_refund_a_reserved_dispatch() {
        let budget = Budget::new();
        let calls = Arc::new(AtomicUsize::new(0));
        for _ in 0..5 {
            assert!(provider(budget.clone(), calls.clone(), true)
                .decide(&request())
                .is_err());
        }
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        assert_eq!(budget.snapshot()["refusedBeforeDelegation"], 1);
    }
    #[test]
    fn expired_deadline_refuses_both_kinds_before_delegation() {
        let budget = Budget {
            deadline: Instant::now(),
            counts: Mutex::default(),
            evidence: None,
        };
        assert!(budget.reserve("clef").is_err());
        assert!(budget.reserve("codex").is_err());
        assert_eq!(budget.snapshot()["clefDispatches"], 0);
        assert_eq!(budget.snapshot()["codexTurnsStarted"], 0);
    }
    #[tokio::test]
    async fn brain_failures_and_rewrapped_context_keep_one_cumulative_three_turn_budget() {
        let budget = Budget::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let definition = definition();
        let cx = TurnContext {
            agent: "synthetic",
            device: "synthetic",
            owner: "synthetic",
            definition: &definition,
        };
        let inbound = Turn {
            space: "synthetic".into(),
            from: "synthetic".into(),
            text: "synthetic".into(),
        };
        for _ in 0..4 {
            let counter = calls.clone();
            let inner = zoen_agentd::citizen::testing::ControlledToolBrain {
                evaluate: Box::new(move |_| {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err("synthetic failure".into())
                }),
            };
            assert!(
                guarded(Box::new(inner), &budget, expected(), "88".repeat(32))
                    .think(cx, &[], &inbound)
                    .await
                    .is_err()
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_eq!(budget.snapshot()["codexTurnsStarted"], 3);
    }
    #[test]
    fn model_route_guard_rejects_extra_calls_foreign_arguments_and_repeated_steps() {
        let expected = expected();
        let read = expected.read();
        assert!(expected
            .admit(
                &expected.worker_binding,
                &[],
                &BrainOut::CallTools(vec![read.clone()])
            )
            .is_ok());
        assert!(expected
            .admit(
                &expected.worker_binding,
                &[],
                &BrainOut::CallTools(vec![read.clone(), read.clone()])
            )
            .is_err());
        let mut foreign = read.clone();
        foreign.arguments["artifactId"] = json!("foreign");
        assert!(expected
            .admit(
                &expected.worker_binding,
                &[],
                &BrainOut::CallTools(vec![foreign])
            )
            .is_err());
        let steps = vec![verified_read(&expected)];
        assert!(expected
            .admit(
                &expected.worker_binding,
                &steps,
                &BrainOut::CallTools(vec![native_write(&expected, json!(257))])
            )
            .is_ok());
        assert!(expected
            .admit(
                &expected.worker_binding,
                &steps,
                &BrainOut::CallTools(vec![read])
            )
            .is_err());
        let mut stale = native_write(&expected, json!(257));
        stale.arguments["version"] = json!(2);
        assert!(expected
            .admit(
                &expected.worker_binding,
                &steps,
                &BrainOut::CallTools(vec![stale])
            )
            .is_err());
    }
    fn verified_read(expected: &Expected) -> ToolStep {
        ToolStep {
            call: expected.read(),
            result: json!({"engineRevision":zoen_agentd::deckcraft::ENGINE_REVISION,
                "scope":{"artifactId":expected.item},
                "head":{"version":expected.version.to_string(),"sha256":expected.head},
                "slides":[{"id":257}]})
            .to_string(),
            is_error: false,
        }
    }
    fn native_write(expected: &Expected, slide: Value) -> ToolCall {
        ToolCall {
            id: "independent-write-proposal".into(),
            name: crate::deckcraft::model_name("deckcraft_slide_notes", &expected.item).unwrap(),
            arguments: json!({"artifactId":expected.item,"version":expected.version,"sha256":expected.head,"slideId":slide,"text":NOTES}),
        }
    }
    #[test]
    fn native_id_guard_accepts_observed_257_and_rejects_positional_unknown_missing_or_unread_ids() {
        let expected = expected();
        let read = verified_read(&expected);
        let valid = native_write(&expected, json!(257));
        assert!(expected
            .admit(
                &expected.worker_binding,
                std::slice::from_ref(&read),
                &BrainOut::CallTools(vec![valid.clone()])
            )
            .is_ok());
        for id in [json!(1), json!(258), Value::Null, json!("257"), json!(-1)] {
            let proposal = native_write(&expected, id);
            assert!(expected
                .admit(
                    &expected.worker_binding,
                    std::slice::from_ref(&read),
                    &BrainOut::CallTools(vec![proposal])
                )
                .is_err());
        }
        let mut missing = valid.clone();
        missing.arguments.as_object_mut().unwrap().remove("slideId");
        assert!(expected
            .admit(
                &expected.worker_binding,
                std::slice::from_ref(&read),
                &BrainOut::CallTools(vec![missing])
            )
            .is_err());
        assert!(expected
            .admit(
                &expected.worker_binding,
                &[],
                &BrainOut::CallTools(vec![valid])
            )
            .is_err());
    }
    #[test]
    fn native_id_guard_rejects_unverified_head_artifact_engine_worker_and_read_shape() {
        let expected = expected();
        let read = verified_read(&expected);
        let valid = native_write(&expected, json!(257));
        for pointer in [
            "/head/sha256",
            "/head/version",
            "/scope/artifactId",
            "/engineRevision",
            "/slides/0/id",
        ] {
            let mut changed: Value = serde_json::from_str(&read.result).unwrap();
            *changed.pointer_mut(pointer).unwrap() = Value::Null;
            let mut step = read.clone();
            step.result = changed.to_string();
            assert!(
                expected
                    .admit(
                        &expected.worker_binding,
                        &[step],
                        &BrainOut::CallTools(vec![valid.clone()])
                    )
                    .is_err(),
                "{pointer}"
            );
        }
        for id in [json!(-1), json!(u64::MAX), json!(1.5), json!("257")] {
            let mut changed: Value = serde_json::from_str(&read.result).unwrap();
            changed["slides"][0]["id"] = id;
            let mut step = read.clone();
            step.result = changed.to_string();
            assert!(expected
                .admit(
                    &expected.worker_binding,
                    &[step],
                    &BrainOut::CallTools(vec![valid.clone()])
                )
                .is_err());
        }
        let mut changed = valid.clone();
        changed.arguments["sha256"] = json!("00".repeat(32));
        assert!(expected
            .admit(
                &expected.worker_binding,
                std::slice::from_ref(&read),
                &BrainOut::CallTools(vec![changed])
            )
            .is_err());
        assert!(expected
            .admit(
                &"99".repeat(32),
                std::slice::from_ref(&read),
                &BrainOut::CallTools(vec![valid])
            )
            .is_err());
    }
    #[tokio::test]
    async fn rejected_proposal_persists_only_reason_shape_flags_and_cumulative_counts() {
        let expected = expected();
        let read = verified_read(&expected);
        let mut candidate = native_write(&expected, json!(1));
        candidate.arguments["untrusted-key-DO_NOT_LOG"] = json!("untrusted-value-DO_NOT_LOG");
        let path = std::env::temp_dir().join(types::new_id("redacted-provider-rejection"));
        std::fs::create_dir(&path).unwrap();
        let budget = Budget::with_evidence(&path);
        let inner = zoen_agentd::citizen::testing::ControlledToolBrain {
            evaluate: Box::new(move |_| Ok(BrainOut::CallTools(vec![candidate.clone()]))),
        };
        let definition = definition();
        let cx = TurnContext {
            agent: "synthetic",
            device: "synthetic",
            owner: "synthetic",
            definition: &definition,
        };
        let inbound = Turn {
            space: "synthetic".into(),
            from: "synthetic".into(),
            text: "synthetic".into(),
        };
        let result = guarded(Box::new(inner), &budget, expected, "88".repeat(32))
            .think_with_tools(cx, &[], &inbound, &[], &[read])
            .await;
        assert_eq!(result, Err("acceptance guard: write-arguments".into()));
        let saved = std::fs::read_to_string(path.join("provider-trace.json")).unwrap();
        assert!(!saved.contains("DO_NOT_LOG"));
        let value: Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(value["codexTurnsStarted"], 1);
        assert_eq!(value["clefDispatches"], 0);
        assert_eq!(value["trace"][1]["state"], "rejected-synthetic-shape");
        assert_eq!(value["trace"][1]["shape"]["calls"][0]["nativeSlideId"], 1);
        assert_eq!(value["trace"][1]["shape"]["readFirstNativeSlideId"], 257);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "offline only; requires the actual operator-signed pinned WASI worker"]
    async fn native_slide_id_from_actual_signed_worker_is_admitted() {
        let worker = crate::deckcraft::configured()
            .unwrap()
            .expect("explicit signed worker required");
        let fixture = crate::deckcraft::tests::fixture();
        let result = crate::deckcraft::tests::inspect(&fixture, &worker).await;
        assert_eq!(result["slides"][0]["id"], json!(257));
        let expected = Expected {
            item: fixture.base.item_id.clone(),
            version: fixture.base.version,
            head: fixture.base.sha256.clone(),
            worker_binding: worker.binding_sha256().into(),
        };
        let read = ToolStep {
            call: expected.read(),
            result: result.to_string(),
            is_error: false,
        };
        let write = native_write(&expected, result["slides"][0]["id"].clone());
        let admitted = expected.admit(
            worker.binding_sha256(),
            &[read],
            &BrainOut::CallTools(vec![write]),
        );
        println!("actual signed worker read nativeSlideId=257; guarded single-write admission={admitted:?}; no provider dispatch");
        let path = fixture.path.clone();
        drop(fixture);
        std::fs::remove_dir_all(path).unwrap();
        assert!(
            admitted.is_ok(),
            "the verified native first-slide ID must be admitted: {admitted:?}"
        );
    }
}

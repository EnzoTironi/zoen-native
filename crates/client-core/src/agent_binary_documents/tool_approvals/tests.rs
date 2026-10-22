use super::*;
use crate::agent_binary_documents::tests::{counts, fixture, turn, Fixture, CHANGED, ORIGINAL};
use types::{Annotation, Grant, IdentityKind, Role};

#[path = "projection_tests.rs"]
mod projection;

const WORKER: &str = "4444444444444444444444444444444444444444444444444444444444444444";

fn owner_decide(
    fixture: &Fixture,
    request: &str,
    observed: &str,
    decision: ToolRequestDecision,
) -> R<DecideOutcome> {
    let mut engine = fixture.agent.lock();
    let agent = engine.me.replace(fixture.owner.clone());
    // Offline owner fixture signs locally with its retained synthetic root. Ingest
    // marks this Space relay-ordered; it must not publish as the agent's account.
    let synced = engine.net.synced.remove(&fixture.space);
    let result = engine.decide_tool_request(request, observed, decision);
    if synced {
        engine.net.synced.insert(fixture.space.clone());
    }
    engine.me = agent;
    result
}

fn signed_trust(fixture: &Fixture, level: TrustLevel) {
    let mut engine = fixture.agent.lock();
    let agent = engine.me_id().unwrap();
    engine
        .append(
            &fixture.space,
            &fixture.owner,
            EventBody::GrantIssued {
                grant: Grant {
                    id: types::new_id("synthetic_tool_trust"),
                    grantor: fixture.owner.clone(),
                    grantee: Some(agent),
                    scope: GrantScope::Space(fixture.space.clone()),
                    capability: Capability::Trust(level),
                    expires_at_ms: None,
                },
            },
        )
        .unwrap();
}

fn input(fixture: &Fixture, turn: &AgentBinaryTurn, tool: &str) -> AgentToolApprovalInput {
    let arguments = if tool == "read_deckcraft" {
        json!({"artifactId":fixture.base.item_id})
    } else {
        json!({"artifactId":fixture.base.item_id,"version":fixture.base.version,"sha256":fixture.base.sha256,"slideId":1,"text":"Synthetic reviewed notes"})
    };
    let definition = agent_tools::tools()
        .into_iter()
        .find(|definition| definition.name == tool)
        .unwrap();
    let alias = agent_tools::model_name(tool, &fixture.base.item_id).unwrap();
    let inbound = {
        let engine = fixture.agent.lock();
        let entry = engine.state.spaces[&fixture.space]
            .entries
            .iter()
            .find(|entry| entry.id() == fixture.source)
            .unwrap();
        let EntryBody::Message { text, .. } = &entry.body else {
            panic!("fixture source must be a real signed message");
        };
        sha(text.as_bytes())
    };
    let continuation_json = serde_json::to_string(&json!({
        "version":1,"policy_sha256":"77".repeat(32),
        "envelope":{"agent":turn.document.actor_id,"device":turn.device,"owner":turn.owner,"space":fixture.space,"from":fixture.owner,"inbound_sha256":inbound},
        "call":{"id":"synthetic-held-call","name":alias,"arguments":arguments},
        "definition":{"name":alias,"description":definition.description,"parameters":definition.parameters},
        "steps":[],"remaining":[],"round":0,"rule":"model:low-confidence"
    }))
    .unwrap();
    AgentToolApprovalInput {
        canonical_tool: tool.into(),
        arguments,
        worker_binding: WORKER.into(),
        continuation_json,
        gate_rule: "model:low-confidence".into(),
    }
}

fn stage(fixture: &Fixture, tool: &str) -> AgentRequestDto {
    let turn = turn(fixture);
    fixture
        .agent
        .stage_tool_approval(&turn, input(fixture, &turn, tool))
        .unwrap()
}

fn observed(request: &AgentRequestDto) -> &str {
    &request.tool_approval.as_ref().unwrap().content_hash
}

fn proposal(fixture: &Fixture, id: &str) -> ToolProposal {
    fixture.agent.lock().state.requests[id]
        .req
        .tool_proposal
        .as_deref()
        .unwrap()
        .clone()
}

fn intent(proposal: &ToolProposal) -> (String, String) {
    let (_, command) = parsed(proposal).unwrap();
    let digest = command_intent(proposal, command.as_ref()).unwrap();
    (format!("deckcmd_{digest}"), digest)
}

fn current(fixture: &Fixture) -> AppBinaryDocumentDto {
    fixture
        .agent
        .app_binary_read(fixture.base.item_id.clone(), None, None)
        .unwrap()
}

fn assert_v1(fixture: &Fixture) {
    let document = current(fixture);
    assert_eq!(document.version, 1);
    assert_eq!(document.sha256, fixture.base.sha256);
    assert_eq!(document.data_base64, STANDARD.encode(ORIGINAL));
}

#[test]
fn canonical_request_is_deduplicated_and_generic_or_wrong_owner_decisions_never_authorize() {
    let fixture = fixture();
    signed_trust(&fixture, TrustLevel::Suggest);
    let request = stage(&fixture, "deckcraft_slide_notes");
    assert_eq!(request.status, RequestStatus::Pending);
    assert!(!request.can_always_approve);
    assert!(request.item_id.is_none());
    assert_eq!(request.tool_approval.as_ref().unwrap().version, 1);
    let before = counts(&fixture);
    assert_eq!(stage(&fixture, "deckcraft_slide_notes"), request);
    assert_eq!(counts(&fixture), before);
    assert_eq!(
        fixture
            .agent
            .tool_approval_requests(&fixture.space, &fixture.source)
            .len(),
        1
    );
    for decision in [
        RequestDecision::Approve,
        RequestDecision::Deny,
        RequestDecision::AlwaysApprove,
        RequestDecision::AlwaysDeny,
    ] {
        let mut engine = fixture.agent.lock();
        let agent = engine.me.replace(fixture.owner.clone());
        assert!(engine.decide_request(&request.id, decision).is_err());
        engine.me = agent;
    }
    assert!(fixture
        .agent
        .lock()
        .decide_tool_request(
            &request.id,
            observed(&request),
            ToolRequestDecision::ApproveOnce
        )
        .is_err());
    assert!(owner_decide(
        &fixture,
        &request.id,
        &"00".repeat(32),
        ToolRequestDecision::ApproveOnce
    )
    .is_err());
    assert!(fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .is_err());
    assert_eq!(counts(&fixture), before);
    assert_v1(&fixture);

    let approved = owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    assert_eq!(approved.request.status, RequestStatus::Approved);
    assert!(approved.standing_grant_id.is_none());
    let before = counts(&fixture);
    assert_eq!(
        owner_decide(
            &fixture,
            &request.id,
            observed(&request),
            ToolRequestDecision::ApproveOnce
        )
        .unwrap(),
        approved
    );
    assert!(owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::Deny
    )
    .is_err());
    assert!(fixture
        .agent
        .approved_tool_turn(&request.id, &"55".repeat(32))
        .is_err());
    let admission = fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .unwrap();
    assert_eq!(admission.proposal().source, fixture.source);
    assert_eq!(
        admission.proposal().arguments_json,
        proposal(&fixture, &request.id).arguments_json
    );
    assert_eq!(admission.turn().document().version, 1);
    assert_eq!(counts(&fixture), before);
    assert_v1(&fixture);
}

#[test]
fn denial_and_cancellation_are_terminal_idempotent_without_tools_files_or_standing_grants() {
    for (decision, status) in [
        (ToolRequestDecision::Deny, RequestStatus::Denied),
        (ToolRequestDecision::Cancel, RequestStatus::Cancelled),
    ] {
        let fixture = fixture();
        let request = stage(&fixture, "read_deckcraft");
        let before = counts(&fixture);
        let outcome = owner_decide(&fixture, &request.id, observed(&request), decision).unwrap();
        assert_eq!(outcome.request.status, status);
        assert!(outcome.standing_grant_id.is_none());
        let after = counts(&fixture);
        assert_eq!(after[1..4], before[1..4]);
        assert_eq!(
            owner_decide(&fixture, &request.id, observed(&request), decision).unwrap(),
            outcome
        );
        assert!(owner_decide(
            &fixture,
            &request.id,
            observed(&request),
            ToolRequestDecision::ApproveOnce
        )
        .is_err());
        assert!(fixture
            .agent
            .approved_tool_turn(&request.id, WORKER)
            .is_err());
        assert_eq!(counts(&fixture), after);
        assert_v1(&fixture);
    }
}

#[test]
fn mutation_of_frozen_arguments_or_source_cannot_replace_the_reviewed_intent() {
    let fixture = fixture();
    signed_trust(&fixture, TrustLevel::Suggest);
    let original_turn = turn(&fixture);
    let request = stage(&fixture, "deckcraft_slide_notes");
    let before = counts(&fixture);
    let mut changed = input(&fixture, &original_turn, "deckcraft_slide_notes");
    changed.arguments["text"] = json!("Unreviewed replacement");
    let mut ask: Value = serde_json::from_str(&changed.continuation_json).unwrap();
    ask["call"]["arguments"] = changed.arguments.clone();
    changed.continuation_json = serde_json::to_string(&ask).unwrap();
    assert!(fixture
        .agent
        .stage_tool_approval(&original_turn, changed)
        .is_err());
    assert_eq!(counts(&fixture), before);
    fixture
        .agent
        .lock()
        .append(
            &fixture.space,
            &fixture.owner,
            EventBody::MessageAnnotated {
                target: fixture.source.clone(),
                note: Annotation::Edit {
                    text: "Changed signed instruction".into(),
                },
            },
        )
        .unwrap();
    let before = counts(&fixture);
    assert!(owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce
    )
    .is_err());
    assert!(fixture
        .agent
        .stage_tool_approval(
            &original_turn,
            input(&fixture, &original_turn, "deckcraft_slide_notes")
        )
        .is_err());
    assert!(fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .is_err());
    assert_eq!(counts(&fixture), before);
    assert_v1(&fixture);
    // Content changes do not prevent the owner from cancelling the exact pending request.
    assert_eq!(
        owner_decide(
            &fixture,
            &request.id,
            observed(&request),
            ToolRequestDecision::Cancel
        )
        .unwrap()
        .request
        .status,
        RequestStatus::Cancelled
    );
}

#[test]
fn changed_head_and_revoked_grants_or_device_refuse_before_execution() {
    for failure in [
        "head",
        "app-grant",
        "device",
        "served-writer",
        "trust-block",
    ] {
        let fixture = fixture();
        signed_trust(&fixture, TrustLevel::Suggest);
        let request = stage(&fixture, "deckcraft_slide_notes");
        owner_decide(
            &fixture,
            &request.id,
            observed(&request),
            ToolRequestDecision::ApproveOnce,
        )
        .unwrap();
        let admitted = fixture
            .agent
            .approved_tool_turn(&request.id, WORKER)
            .unwrap();
        let (mutation, intent) = intent(admitted.proposal());
        match failure {
            "head" => {
                fixture
                    .agent
                    .app_binary_save(
                        fixture.base.item_id.clone(),
                        1,
                        fixture.base.sha256.clone(),
                        STANDARD.encode(CHANGED),
                        "synthetic-independent-edit".into(),
                        "66".repeat(32),
                    )
                    .unwrap();
            }
            "app-grant" => {
                let mut engine = fixture.agent.lock();
                let grant = engine
                    .state
                    .grants
                    .iter()
                    .rev()
                    .find(|grant| {
                        grant.grant.scope == GrantScope::Item(fixture.base.item_id.clone())
                            && matches!(grant.grant.capability, Capability::Trust(TrustLevel::Act))
                    })
                    .unwrap()
                    .grant
                    .clone();
                engine
                    .append(
                        &fixture.space,
                        &grant.grantor,
                        EventBody::GrantRevoked { grant: grant.id },
                    )
                    .unwrap();
            }
            "device" => {
                let device = fixture.agent.account().unwrap().device_id;
                fixture
                    .agent
                    .lock()
                    .store
                    .set_meta("devices.revoked", &json!([device]).to_string())
                    .unwrap();
            }
            "served-writer" => {
                let mut engine = fixture.agent.lock();
                let agent = engine.me_id().unwrap();
                engine
                    .append(
                        &fixture.space,
                        &agent,
                        EventBody::MemberAdded {
                            identity: fixture.owner.clone(),
                            role: Role::Reader,
                        },
                    )
                    .unwrap();
            }
            "trust-block" => signed_trust(&fixture, TrustLevel::Listen),
            _ => unreachable!(),
        }
        let before = counts(&fixture);
        assert!(
            fixture
                .agent
                .approved_tool_turn(&request.id, WORKER)
                .is_err(),
            "{failure}"
        );
        assert!(
            fixture
                .agent
                .agent_binary_result(admitted.turn(), &mutation, &intent)
                .is_err(),
            "{failure}"
        );
        assert!(
            fixture
                .agent
                .agent_binary_save(
                    admitted.turn(),
                    &mutation,
                    &intent,
                    &STANDARD.encode(CHANGED)
                )
                .is_err(),
            "{failure}"
        );
        assert_eq!(counts(&fixture), before, "{failure}");
        assert_eq!(
            fixture.agent.lock().state.items[&fixture.base.item_id]
                .current()
                .number,
            if failure == "head" { 2 } else { 1 }
        );
    }
}

fn append_at(fixture: &Fixture, request: AgentRequest, at: i64) {
    let mut engine = fixture.agent.lock();
    let mut log = event_log::SpaceLog::from_events(
        &fixture.space,
        engine.logs[&fixture.space].events().to_vec(),
    )
    .unwrap();
    let author = engine.author_for(&request.agent).unwrap();
    let signed = author.sign_event(
        &fixture.space,
        &types::new_ulid(at),
        at,
        log.head(),
        EventBody::RequestOpened { request },
    );
    let event = log.sequence(signed).clone();
    let frame = protocol::Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env: protocol::Envelope::plain(&event),
    };
    assert_eq!(engine.ingest(frame), crate::sync::Ingest::Applied);
}

#[test]
fn expired_signed_request_never_approves_and_can_be_cancelled_without_a_clock_sleep() {
    let fixture = fixture();
    let original = stage(&fixture, "read_deckcraft");
    let mut expired = fixture.agent.lock().state.requests[&original.id]
        .req
        .clone();
    expired.id = types::new_id("synthetic_expired_ask");
    expired.tool_proposal.as_mut().unwrap().expires_at_ms = crate::engine::now_ms() - 1000;
    expired.content_hash = expired.tool_proposal_hash(&fixture.space).unwrap();
    let opened = expired.tool_proposal.as_ref().unwrap().expires_at_ms - 1000;
    let id = expired.id.clone();
    let hash = expired.content_hash.clone();
    append_at(&fixture, expired, opened);
    assert!(fixture.agent.lock().state.requests.contains_key(&id));
    let before = counts(&fixture);
    assert!(owner_decide(&fixture, &id, &hash, ToolRequestDecision::ApproveOnce).is_err());
    assert!(fixture.agent.approved_tool_turn(&id, WORKER).is_err());
    assert_eq!(counts(&fixture), before);
    assert_eq!(
        owner_decide(&fixture, &id, &hash, ToolRequestDecision::Cancel)
            .unwrap()
            .request
            .status,
        RequestStatus::Cancelled
    );
    assert_v1(&fixture);
}

#[test]
fn accepted_receipt_replays_once_and_wrong_mutation_or_intent_never_write() {
    let fixture = fixture();
    signed_trust(&fixture, TrustLevel::Suggest);
    let request = stage(&fixture, "deckcraft_slide_notes");
    owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    let approved = fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .unwrap();
    let (mutation, intent) = intent(approved.proposal());
    let before = counts(&fixture);
    let wrong_hash = "00".repeat(32);
    for (id, hash) in [
        ("unreviewed-mutation", intent.as_str()),
        (mutation.as_str(), wrong_hash.as_str()),
    ] {
        assert!(fixture
            .agent
            .agent_binary_result(approved.turn(), id, hash)
            .is_err());
        assert!(fixture
            .agent
            .agent_binary_save(approved.turn(), id, hash, &STANDARD.encode(CHANGED))
            .is_err());
    }
    assert_eq!(counts(&fixture), before);
    assert!(fixture
        .agent
        .agent_binary_result(approved.turn(), &mutation, &intent)
        .unwrap()
        .is_none());
    let accepted = fixture
        .agent
        .agent_binary_save(
            approved.turn(),
            &mutation,
            &intent,
            &STANDARD.encode(CHANGED),
        )
        .unwrap();
    assert_eq!(accepted.version, 2);
    assert_eq!(accepted.data_base64, STANDARD.encode(CHANGED));
    let before = counts(&fixture);
    assert_eq!(
        fixture
            .agent
            .agent_binary_result(approved.turn(), &mutation, &intent)
            .unwrap(),
        Some(accepted.clone())
    );
    assert_eq!(
        fixture
            .agent
            .agent_binary_save(
                approved.turn(),
                &mutation,
                &intent,
                &STANDARD.encode(CHANGED)
            )
            .unwrap(),
        accepted
    );
    let reopened_admission = fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .unwrap();
    assert_eq!(
        fixture
            .agent
            .agent_binary_result(reopened_admission.turn(), &mutation, &intent)
            .unwrap(),
        Some(accepted)
    );
    assert_eq!(counts(&fixture), before);
    assert_eq!(current(&fixture).version, 2);
}

#[test]
fn signed_grant_block_cannot_be_overridden_by_an_owner_approval_request() {
    let fixture = fixture();
    signed_trust(&fixture, TrustLevel::Listen);
    let turn = turn(&fixture);
    let before = counts(&fixture);
    assert!(fixture
        .agent
        .stage_tool_approval(&turn, input(&fixture, &turn, "deckcraft_slide_notes"))
        .is_err());
    assert_eq!(counts(&fixture), before);
    assert_v1(&fixture);
}

#[test]
fn canonical_approval_and_accepted_document_survive_a_cold_unlocked_reopen() {
    let fixture = fixture();
    signed_trust(&fixture, TrustLevel::Suggest);
    let request = stage(&fixture, "deckcraft_slide_notes");
    owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    let approved = fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .unwrap();
    let (mutation, intent) = intent(approved.proposal());
    let accepted = fixture
        .agent
        .agent_binary_save(
            approved.turn(),
            &mutation,
            &intent,
            &STANDARD.encode(CHANGED),
        )
        .unwrap();
    let directory = std::env::temp_dir().join(types::new_id("zoen_tool_approval_cold"));
    std::fs::create_dir(&directory).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    let path = directory.join("state.sqlite");
    fixture
        .agent
        .lock()
        .store
        .conn()
        .execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    let Fixture {
        agent,
        agent_vault,
        space,
        source,
        base,
        ..
    } = fixture;
    drop(agent);
    let client = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(client.unlock(agent_vault).unwrap());
    assert_eq!(client.account().unwrap().identity_id, accepted.actor_id);
    assert_eq!(
        client.lock().identities[&accepted.actor_id].kind,
        IdentityKind::Agent
    );
    let requests = client.tool_approval_requests(&space, &source);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].id, request.id);
    let admission = client.approved_tool_turn(&request.id, WORKER).unwrap();
    assert_eq!(admission.turn().document().version, 1);
    let before = client
        .lock()
        .store
        .conn()
        .query_row("SELECT count(*) FROM events", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();
    assert_eq!(
        client
            .agent_binary_result(admission.turn(), &mutation, &intent)
            .unwrap(),
        Some(accepted.clone())
    );
    assert_eq!(
        client
            .agent_binary_save(
                admission.turn(),
                &mutation,
                &intent,
                &STANDARD.encode(CHANGED)
            )
            .unwrap(),
        accepted
    );
    assert_eq!(
        client
            .app_binary_read(base.item_id, None, None)
            .unwrap()
            .version,
        2
    );
    assert_eq!(
        client
            .lock()
            .store
            .conn()
            .query_row("SELECT count(*) FROM events", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        before
    );
}

#[test]
fn read_only_approval_cannot_authorize_a_result_probe_or_save_even_with_its_exact_intent() {
    let fixture = fixture();
    let request = stage(&fixture, "read_deckcraft");
    owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    let approved = fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .unwrap();
    let (mutation, intent) = intent(approved.proposal());
    let before = counts(&fixture);
    fixture
        .agent
        .agent_binary_finish_read(approved.turn())
        .unwrap();
    assert!(fixture
        .agent
        .agent_binary_result(approved.turn(), &mutation, &intent)
        .is_err());
    assert!(fixture
        .agent
        .agent_binary_save(
            approved.turn(),
            &mutation,
            &intent,
            &STANDARD.encode(CHANGED)
        )
        .is_err());
    assert_eq!(counts(&fixture), before);
    assert_v1(&fixture);
}

#[test]
fn raw_worker_turn_cannot_bypass_a_later_signed_core_block_at_result_or_save_boundary() {
    let fixture = fixture();
    signed_trust(&fixture, TrustLevel::Suggest);
    let raw_turn = turn(&fixture);
    let hash = "aa".repeat(32);
    assert!(fixture
        .agent
        .agent_binary_result(&raw_turn, "synthetic-raw-write", &hash)
        .unwrap()
        .is_none());
    signed_trust(&fixture, TrustLevel::Listen);
    let before = counts(&fixture);
    assert!(fixture
        .agent
        .agent_binary_result(&raw_turn, "synthetic-raw-write", &hash)
        .is_err());
    assert!(fixture
        .agent
        .agent_binary_save(
            &raw_turn,
            "synthetic-raw-write",
            &hash,
            &STANDARD.encode(CHANGED)
        )
        .is_err());
    // Reading the same authorized artifact remains possible; the denied write adds nothing.
    fixture.agent.agent_binary_finish_read(&raw_turn).unwrap();
    assert_eq!(counts(&fixture), before);
    assert_v1(&fixture);
}

#[test]
fn completed_read_then_accepted_write_recovers_context_without_relaxing_read_cas_or_authorizing_new_effects(
) {
    let fixture = fixture();
    let request = stage(&fixture, "read_deckcraft");
    owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    let original = fixture
        .agent
        .tool_approval_requests(&fixture.space, &fixture.source)
        .pop()
        .unwrap();
    let proposal = original.tool_proposal.as_ref().unwrap();
    let original_ask: Value = serde_json::from_str(&proposal.continuation_json).unwrap();
    let cached=json!({"scope":{"artifactId":fixture.base.item_id,"actorId":original.agent,"actorKind":"agent"},"head":{"version":"1","sha256":fixture.base.sha256},"slides":[]}).to_string();
    let args = json!({"artifactId":fixture.base.item_id,"version":1,"sha256":fixture.base.sha256,"slideId":1,"text":"Synthetic reviewed notes"});
    let action = agent_tools::command("deckcraft_slide_notes", &args)
        .unwrap()
        .2
        .unwrap();
    let mut command = (**proposal).clone();
    command.canonical_tool = "deckcraft_slide_notes".into();
    let intent = command_intent(&command, Some(&action)).unwrap();
    let raw = turn(&fixture);
    let accepted = fixture
        .agent
        .agent_binary_save(
            &raw,
            &format!("deckcmd_{intent}"),
            &intent,
            &STANDARD.encode(CHANGED),
        )
        .unwrap();
    assert_eq!(accepted.version, 2);
    assert!(
        fixture
            .agent
            .approved_tool_turn(&request.id, WORKER)
            .is_err(),
        "a completed read must not become a fresh historical-head execution permit"
    );
    let steps = json!([
        {"call":original_ask["call"],"result":cached,"is_error":false},
        {"call":{"id":"prepared_synthetic_write","name":agent_tools::model_name("deckcraft_slide_notes",&fixture.base.item_id).unwrap(),"arguments":args},"result":"","is_error":false}
    ]);
    let before = counts(&fixture);
    let recovered: Value = serde_json::from_str(
        &fixture
            .agent
            .recover_tool_results(&request.id, WORKER, &steps.to_string())
            .unwrap(),
    )
    .unwrap();
    let result: Value = serde_json::from_str(recovered[1]["result"].as_str().unwrap()).unwrap();
    assert_eq!(result["version"], 2);
    assert_eq!(result["replayed"], true);
    assert_eq!(
        counts(&fixture),
        before,
        "completion recovery never appends events, files or grants"
    );
    let mut unaccepted = steps.clone();
    unaccepted[1]["call"]["arguments"]["text"] = json!("Different unaccepted command");
    assert!(fixture
        .agent
        .recover_tool_results(&request.id, WORKER, &unaccepted.to_string())
        .is_err());
    let mut other_source = fixture.agent.lock();
    other_source
        .append(
            &fixture.space,
            &fixture.owner,
            EventBody::MessageAnnotated {
                target: fixture.source.clone(),
                note: Annotation::Edit {
                    text: "Changed source".into(),
                },
            },
        )
        .unwrap();
    drop(other_source);
    assert!(fixture
        .agent
        .recover_tool_results(&request.id, WORKER, &steps.to_string())
        .is_err());
}

#[test]
fn generic_standing_decision_never_resolves_or_labels_a_pending_tool_request() {
    let fixture = fixture();
    let tool = stage(&fixture, "read_deckcraft");
    let mut engine = fixture.agent.lock();
    let agent = engine.me_id().unwrap();
    let generic = engine
        .open_request(
            &fixture.space,
            &agent,
            "Legacy request",
            "Exact legacy detail",
            "This conversation",
            ActionClass::Reversible,
            None,
        )
        .unwrap()
        .unwrap();
    let prior = engine.me.replace(fixture.owner.clone());
    engine
        .decide_request(&generic, RequestDecision::AlwaysApprove)
        .unwrap();
    engine.me = prior;
    let request = engine
        .requests()
        .into_iter()
        .find(|request| request.id == tool.id)
        .unwrap();
    assert_eq!(request.status, RequestStatus::Pending);
    assert!(!request.by_standing);
    assert!(!request.can_always_approve);
}

#[path = "json_order_tests.rs"]
mod json_order;

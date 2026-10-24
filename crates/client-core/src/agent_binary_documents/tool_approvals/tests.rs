use super::*;
use crate::agent_binary_documents::tests::{counts, fixture, turn, Fixture, CHANGED, ORIGINAL};
use types::{Annotation, Grant, GrantScope, IdentityKind, Role};

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

fn signed_trust(
    fixture: &Fixture,
    level: TrustLevel,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut engine = fixture.agent.lock();
    let agent = engine.me_id().unwrap();
    engine
        .append(
            &fixture.space,
            &fixture.owner,
            EventBody::GrantIssued {
                grant: Grant {
                    id: types::new_id("synthetic_tool_trust")?,
                    grantor: fixture.owner.clone(),
                    grantee: Some(agent),
                    scope: GrantScope::Space(fixture.space.clone()),
                    capability: Capability::Trust(level),
                    expires_at_ms: None,
                },
            },
        )
        .unwrap();

    Ok(())
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
    admission::canonical_request_is_deduplicated_and_generic_or_wrong_owner_decisions_never_authorize().unwrap()
}

#[test]
fn denial_and_cancellation_are_terminal_idempotent_without_tools_files_or_standing_grants() {
    decisions::denial_and_cancellation_are_terminal_idempotent_without_tools_files_or_standing_grants().unwrap()
}

#[test]
fn mutation_of_frozen_arguments_or_source_cannot_replace_the_reviewed_intent() {
    admission::mutation_of_frozen_arguments_or_source_cannot_replace_the_reviewed_intent().unwrap()
}

#[test]
fn changed_head_and_revoked_grants_or_device_refuse_before_execution() {
    admission::changed_head_and_revoked_grants_or_device_refuse_before_execution().unwrap()
}

pub(crate) fn append_at(
    fixture: &Fixture,
    request: AgentRequest,
    at: i64,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut engine = fixture.agent.lock();
    let mut log = event_log::SpaceLog::from_events(
        &fixture.space,
        engine.logs[&fixture.space].events().to_vec(),
    )
    .unwrap();
    let author = engine.author_for(&request.agent).unwrap();
    let signed = author.sign_event(
        &fixture.space,
        &types::new_ulid(at)?,
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

    Ok(())
}

#[test]
fn expired_signed_request_never_approves_and_can_be_cancelled_without_a_clock_sleep() {
    decisions::expired_signed_request_never_approves_and_can_be_cancelled_without_a_clock_sleep()
        .unwrap()
}

#[test]
fn accepted_receipt_replays_once_and_wrong_mutation_or_intent_never_write() {
    recovery::accepted_receipt_replays_once_and_wrong_mutation_or_intent_never_write().unwrap()
}

#[test]
fn signed_grant_block_cannot_be_overridden_by_an_owner_approval_request() {
    decisions::signed_grant_block_cannot_be_overridden_by_an_owner_approval_request().unwrap()
}

#[test]
fn canonical_approval_and_accepted_document_survive_a_cold_unlocked_reopen() {
    recovery::canonical_approval_and_accepted_document_survive_a_cold_unlocked_reopen().unwrap()
}

#[test]
fn read_only_approval_cannot_authorize_a_result_probe_or_save_even_with_its_exact_intent() {
    recovery::read_only_approval_cannot_authorize_a_result_probe_or_save_even_with_its_exact_intent(
    )
    .unwrap()
}

#[test]
fn raw_worker_turn_cannot_bypass_a_later_signed_core_block_at_result_or_save_boundary() {
    admission::raw_worker_turn_cannot_bypass_a_later_signed_core_block_at_result_or_save_boundary()
        .unwrap()
}

#[test]
fn completed_read_then_accepted_write_recovers_context_without_relaxing_read_cas_or_authorizing_new_effects(
) {
    recovery::completed_read_then_accepted_write_recovers_context_without_relaxing_read_cas_or_authorizing_new_effects().unwrap()
}

#[test]
fn generic_standing_decision_never_resolves_or_labels_a_pending_tool_request() {
    decisions::generic_standing_decision_never_resolves_or_labels_a_pending_tool_request().unwrap()
}

#[path = "json_order_tests.rs"]
mod json_order;

mod admission;
mod decisions;
mod recovery;

mod renewal;

mod trust_revocation;

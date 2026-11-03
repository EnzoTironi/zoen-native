//! Real signed requests and index reads; no model, network or user data.
use super::*;
use crate::{
    agent_binary_documents::{
        tests::{fixture, Fixture},
        tool_approvals::source_hash,
    },
    AgentRequestDto, AgentToolApprovalInput, ToolRequestDecision,
};
use serde_json::json;
use types::{Annotation, EventBody};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn input(f: &Fixture, args: Value) -> Result<AgentToolApprovalInput, Box<dyn std::error::Error>> {
    let e = f.agent.lock();
    let account = e.account().ok_or("account missing")?;
    let (_, inbound) =
        source_hash(&e.state, &f.space, &f.source, &f.owner).ok_or("source missing")?;
    Ok(AgentToolApprovalInput {
        canonical_tool: TOOL.into(), arguments: args.clone(), worker_binding: executor_binding(), gate_rule: "model:low-confidence".into(),
        continuation_json: json!({"version":1,"policy_sha256":"77".repeat(32),
            "envelope":{"agent":account.identity,"device":account.device,"owner":f.owner,"space":f.space,"from":f.owner,"inbound_sha256":inbound},
            "call":{"id":"history-call","name":TOOL,"arguments":args,"requires_review":true},
            "definition":{"name":TOOL},"steps":[],"remaining":[],"round":0,"rule":"model:low-confidence"}).to_string(),
    })
}
fn stage(f: &Fixture) -> Result<AgentRequestDto, Box<dyn std::error::Error>> {
    Ok(f.agent.stage_conversation_search_approval(
        &f.space,
        &f.source,
        &f.owner,
        input(f, json!({"query":"Update", "limit":1}))?,
    )?)
}
fn decide(f: &Fixture, request: &AgentRequestDto, decision: ToolRequestDecision) -> TestResult {
    let hash = &request
        .tool_approval
        .as_ref()
        .ok_or("approval missing")?
        .content_hash;
    let mut e = f.agent.lock();
    let previous = e.me.replace(f.owner.clone());
    let synced = e.net.synced.remove(&f.space);
    let result = e.decide_tool_request(&request.id, hash, decision);
    if synced {
        e.net.synced.insert(f.space.clone());
    }
    e.me = previous;
    let outcome = result?;
    assert!(outcome.standing_grant_id.is_none());
    Ok(())
}
fn steps(permit: &ApprovedConversationSearch, result: &str) -> Result<Value, serde_json::Error> {
    let ask: Value = serde_json::from_str(&permit.proposal().continuation_json)?;
    Ok(json!([{"call":ask["call"],"result":result,"is_error":false}]))
}

#[test]
fn pending_review_reads_nothing_then_owner_approves_exact_query_without_artifact() -> TestResult {
    let f = fixture()?;
    let request = stage(&f)?;
    let dto = request.tool_approval.as_ref().ok_or("approval missing")?;
    assert_eq!(dto.version, 0);
    assert!(dto.artifact_id.is_empty());
    assert_eq!(dto.preview_kind.as_deref(), Some("conversation"));
    assert_eq!(dto.artifact_title, "Worker contract");
    assert!(f.agent.approved_conversation_search(&request.id).is_err());
    assert_eq!(stage(&f)?.id, request.id, "staging is idempotent");
    let changed = input(&f, json!({"query":"different", "limit":1}))?;
    assert!(f
        .agent
        .stage_conversation_search_approval(&f.space, &f.source, &f.owner, changed)
        .is_err());
    assert!(
        f.agent
            .decide_tool_request(
                request.id.clone(),
                dto.content_hash.clone(),
                ToolRequestDecision::ApproveOnce
            )
            .is_err(),
        "agent cannot approve as owner"
    );
    decide(&f, &request, ToolRequestDecision::ApproveOnce)?;
    let permit = f.agent.approved_conversation_search(&request.id)?;
    let before = f.agent.lock().store.outbox()?.len();
    let encoded = f.agent.execute_conversation_search(&permit)?;
    let page: crate::MessageSearchPage = serde_json::from_str(&encoded)?;
    assert_eq!(page.hits.len(), 1);
    assert_eq!(page.hits[0].ref_id, f.source);
    assert_eq!(f.agent.lock().store.outbox()?.len(), before);
    assert!(f.owner_client.execute_conversation_search(&permit).is_err());
    Ok(())
}

#[test]
fn malformed_or_substituted_continuations_cannot_open_review() -> TestResult {
    let f = fixture()?;
    for args in [
        json!({"query":""}),
        json!({"query":"x","space":"other"}),
        json!({"query":"x","limit":21}),
        json!({"query":"x","cursor":"x".repeat(4097)}),
    ] {
        assert!(f
            .agent
            .stage_conversation_search_approval(&f.space, &f.source, &f.owner, input(&f, args)?)
            .is_err());
    }
    for field in [
        "agent",
        "device",
        "owner",
        "space",
        "from",
        "inbound_sha256",
    ] {
        let mut call = input(&f, json!({"query":"Update"}))?;
        let mut ask: Value = serde_json::from_str(&call.continuation_json)?;
        ask["envelope"][field] = "substituted".into();
        call.continuation_json = ask.to_string();
        assert!(
            f.agent
                .stage_conversation_search_approval(&f.space, &f.source, &f.owner, call)
                .is_err(),
            "{field}"
        );
    }
    assert!(f
        .agent
        .tool_approval_requests(&f.space, &f.source)
        .is_empty());
    Ok(())
}

#[test]
fn deny_and_cancel_never_become_read_permission() -> TestResult {
    for decision in [ToolRequestDecision::Deny, ToolRequestDecision::Cancel] {
        let f = fixture()?;
        let request = stage(&f)?;
        decide(&f, &request, decision)?;
        assert!(f.agent.approved_conversation_search(&request.id).is_err());
        assert!(decide(&f, &request, ToolRequestDecision::ApproveOnce).is_err());
    }
    Ok(())
}

#[test]
fn recovery_requeries_history_and_rejects_call_query_or_cursor_substitution() -> TestResult {
    let f = fixture()?;
    let message = f
        .agent
        .lock()
        .append(
            &f.space,
            &f.owner,
            EventBody::MessagePosted {
                message: types::new_id("history")?,
                text: "Update old detail".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
                forwarded_author: None,
            },
        )?
        .client_id;
    let request = stage(&f)?;
    decide(&f, &request, ToolRequestDecision::ApproveOnce)?;
    let permit = f.agent.approved_conversation_search(&request.id)?;
    let old = f.agent.execute_conversation_search(&permit)?;
    let original = steps(&permit, &old)?;
    for (key, value) in [
        ("query", json!("another query")),
        ("limit", json!(2)),
        ("cursor", json!("another-page")),
    ] {
        let mut forged = original.clone();
        forged[0]["call"]["arguments"][key] = value;
        assert!(f
            .agent
            .recover_conversation_search_results(&request.id, &forged.to_string())
            .is_err());
    }
    f.agent.lock().append(
        &f.space,
        &f.owner,
        EventBody::MessageAnnotated {
            target: message,
            note: Annotation::Edit {
                text: "No longer matches".into(),
            },
        },
    )?;
    let recovered: Value = serde_json::from_str(
        &f.agent
            .recover_conversation_search_results(&request.id, &original.to_string())?,
    )?;
    let now = f.agent.execute_conversation_search(&permit)?;
    assert_eq!(recovered[0]["result"], now);
    assert_ne!(old, now, "recovery must not trust retained plaintext");
    Ok(())
}

#[test]
fn source_edits_and_revoked_membership_invalidate_existing_permits() -> TestResult {
    for change in [0, 1, 2] {
        let f = fixture()?;
        let request = stage(&f)?;
        decide(&f, &request, ToolRequestDecision::ApproveOnce)?;
        let permit = f.agent.approved_conversation_search(&request.id)?;
        let body = match change {
            0 => EventBody::MessageAnnotated {
                target: f.source.clone(),
                note: Annotation::Edit {
                    text: "Different task".into(),
                },
            },
            1 => EventBody::MessageAnnotated {
                target: f.source.clone(),
                note: Annotation::Delete,
            },
            _ => EventBody::MemberRemoved {
                identity: f.owner.clone(),
            },
        };
        f.agent.lock().append(&f.space, &f.owner, body)?;
        assert!(f.agent.execute_conversation_search(&permit).is_err());
        assert!(f
            .agent
            .recover_conversation_search_results(
                &request.id,
                &steps(&permit, "cached")?.to_string()
            )
            .is_err());
    }
    Ok(())
}

#[test]
fn expired_review_requires_explicit_owner_intent_and_new_approval() -> TestResult {
    use crate::agent_binary_documents::tests::renewal_history::{
        confirm_pending, expired_approved,
    };
    let f = fixture()?;
    let old = expired_approved(&f, stage(&f)?)?;
    assert!(f.agent.approved_conversation_search(&old.id).is_err());
    decide(&f, &old, ToolRequestDecision::RequestReview)?;
    let intent = f
        .agent
        .tool_review_pending_intent(&old.id)
        .ok_or("intent missing")?;
    let next = f
        .agent
        .stage_tool_review_successor(&old.id, &intent)?
        .ok_or("successor missing")?;
    assert!(f.agent.approved_conversation_search(&next.id).is_err());
    confirm_pending(&f)?;
    decide(&f, &next, ToolRequestDecision::ApproveOnce)?;
    let permit = f.agent.approved_conversation_search(&next.id)?;
    assert!(!f.agent.execute_conversation_search(&permit)?.is_empty());
    assert!(f.agent.approved_conversation_search(&old.id).is_err());
    Ok(())
}

#[test]
fn multiple_reviewed_reads_recover_with_separate_exact_approvals() -> TestResult {
    let f = fixture()?;
    let first = stage(&f)?;
    decide(&f, &first, ToolRequestDecision::ApproveOnce)?;
    let first_permit = f.agent.approved_conversation_search(&first.id)?;
    let first_result = f.agent.execute_conversation_search(&first_permit)?;
    let prefix = steps(&first_permit, &first_result)?;
    let mut second = input(&f, json!({"query":"missing", "limit":1}))?;
    let mut ask: Value = serde_json::from_str(&second.continuation_json)?;
    ask["steps"] = prefix.clone();
    ask["call"]["id"] = "second-search".into();
    ask["round"] = 1.into();
    second.continuation_json = ask.to_string();
    let second = f
        .agent
        .stage_conversation_search_approval(&f.space, &f.source, &f.owner, second)?;
    decide(&f, &second, ToolRequestDecision::ApproveOnce)?;
    let permit = f.agent.approved_conversation_search(&second.id)?;
    let mut all = prefix.as_array().ok_or("prefix missing")?.clone();
    all.push(json!({"call":ask["call"],"result":"stale cache", "is_error":false}));
    let recovered: Value = serde_json::from_str(
        &f.agent
            .recover_conversation_search_results(&second.id, &serde_json::to_string(&all)?)?,
    )?;
    assert_eq!(recovered[0]["result"], first_result);
    assert_eq!(
        recovered[1]["result"],
        f.agent.execute_conversation_search(&permit)?
    );
    all.push(json!({"call":{"id":"unapproved","name":TOOL,"arguments":{"query":"private"},"requires_review":true},"result":"forged", "is_error":false}));
    assert!(f
        .agent
        .recover_conversation_search_results(&second.id, &serde_json::to_string(&all)?)
        .is_err());
    Ok(())
}

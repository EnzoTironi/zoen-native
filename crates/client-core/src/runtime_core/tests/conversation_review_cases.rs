//! Same canonical request and signed owner decision, through native MLS custody.
use super::*;
use crate::{AgentToolApprovalInput, RequestStatus, ToolRequestDecision};
use serde_json::{json, Value};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

fn input(
    f: &Fixture,
    original: &ReplyIntent,
    args: Value,
) -> std::result::Result<AgentToolApprovalInput, Box<dyn std::error::Error>> {
    let source = f
        .core
        .engine
        .logs
        .get(SPACE)
        .and_then(|log| {
            log.events()
                .iter()
                .find(|event| event.hash == original.trigger())
                .map(|event| event.client_id.clone())
        })
        .ok_or("source missing")?;
    let (_, inbound) = crate::agent_binary_documents::tool_approvals::source_hash(
        &f.core.engine.state,
        SPACE,
        &source,
        &f.owner_profile.id,
    )
    .ok_or("source hash missing")?;
    let name = crate::agent_conversation_tools::TOOL;
    Ok(AgentToolApprovalInput {
        canonical_tool: name.into(), arguments: args.clone(),
        worker_binding: crate::agent_conversation_tools::executor_binding(),
        gate_rule: "model:low-confidence".into(),
        continuation_json: json!({"version":1,"policy_sha256":"77".repeat(32),
            "envelope":{"agent":f.agent_profile.id,"device":f.agent_device.id(),"owner":f.owner_profile.id,"space":SPACE,"from":f.owner_profile.id,"inbound_sha256":inbound},
            "call":{"id":"retained-search-call","name":name,"arguments":args,"requires_review":true},
            "definition":{"name":name},"steps":[],"remaining":[],"round":0,"rule":"model:low-confidence"}).to_string(),
    })
}

fn setup() -> std::result::Result<(Fixture, ReplyIntent, i64), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    f.grant("reviewed-history", None)?;
    f.message("A needle from older history")?;
    let trigger = f.message("Find our earlier decision")?;
    let now = crate::engine::now_ms();
    let original = f.core.reply(SPACE, &trigger, now)?.intent().clone();
    Ok((f, original, now))
}

fn confirm(f: &mut Fixture, envelope: &Envelope) -> TestResult {
    let frame = ordered(&f.owner, envelope.clone())?;
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Applied);
    f.core
        .ingest(vec![frame])
        .map_err(|error| format!("native canonical echo: {error}"))?;
    Ok(())
}

fn decide(f: &mut Fixture, request: &str, hash: &str, decision: ToolRequestDecision) -> TestResult {
    f.owner.decide_tool_request(request, hash, decision)?;
    f.owner.mls_seal_outbox()?;
    let envelopes = f.owner.outbox_envelopes();
    assert_eq!(envelopes.len(), 1);
    let frame = ordered(&f.owner, envelopes[0].clone())?;
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Confirmed);
    f.core
        .ingest(vec![frame])
        .map_err(|error| format!("native canonical echo: {error}"))?;
    Ok(())
}

#[test]
fn native_conversation_review_cold_retry_and_signed_owner_approval() -> TestResult {
    let (mut f, original, now) = setup()?;
    let args = json!({"query":"needle","limit":1});
    let prepared =
        f.core
            .stage_reply_search_review(&original, input(&f, &original, args.clone())?, now)?;
    let request = prepared.request().id.clone();
    let tool = prepared
        .request()
        .tool_approval
        .as_ref()
        .ok_or("tool review")?;
    assert_eq!(tool.preview_kind.as_deref(), Some("conversation"));
    assert!(tool.artifact_id.is_empty());
    let hash = tool.content_hash.clone();
    let envelope = prepared.envelope().ok_or("original wire")?.clone();
    f.core = DeviceCore::restore(
        NativeImage::decode_bounded(prepared.image().as_bytes().to_vec())?,
        f.credential(),
    )?;
    assert!(DeviceCore::restore(f.core.image()?, f.credential())?
        .approved_reply_history(&original, &request, &hash, &[], &args, now,)
        .is_err());
    let before = f.core.image()?;
    let repeated = f.core.stage_reply_search_review(
        &original,
        input(&f, &original, args.clone())?,
        now + 1,
    )?;
    assert_eq!(repeated.request().id, request);
    assert_eq!(repeated.envelope(), Some(&envelope));
    assert_eq!(repeated.image().as_bytes(), before.as_bytes());
    assert_eq!(f.core.engine.store.outbox_len()?, 1);
    f.core
        .reply_search_review_ready(&original, &request, &hash, &envelope, now)?;
    for (wrong_hash, wire) in [
        (&"0".repeat(64), envelope.clone()),
        (&hash, {
            let mut wire = envelope.clone();
            wire.sig = "0".repeat(128);
            wire
        }),
    ] {
        assert!(f
            .core
            .reply_search_review_ready(&original, &request, wrong_hash, &wire, now)
            .is_err());
    }
    confirm(&mut f, &envelope)?;
    f.core.refresh(&original, now)?;
    assert!(
        DeviceCore::restore(f.core.image()?, f.credential())?
            .approved_reply_history(&original, &request, &hash, &[], &args, now,)
            .is_err(),
        "a confirmed request is not owner consent"
    );
    assert_eq!(f.owner.requests()[0].id, request);
    assert!(f
        .core
        .reply_search_review_ready(&original, &request, &hash, &envelope, now)
        .is_err());
    decide(&mut f, &request, &hash, ToolRequestDecision::ApproveOnce)?;
    f.core = DeviceCore::restore(f.core.image()?, f.credential())?;
    assert_eq!(
        f.core
            .reply_search_review_status(&original, &request, &hash)?
            .status,
        RequestStatus::Approved
    );
    let page = DeviceCore::restore(f.core.image()?, f.credential())?.approved_reply_history(
        &original,
        &request,
        &hash,
        &[],
        &args,
        now + 2,
    )?;
    assert_eq!(page.hits.len(), 1);
    assert!(DeviceCore::restore(f.core.image()?, f.credential())?
        .approved_reply_history(
            &original,
            &request,
            &hash,
            &[],
            &json!({"query":"different","limit":1}),
            now + 2,
        )
        .is_err());
    let observed =
        f.core
            .stage_reply_search_review(&original, input(&f, &original, args)?, now + 3)?;
    assert_eq!(observed.request().id, request);
    assert!(observed.envelope().is_none());
    assert_eq!(f.core.engine.store.outbox_len()?, 0);
    Ok(())
}

#[test]
fn native_conversation_review_refusal_cancellation_expiry_and_revocation() -> TestResult {
    for change in ["deny", "cancel", "expiry", "revoke", "replace"] {
        let (mut f, original, now) = setup()?;
        let args = json!({"query":"needle"});
        let prepared = f.core.stage_reply_search_review(
            &original,
            input(&f, &original, args.clone())?,
            now,
        )?;
        let request = prepared.request().id.clone();
        let hash = prepared
            .request()
            .tool_approval
            .as_ref()
            .ok_or("review")?
            .content_hash
            .clone();
        confirm(&mut f, prepared.envelope().ok_or("wire")?)?;
        decide(
            &mut f,
            &request,
            &hash,
            match change {
                "deny" => ToolRequestDecision::Deny,
                "cancel" => ToolRequestDecision::Cancel,
                _ => ToolRequestDecision::ApproveOnce,
            },
        )?;
        if change == "revoke" {
            f.encrypted(
                EventBody::GrantRevoked {
                    grant: "reviewed-history".into(),
                },
                now + 1,
            )?;
        } else if change == "replace" {
            f.grant("replacement-review-grant", None)?;
        }
        let observed = if change == "expiry" {
            now + 15 * 60 * 1000
        } else {
            now + 2
        };
        let before = f.core.image()?;
        assert!(
            DeviceCore::restore(f.core.image()?, f.credential())?
                .approved_reply_history(&original, &request, &hash, &[], &args, observed,)
                .is_err(),
            "{change}"
        );
        assert_eq!(
            f.core
                .reply_search_review_status(&original, &request, &hash)?
                .id,
            request
        );
        assert_eq!(f.core.image()?.as_bytes(), before.as_bytes());
        assert_eq!(f.core.engine.store.outbox_len()?, 0);
    }
    Ok(())
}

#[test]
fn native_conversation_review_rejects_changed_binding_and_unrelated_outbox() -> TestResult {
    let (mut f, original, now) = setup()?;
    let args = json!({"query":"needle"});
    let before = f.core.image()?;
    for field in [
        "agent",
        "device",
        "owner",
        "space",
        "from",
        "inbound_sha256",
    ] {
        let mut ask = input(&f, &original, args.clone())?;
        let mut value: Value = serde_json::from_str(&ask.continuation_json)?;
        value["envelope"][field] = "substituted".into();
        ask.continuation_json = value.to_string();
        let mut scratch = DeviceCore::restore(f.core.image()?, f.credential())?;
        assert!(
            scratch
                .stage_reply_search_review(&original, ask, now)
                .is_err(),
            "{field}"
        );
        assert!(
            scratch.image().is_err(),
            "failed staging discards scratch view"
        );
    }
    assert_eq!(f.core.image()?.as_bytes(), before.as_bytes());
    let prepared = f
        .core
        .prepare_reply(&original, "Already retained output", now)?;
    let pending = prepared.envelope().clone();
    let before = f.core.image()?;
    let mut scratch = DeviceCore::restore(f.core.image()?, f.credential())?;
    assert!(scratch
        .stage_reply_search_review(&original, input(&f, &original, args)?, now)
        .is_err());
    assert!(scratch.image().is_err());
    assert_eq!(f.core.engine.outbox_envelopes(), vec![pending]);
    assert_eq!(f.core.image()?.as_bytes(), before.as_bytes());
    Ok(())
}

#[test]
fn native_conversation_review_pages_keep_each_original_owner_decision() -> TestResult {
    let (mut f, original, now) = setup()?;
    f.message("Another needle outside the first page")?;
    let args = json!({"query":"needle","limit":1});
    let first =
        f.core
            .stage_reply_search_review(&original, input(&f, &original, args.clone())?, now)?;
    let request = first.request().id.clone();
    let hash = first
        .request()
        .tool_approval
        .as_ref()
        .ok_or("review")?
        .content_hash
        .clone();
    confirm(&mut f, first.envelope().ok_or("wire")?)?;
    decide(&mut f, &request, &hash, ToolRequestDecision::ApproveOnce)?;
    let page = DeviceCore::restore(f.core.image()?, f.credential())?.approved_reply_history(
        &original,
        &request,
        &hash,
        &[],
        &args,
        now,
    )?;
    let cursor = page.next_cursor.clone().ok_or("another page")?;
    let mut reads = vec![ReviewedReplyHistory {
        request: request.clone(),
        content_hash: hash,
        read: ReplyHistoryRead {
            arguments: args,
            page,
        },
    }];
    let args = json!({"query":"needle","limit":1,"cursor":cursor});
    let mut ask = input(&f, &original, args.clone())?;
    let mut continuation: Value = serde_json::from_str(&ask.continuation_json)?;
    continuation["steps"] = json!([{"call":{"id":"retained-search-call","name":crate::agent_conversation_tools::TOOL,"arguments":reads[0].read.arguments},
        "result":serde_json::to_string(&reads[0].read.page)?,"is_error":false}]);
    continuation["call"]["id"] = "retained-page-call".into();
    continuation["round"] = 1.into();
    ask.continuation_json = continuation.to_string();
    let second = f.core.stage_reply_search_review(&original, ask, now + 1)?;
    assert_ne!(second.request().id, request);
    let request = second.request().id.clone();
    let hash = second
        .request()
        .tool_approval
        .as_ref()
        .ok_or("review")?
        .content_hash
        .clone();
    confirm(&mut f, second.envelope().ok_or("wire")?)?;
    assert!(
        DeviceCore::restore(f.core.image()?, f.credential())?
            .approved_reply_history(&original, &request, &hash, &reads, &args, now + 1,)
            .is_err(),
        "approval for the first page cannot approve the next page"
    );
    decide(&mut f, &request, &hash, ToolRequestDecision::ApproveOnce)?;
    let next = DeviceCore::restore(f.core.image()?, f.credential())?.approved_reply_history(
        &original,
        &request,
        &hash,
        &reads,
        &args,
        now + 1,
    )?;
    assert_eq!(next.hits.len(), 1);
    assert_ne!(next.hits[0].ref_id, reads[0].read.page.hits[0].ref_id);
    reads.push(ReviewedReplyHistory {
        request,
        content_hash: hash,
        read: ReplyHistoryRead {
            arguments: args,
            page: next,
        },
    });
    DeviceCore::restore(f.core.image()?, f.credential())?.verify_approved_reply_history(
        &original,
        &reads,
        now + 2,
    )?;
    assert!(
        DeviceCore::restore(f.core.image()?, f.credential())?
            .verify_approved_reply_history(&original, &reads, now + 15 * 60 * 1000,)
            .is_err(),
        "a later approval does not renew expired earlier consent"
    );
    reads[0].content_hash = "00".repeat(32);
    assert!(DeviceCore::restore(f.core.image()?, f.credential())?
        .verify_approved_reply_history(&original, &reads, now + 2)
        .is_err());
    Ok(())
}

#[test]
fn captured_calls_keep_independent_review_identities_and_original_wire() -> TestResult {
    let (mut f, original, now) = setup()?;
    let args = json!({"query":"needle","limit":1});
    let mut requests = Vec::new();
    for index in 0..3 {
        let mut ask = input(&f, &original, args.clone())?;
        let mut value: Value = serde_json::from_str(&ask.continuation_json)?;
        value["version"] = 2.into();
        value["native"] =
            json!({"original":"aa".repeat(32),"capture":"bb".repeat(32),"call":index});
        ask.continuation_json = value.to_string();
        let mut repeated = input(&f, &original, args.clone())?;
        repeated
            .continuation_json
            .clone_from(&ask.continuation_json);
        let prepared = f.core.stage_reply_search_review(&original, ask, now)?;
        let request = prepared.request().id.clone();
        let hash = prepared
            .request()
            .tool_approval
            .as_ref()
            .ok_or("review")?
            .content_hash
            .clone();
        let wire = prepared.envelope().ok_or("wire")?.clone();
        assert_eq!(
            f.core
                .retained_reply_search_review(&original, &request, &hash, wire.client_id())?,
            wire
        );
        assert!(f
            .core
            .accepted_reply_search_review(&original, &request, &hash, wire.client_id())?
            .is_none());
        f.core
            .reply_search_review_bound(&original, &request, &hash, &value["native"])?;
        let mut wrong = value["native"].clone();
        wrong["capture"] = "cc".repeat(32).into();
        assert!(f
            .core
            .reply_search_review_bound(&original, &request, &hash, &wrong)
            .is_err());
        confirm(&mut f, &wire)?;
        assert!(f
            .core
            .accepted_reply_search_review(&original, &request, &hash, wire.client_id())?
            .is_some());
        assert_eq!(
            f.core
                .retained_reply_search_review(&original, &request, &hash, wire.client_id())?,
            wire
        );
        f.core = DeviceCore::restore(f.core.image()?, f.credential())?;
        assert_eq!(
            f.core
                .stage_reply_search_review(&original, repeated, now)?
                .request()
                .id,
            request
        );
        requests.push((request, hash, wire));
    }
    assert_ne!(requests[0].0, requests[1].0);
    assert_ne!(requests[1].0, requests[2].0);
    decide(
        &mut f,
        &requests[0].0,
        &requests[0].1,
        ToolRequestDecision::ApproveOnce,
    )?;
    f.core
        .reply_search_review_approved(&original, &requests[0].0, &requests[0].1, &args, now)?;
    for (request, hash, _) in &requests[1..] {
        assert!(f
            .core
            .reply_search_review_approved(&original, request, hash, &args, now)
            .is_err());
    }
    f.encrypted(
        EventBody::GrantRevoked {
            grant: "reviewed-history".into(),
        },
        now + 1,
    )?;
    let (request, hash, wire) = &requests[0];
    assert!(f
        .core
        .reply_search_review_approved(&original, request, hash, &args, now + 1)
        .is_err());
    assert_eq!(
        f.core
            .retained_reply_search_review(&original, request, hash, wire.client_id())?,
        *wire
    );
    assert!(f
        .core
        .retained_reply_search_review(&original, request, hash, requests[1].2.client_id())
        .is_err());
    Ok(())
}

#[test]
fn captured_review_rejects_invalid_call_binding_without_staging() -> TestResult {
    let (f, original, now) = setup()?;
    for native in [
        json!({"original":"aa".repeat(32),"capture":"bb".repeat(32),"call":3}),
        json!({"original":"bad","capture":"bb".repeat(32),"call":0}),
        json!({"original":"aa".repeat(32),"capture":"bb".repeat(32),"call":0,"extra":true}),
    ] {
        let mut ask = input(&f, &original, json!({"query":"needle"}))?;
        let mut value: Value = serde_json::from_str(&ask.continuation_json)?;
        value["version"] = 2.into();
        value["native"] = native;
        ask.continuation_json = value.to_string();
        let mut scratch = DeviceCore::restore(f.core.image()?, f.credential())?;
        assert!(scratch
            .stage_reply_search_review(&original, ask, now)
            .is_err());
    }
    assert_eq!(f.core.engine.store.outbox_len()?, 0);
    Ok(())
}

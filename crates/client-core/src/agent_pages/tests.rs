//! Actual signed owner/Agent, native Page and canonical Request fixtures.
use super::*;
use crate::{
    agent_binary_documents::tests::{fixture as agent_fixture, Fixture},
    agent_page_tools as tools, AgentToolApprovalInput, ToolRequestDecision,
};
use serde_json::{json, Value};
use types::{Capability, EventBody, Grant, GrantScope, TrustLevel};

mod continuation;
mod contracts;
mod recovery;

struct PageFixture {
    actor: Fixture,
    page: String,
}
fn fixture() -> PageFixture {
    let actor = agent_fixture();
    let page = actor
        .agent
        .page_import_markdown(
            actor.space.clone(),
            "reviewed.md".into(),
            "# Shared Page\n\n**Original** text\n\nKeep this other block exactly.\n".into(),
        )
        .unwrap()
        .id;
    // Actual signed trust governs the Agent; no App or synthetic worker grant.
    {
        let mut e = actor.agent.lock();
        let agent = e.me_id().unwrap();
        e.append(
            &actor.space,
            &actor.owner,
            EventBody::GrantIssued {
                grant: Grant {
                    id: types::new_id("page_trust"),
                    grantor: actor.owner.clone(),
                    grantee: Some(agent),
                    scope: GrantScope::Space(actor.space.clone()),
                    capability: Capability::Trust(TrustLevel::Act),
                    expires_at_ms: None,
                },
            },
        )
        .unwrap();
    }
    PageFixture { actor, page }
}
fn turn(f: &PageFixture) -> AgentPageTurn {
    f.actor
        .agent
        .agent_page_turn(&f.page, &f.actor.source, &f.actor.owner)
        .unwrap()
}
fn command(t: &AgentPageTurn) -> tools::ReplaceBlock {
    let before = t
        .snapshot
        .blocks
        .iter()
        .find(|b| b.text == "Original text")
        .unwrap()
        .clone();
    let mut after = before.clone();
    after.text = "Reviewed 🧪 replacement\nwith exact whitespace  ".into();
    tools::ReplaceBlock {
        artifact_id: t.snapshot.artifact_id.clone(),
        version: t.snapshot.version,
        prefix_sha256: t.snapshot.prefix_sha256.clone(),
        content_sha256: t.snapshot.content_sha256.clone(),
        before,
        after,
    }
}
fn input(
    f: &PageFixture,
    t: &AgentPageTurn,
    tool: &str,
    arguments: Value,
) -> AgentToolApprovalInput {
    let mut definition = tools::tools()
        .into_iter()
        .find(|d| d["name"] == tool)
        .unwrap();
    let alias = tools::model_name(tool, &f.page).unwrap();
    definition["name"] = alias.clone().into();
    let inbound = crate::agent_binary_documents::tool_approvals::source_hash(
        &f.actor.agent.lock().state,
        &f.actor.space,
        &f.actor.source,
        &f.actor.owner,
    )
    .unwrap()
    .1;
    let rule = "native:review-required";
    let continuation_json = json!({"version":1,"policy_sha256":"77".repeat(32),
        "envelope":{"agent":t.snapshot.actor_id,"device":t.device,"owner":t.owner,"space":t.snapshot.space_id,"from":t.serving,"inbound_sha256":inbound},
        "call":{"id":"page-fixture-call","name":alias,"arguments":arguments},"definition":definition,
        "steps":[],"remaining":[],"round":0,"rule":rule}).to_string();
    AgentToolApprovalInput {
        canonical_tool: tool.into(),
        arguments,
        worker_binding: tools::executor_binding(),
        continuation_json,
        gate_rule: rule.into(),
    }
}
fn stage(f: &PageFixture, t: &AgentPageTurn, c: &tools::ReplaceBlock) -> crate::AgentRequestDto {
    f.actor
        .agent
        .stage_page_tool_approval(
            t,
            input(f, t, "replace_page_block", serde_json::to_value(c).unwrap()),
        )
        .unwrap()
}
fn decide(
    f: &PageFixture,
    req: &crate::AgentRequestDto,
    decision: ToolRequestDecision,
) -> R<crate::DecideOutcome> {
    let mut e = f.actor.agent.lock();
    let agent = e.me.replace(f.actor.owner.clone());
    let synced = e.net.synced.remove(&f.actor.space);
    let result = e.decide_tool_request(
        &req.id,
        &req.tool_approval.as_ref().unwrap().content_hash,
        decision,
    );
    if synced {
        e.net.synced.insert(f.actor.space.clone());
    }
    e.me = agent;
    result
}
fn approved(
    f: &PageFixture,
    t: &AgentPageTurn,
    c: &tools::ReplaceBlock,
) -> (crate::AgentRequestDto, crate::ApprovedPageTurn) {
    let request = stage(f, t, c);
    decide(f, &request, ToolRequestDecision::ApproveOnce).unwrap();
    let permit = f.actor.agent.approved_page_tool_turn(&request.id).unwrap();
    (request, permit)
}
fn counts(f: &PageFixture) -> Vec<i64> {
    let e = f.actor.agent.lock();
    [
        "events",
        "outbox",
        "media",
        "media_keys",
        "blob_uploads",
        "meta",
    ]
    .iter()
    .map(|table| {
        e.store
            .conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}
fn page(f: &PageFixture) -> crate::PageDto {
    f.actor.agent.page(f.page.clone()).unwrap()
}
fn block(f: &PageFixture) {
    let mut e = f.actor.agent.lock();
    let agent = e.me_id().unwrap();
    e.append(
        &f.actor.space,
        &f.actor.owner,
        EventBody::GrantIssued {
            grant: Grant {
                id: types::new_id("block_page"),
                grantor: f.actor.owner.clone(),
                grantee: Some(agent),
                scope: GrantScope::Space(f.actor.space.clone()),
                capability: Capability::Trust(TrustLevel::Listen),
                expires_at_ms: None,
            },
        },
    )
    .unwrap();
}

mod renewal;

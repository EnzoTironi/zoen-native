//! Normal signed transport and owned package admission for pending-review cases.
use super::*;
use crate::agent_scenes::{tools as scene_tools, SceneSnapshot};

pub(super) type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
pub(super) struct Owned {
    pub f: Fixture,
    pub item: String,
    pub install: crate::app_resources::Install,
    pub pin: String,
}

fn confirm(client: &Client, space: &str) -> TestResult {
    let rows = client.lock().store.outbox()?;
    for row in rows
        .into_iter()
        .filter(|row| row.event.space == space && !row.failed)
    {
        let mut e = client.lock();
        let mut log = e
            .logs
            .get(space)
            .cloned()
            .unwrap_or_else(|| event_log::SpaceLog::new(space));
        let event = log.sequence(row.event)?.clone();
        assert!(matches!(
            e.ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(&event)?,
            }),
            crate::Ingest::Confirmed | crate::Ingest::Applied
        ));
    }
    Ok(())
}

/// Only normal signed envelopes and admitted media are delivered. Every view is
/// rebuilt by ingest; no App/request/grant projection rows are manufactured.
pub(super) fn deliver(from: &Client, to: &Client, space: &str) -> TestResult {
    confirm(from, space)?;
    let (profiles, events, media) = {
        let e = from.lock();
        let events = e.logs.get(space).ok_or("source log")?.events().to_vec();
        let mut media = Vec::new();
        for event in &events {
            let content = match &event.body {
                EventBody::ItemCreated { content, .. }
                | EventBody::ItemVersioned { content, .. } => Some(content),
                _ => None,
            };
            let mut files = Vec::new();
            if let Some(ItemContent::File(file)) = content {
                files.push(file);
            }
            if let Some(ItemContent::App(app)) = content {
                files.extend(app.document.as_ref());
                files.extend(app.view.as_deref());
            }
            for file in files {
                for chunk in &file.chunks {
                    if let Some((mime, bytes)) = e.store.media(&chunk.sha256)? {
                        media.push((chunk.clone(), mime, bytes, event.at_ms));
                    }
                }
            }
        }
        (e.identities.values().cloned().collect(), events, media)
    };
    let mut target = to.lock();
    target.put_profiles(profiles)?;
    for (chunk, mime, bytes, at) in media {
        target.store.put_media(&chunk.sha256, &mime, &bytes, at)?;
        target
            .store
            .learn_media_key(&chunk.sha256, &chunk.blob, &chunk.key)?;
    }
    for event in events {
        if target.logs.get(space).is_some_and(|log| {
            log.events()
                .iter()
                .any(|known| known.client_id == event.client_id)
        }) {
            continue;
        }
        assert!(matches!(
            target.ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(&event)?,
            }),
            crate::Ingest::Applied | crate::Ingest::Confirmed
        ));
    }
    Ok(())
}

pub(super) fn owned(targeted: bool) -> TestResult<Owned> {
    let mut f = fixture()?;
    deliver(&f.agent, &f.owner_client, &f.space)?;
    let html = b"<!doctype html><p>Pending owned drawing review</p>";
    let package = f.owner_client.file_add(
        f.space.clone(),
        String::new(),
        "editor.html".into(),
        crate::app_resources::MIME.into(),
        html.to_vec(),
        None,
    )?;
    let account = f.owner_client.account().ok_or("owner account")?;
    let install = crate::app_resources::Install {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: f.space.clone(),
        package_item_id: package.id,
        package_version: 1,
        package_sha256: sha(html),
        title: "Pending drawing".into(),
        mutation_id: "pending_scene_install".into(),
    };
    let item = f
        .owner_client
        .lock()
        .install_app_resource(install.clone())?
        .id;
    let pin = f
        .owner_client
        .item(item.clone())?
        .app
        .ok_or("owned App")?
        .resource_uri;
    let agent = f.agent.account().ok_or("Agent account")?.identity_id;
    f.owner_client
        .lock()
        .set_trust(&agent, &f.space, TrustLevel::Suggest)?;
    let source = append_source(&f, &item, targeted.then_some(agent))?;
    deliver(&f.owner_client, &f.agent, &f.space)?;
    f.source = source;
    let scene = f.agent.app_document_read(item.clone())?;
    assert_eq!(scene.version, 1);
    assert!(f
        .agent
        .agent_artifact_request(&f.space, &f.source, &f.owner)?
        .is_some());
    Ok(Owned {
        f,
        item,
        install,
        pin,
    })
}

pub(super) fn append_source(f: &Fixture, item: &str, target: Option<String>) -> TestResult<String> {
    let mut e = f.owner_client.lock();
    let doc = e.app_document_read(item)?;
    let root = e
        .item_discussion(&f.space, item)?
        .root
        .ok_or("App discussion")?;
    let text = "Read the unchanged owned drawing";
    let mut reference = types::ReplyRef::thread(root.id);
    reference.artifact = Some(types::ArtifactRequest {
        item_id: item.into(),
        version: doc.version,
        sha256: doc.sha256,
        request_id: types::new_id("pending_scene_source")?,
        text_sha256: sha(text.as_bytes()),
        // Legacy and explicit sources must preserve their original signed shape.
        intent_sha256: target.as_ref().map(|_| "55".repeat(32)),
        agent_id: target,
    });
    Ok(e.append(
        &f.space,
        &f.owner,
        EventBody::AppEditRequested {
            message: types::new_id("msg")?,
            text: text.into(),
            reply: reference,
        },
    )?
    .client_id)
}

pub(super) fn stage_scene(o: &Owned) -> TestResult<AgentRequestDto> {
    let turn =
        o.f.agent
            .agent_scene_turn(&o.item, &o.f.source, &o.f.owner)?;
    let alias = scene_tools::model_name("read_scene", &o.item).ok_or("read alias")?;
    let mut definition = scene_tools::tools()
        .into_iter()
        .find(|d| d["name"] == "read_scene")
        .ok_or("definition")?;
    definition["name"] = json!(alias);
    let args = json!({"artifactId":o.item});
    let (_, inbound) = source_hash(&o.f.agent.lock().state, &o.f.space, &o.f.source, &o.f.owner)
        .ok_or("signed source")?;
    let device = o.f.agent.account().ok_or("Agent account")?.device_id;
    let continuation = json!({"version":1,"policy_sha256":"77".repeat(32),
        "envelope":{"agent":turn.snapshot().actor_id,"device":device,"owner":o.f.owner,
            "space":o.f.space,"from":o.f.owner,"inbound_sha256":inbound},
        "call":{"id":"pending-scene-call","name":alias,"arguments":args},
        "definition":definition,"steps":[],"remaining":[],"round":0,"rule":"native:review-required"});
    Ok(o.f.agent.stage_scene_tool_approval(
        &turn,
        AgentToolApprovalInput {
            canonical_tool: "read_scene".into(),
            arguments: args,
            worker_binding: scene_tools::executor_binding(),
            continuation_json: continuation.to_string(),
            gate_rule: "native:review-required".into(),
        },
    )?)
}

pub(super) fn expired_pending(f: &Fixture, live: &AgentRequestDto) -> TestResult<AgentRequestDto> {
    confirm_pending(f)?;
    let mut request = f.agent.lock().state.requests[&live.id].req.clone();
    request.id = types::new_id("expired_pending_review")?;
    let expiry = crate::engine::now_ms() - 1000;
    request
        .tool_proposal
        .as_mut()
        .ok_or("proposal")?
        .expires_at_ms = expiry;
    request.content_hash = request
        .tool_proposal_hash(&f.space)
        .ok_or("proposal hash")?;
    let id = request.id.clone();
    append_at(f, request, expiry - 1000)?;
    let dto = f
        .agent
        .requests()
        .into_iter()
        .find(|r| r.id == id)
        .ok_or("pending request")?;
    assert_eq!(dto.status, RequestStatus::Stale);
    assert!(f.agent.lock().state.requests[&id].resolution.is_none());
    Ok(dto)
}

pub(super) fn owner_decision(
    o: &Owned,
    r: &AgentRequestDto,
    decision: ToolRequestDecision,
) -> TestResult<DecideOutcome> {
    deliver(&o.f.agent, &o.f.owner_client, &o.f.space)?;
    let result =
        o.f.owner_client
            .decide_tool_request(r.id.clone(), observed(r).into(), decision)?;
    deliver(&o.f.owner_client, &o.f.agent, &o.f.space)?;
    Ok(result)
}

pub(super) fn source_count(f: &Fixture) -> usize {
    f.agent.lock().state.spaces[&f.space].entries.iter().filter(|e|
        matches!(&e.body, EntryBody::Message { reply: Some(r), .. } if r.artifact.is_some())
    ).count()
}

pub(super) fn read_owned(o: &Owned) -> TestResult<SceneSnapshot> {
    let turn =
        o.f.agent
            .agent_scene_turn(&o.item, &o.f.source, &o.f.owner)?;
    Ok(o.f.agent.agent_scene_finish_read(&turn)?)
}

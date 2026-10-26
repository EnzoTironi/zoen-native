//! Real late MLS membership, current sharing and separately signed owner consent.
use super::*;
use crate::agent_binary_documents::tests::Vault;
use std::{collections::HashMap, path::PathBuf, sync::Arc};

pub(super) struct Fixture {
    pub directory: PathBuf,
    pub owner: Arc<Client>,
    pub agent: Arc<Client>,
    pub owner_vault: Arc<Vault>,
    pub agent_vault: Arc<Vault>,
    pub blobs: HashMap<String, Vec<u8>>,
    pub item: String,
    pub source: String,
    pub pin: String,
}
pub(super) fn publish(
    from: &Client,
    to: &Client,
    blobs: &mut HashMap<String, Vec<u8>>,
) -> TestResult<Vec<Sequenced>> {
    let mut from = from.lock();
    blobs.extend(from.pending_uploads(100));
    relay_ready_to_peer(&mut from, &mut to.lock())
}
pub(super) fn media(client: &Client, blobs: &HashMap<String, Vec<u8>>) -> TestResult {
    let mut e = client.lock();
    for wanted in e.wanted_media() {
        assert!(e.media_arrived(&wanted, blobs.get(&wanted.blob).ok_or("ciphertext")?));
    }
    Ok(())
}
pub(super) fn fixture(legacy: bool) -> TestResult<Fixture> {
    let directory = std::env::temp_dir().join(types::new_id("owned-scene-receiver")?);
    std::fs::create_dir(&directory)?;
    let owner = Client::open(
        directory
            .join("owner.sqlite")
            .to_str()
            .ok_or("path")?
            .into(),
        "en".into(),
    )?;
    let owner_vault = Arc::new(Vault::default());
    let owner_id = owner
        .create_account(
            "Owner".into(),
            "scene_receiver_owner".into(),
            "http://relay.test".into(),
            owner_vault.clone(),
        )?
        .identity_id;
    let root = [84; 32];
    let agent_id = Signer::from_secret(&root).id();
    let proof = owner.authorize_agent_json(agent_id.clone())?;
    let agent = Client::open(
        directory
            .join("agent.sqlite")
            .to_str()
            .ok_or("path")?
            .into(),
        "en".into(),
    )?;
    let agent_vault = Arc::new(Vault::default());
    agent.create_agent_account(
        "Agent".into(),
        "scene_receiver_agent".into(),
        "http://relay.test".into(),
        root.to_vec(),
        owner_id.clone(),
        proof,
        agent_vault.clone(),
    )?;
    let profile = agent.lock().identities[&agent_id].clone();
    owner.lock().put_profiles(vec![profile])?;
    let profile = owner.lock().identities[&owner_id].clone();
    agent.lock().put_profiles(vec![profile])?;
    for frame in claim_group(&mut owner.lock(), SPACE, &mut [])? {
        assert_eq!(agent.lock().ingest(frame), Ingest::Applied);
    }
    let html = b"<!doctype html><p>Owned scene receiver</p>";
    let package = owner.file_add(
        SPACE.into(),
        String::new(),
        "editor.html".into(),
        crate::app_resources::MIME.into(),
        html.to_vec(),
        None,
    )?;
    let account = owner.account().ok_or("owner account")?;
    let installed = owner
        .lock()
        .install_app_resource(crate::app_resources::Install {
            expected_identity_id: account.identity_id.clone(),
            expected_device_id: account.device_id.clone(),
            space_id: SPACE.into(),
            package_item_id: package.id,
            package_version: 1,
            package_sha256: event_log::sha256_hex(html),
            title: "Shared drawing".into(),
            mutation_id: "scene_receiver_install".into(),
        })?;
    let item = installed.id;
    let pin = owner.item(item.clone())?.app.ok_or("App")?.resource_uri;
    let base = owner.app_document_read(item.clone())?;
    let scene = serde_json::json!({"type":"excalidraw","version":2,"elements":[],"files":{},
        "appState":{},"unknown":{"keep":"owner data"}})
    .to_string();
    owner.lock().save_app_document(
        &item,
        base.version,
        (&base.sha256, Some(&pin)),
        &scene,
        "owner_before_join",
    )?;
    if legacy {
        let mut e = owner.lock();
        let ItemContent::App(mut app) = e.item_state(&item)?.current().content.clone() else {
            return Err("App".into());
        };
        app.document.as_mut().ok_or("document")?.app_origin = None;
        // Synthetic old producer's signed next version, before any disclosure.
        // No stored history or materialized App row is rewritten.
        e.append(
            SPACE,
            &owner_id,
            EventBody::ItemVersioned {
                item: item.clone(),
                content: ItemContent::App(app),
                note: "Legacy save".into(),
            },
        )?;
    }
    let mut blobs = HashMap::new();
    publish(&owner, &agent, &mut blobs)?;
    assert!(
        agent.item(item.clone()).is_err(),
        "no historical plaintext before Welcome"
    );
    let owner_author = owner.lock().net.author.clone().ok_or("owner author")?;
    let agent_author = agent.lock().net.author.clone().ok_or("agent author")?;
    add_member(&mut owner.lock(), &owner_author, SPACE, &agent_author)?;
    let added = owner.lock().logs[SPACE]
        .events()
        .last()
        .ok_or("MemberAdded")?
        .clone();
    let frame = ordered(&agent.lock(), Envelope::plain(&added)?)?;
    assert_eq!(agent.lock().ingest(frame), Ingest::Applied);
    let data = agent.lock().device()?.key_packages(1, false)?.remove(0);
    owner
        .lock()
        .commit_now(SPACE, &[data], &Default::default())?;
    publish(&owner, &agent, &mut blobs)?;
    owner
        .lock()
        .set_trust(&agent_id, SPACE, types::TrustLevel::Suggest)?;
    publish(&owner, &agent, &mut blobs)?;
    let context =
        owner
            .lock()
            .app_current_share_context(crate::app_current_share::AppShareScope {
                expected_identity_id: account.identity_id,
                expected_device_id: account.device_id,
                space_id: SPACE.into(),
                item_id: item.clone(),
                target_id: agent_id,
            })?;
    assert!(
        owner
            .lock()
            .share_app_current(context, "scene_receiver_share")?
            .applied
    );
    publish(&owner, &agent, &mut blobs)?;
    media(&agent, &blobs)?;
    assert_eq!(
        owner.app_document_read(item.clone())?,
        agent.app_document_read(item.clone())?
    );
    assert!(
        agent.item_at(item.clone(), 1).is_err(),
        "current share does not import history"
    );
    let doc = owner.app_document_read(item.clone())?;
    let anchor = owner
        .lock()
        .item_discussion(SPACE, &item)?
        .root
        .ok_or("anchor")?
        .id;
    let mut reply = types::ReplyRef::thread(anchor);
    reply.artifact = Some(types::ArtifactRequest {
        item_id: item.clone(),
        version: doc.version,
        sha256: doc.sha256,
        request_id: types::new_id("scene_receiver_source")?,
        text_sha256: event_log::sha256_hex(TEXT.as_bytes()),
        agent_id: None,
        intent_sha256: None,
    });
    let source = owner
        .lock()
        .append(
            SPACE,
            &owner_id,
            EventBody::AppEditRequested {
                message: types::new_id("msg")?,
                text: TEXT.into(),
                reply,
            },
        )?
        .client_id;
    publish(&owner, &agent, &mut blobs)?;
    Ok(Fixture {
        directory,
        owner,
        agent,
        owner_vault,
        agent_vault,
        blobs,
        item,
        source,
        pin,
    })
}

pub(super) fn stage(f: &Fixture) -> TestResult<crate::AgentRequestDto> {
    use crate::agent_scenes::tools;
    let owner = f.owner.account().ok_or("owner")?.identity_id;
    let turn = f.agent.agent_scene_turn(&f.item, &f.source, &owner)?;
    let snapshot = turn.snapshot();
    let alias = tools::model_name("replace_scene", &f.item).ok_or("alias")?;
    let args = serde_json::json!({"artifactId":f.item,"version":snapshot.version,
        "sha256":snapshot.sha256,"resourceUri":f.pin,"sceneJson":edited_scene()});
    let continuation = serde_json::json!({"version":1,"policy_sha256":"77".repeat(32),
        "envelope":{"agent":snapshot.actor_id,"device":f.agent.account().ok_or("agent")?.device_id,
            "owner":owner,"space":SPACE,"from":owner,"inbound_sha256":event_log::sha256_hex(TEXT.as_bytes())},
        "call":{"id":"replace-approved-scene","name":alias,"arguments":args},
        "definition":{"name":alias},"steps":[],"remaining":[],"round":0,"rule":"native:review-required"});
    Ok(f.agent.stage_scene_tool_approval(
        &turn,
        crate::AgentToolApprovalInput {
            canonical_tool: "replace_scene".into(),
            arguments: args,
            worker_binding: tools::executor_binding(),
            continuation_json: continuation.to_string(),
            gate_rule: "native:review-required".into(),
        },
    )?)
}
pub(super) fn approve(f: &mut Fixture, request: &crate::AgentRequestDto) -> TestResult {
    publish(&f.agent, &f.owner, &mut f.blobs)?;
    f.owner.decide_tool_request(
        request.id.clone(),
        request
            .tool_approval
            .as_ref()
            .ok_or("approval")?
            .content_hash
            .clone(),
        crate::ToolRequestDecision::ApproveOnce,
    )?;
    publish(&f.owner, &f.agent, &mut f.blobs)?;
    Ok(())
}

pub(super) fn reopen(f: &Fixture, name: &str, vault: Arc<Vault>) -> TestResult<Arc<Client>> {
    let path = f.directory.join(format!("{name}.sqlite"));
    let client = Client::open(path.to_str().ok_or("cold path")?.into(), "en".into())?;
    assert!(client.unlock(vault)?);
    Ok(client)
}

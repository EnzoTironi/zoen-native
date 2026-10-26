//! Fresh real owner/Agent accounts and the normal package admission command.
//! The offline transport sequences and delivers signed events, never App rows.
use super::*;
use client_core::{PrivacyDto, SecretVault};
use sha2::{Digest, Sha256};

type R<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Vault = crate::deckcraft::tests::Vault;
pub(super) struct Fixture {
    pub path: std::path::PathBuf,
    pub client: Arc<Client>,
    pub vault: Arc<Vault>,
    pub owner_vault: Arc<Vault>,
    pub item: String,
    pub space: String,
    pub owner: String,
    pub source: String,
    pub resource_uri: String,
}

pub(super) fn signer(vault: &Vault) -> event_log::Author {
    let root: [u8; 32] = vault
        .load("zoen.identity.v1".into())
        .unwrap()
        .try_into()
        .unwrap();
    let device: [u8; 32] = vault
        .load("zoen.device.v1".into())
        .unwrap()
        .try_into()
        .unwrap();
    event_log::Author::device(
        &event_log::Signer::from_secret(&root),
        event_log::Signer::from_secret(&device),
    )
}
fn ordered(store: &store::Store, space: &str) -> R<event_log::SpaceLog> {
    let mut log = event_log::SpaceLog::from_events(space, store.events(space)?)?;
    let known: BTreeSet<_> = log
        .events()
        .iter()
        .map(|event| event.client_id.clone())
        .collect();
    for row in store
        .outbox()?
        .into_iter()
        .filter(|row| row.event.space == space)
    {
        let client_id = row.event.client_id.clone();
        if !known.contains(&client_id) {
            let event = log.sequence(row.event)?;
            store.append_event(event)?;
        }
        store.outbox_remove(&client_id)?;
    }
    log.verify()?;
    Ok(log)
}

fn transfer(from: &store::Store, to: &store::Store, space: &str) -> R<()> {
    let known: BTreeSet<_> = to.events(space)?.into_iter().map(|e| e.client_id).collect();
    for event in from.events(space)? {
        if !known.contains(&event.client_id) {
            to.append_event(&event)?;
        }
        let content = match &event.body {
            types::EventBody::ItemCreated { content, .. }
            | types::EventBody::ItemVersioned { content, .. } => Some(content),
            _ => None,
        };
        let mut files = Vec::new();
        if let Some(types::ItemContent::File(file)) = content {
            files.push(file);
        }
        if let Some(types::ItemContent::App(app)) = content {
            files.extend(app.document.as_ref());
            files.extend(app.view.as_deref());
        }
        for file in files {
            for chunk in &file.chunks {
                if let Some((mime, bytes)) = from.media(&chunk.sha256)? {
                    to.put_media(&chunk.sha256, &mime, &bytes, event.at_ms)?;
                    to.learn_media_key(&chunk.sha256, &chunk.blob, &chunk.key)?;
                }
            }
        }
    }
    event_log::SpaceLog::from_events(space, to.events(space)?)?.verify()?;
    Ok(())
}

fn close(f: &mut Fixture) {
    let client = std::mem::replace(
        &mut f.client,
        Client::open(":memory:".into(), "en".into()).unwrap(),
    );
    assert_eq!(
        Arc::strong_count(&client),
        1,
        "close the real fixture before offline delivery"
    );
    drop(client);
}

/// Only signed protocol events are injected by the offline transport. Projection
/// is always rebuilt by the native Client; callers cannot replace App rows.
pub(super) fn rewrite<T>(
    f: &mut Fixture,
    operation: impl FnOnce(&store::Store, &mut event_log::SpaceLog) -> R<T>,
) -> R<T> {
    close(f);
    let result = (|| {
        let store = store::Store::open(f.path.join("agent.sqlite").to_str().unwrap())?;
        let mut log = ordered(&store, &f.space)?;
        let result = operation(&store, &mut log);
        log.verify()?;
        result
    })();
    f.client = canonical_tool_approval_tests::reopen(&f.path, f.vault.clone());
    result
}

pub(super) fn append(
    store: &store::Store,
    log: &mut event_log::SpaceLog,
    space: &str,
    author: &event_log::Author,
    at: i64,
    body: types::EventBody,
) -> R<()> {
    let event = author.sign_event(space, &types::new_ulid(at)?, at, log.head(), body)?;
    store.append_event(log.sequence(event)?)?;
    Ok(())
}

/// Advance the real Owner Client through its public APIs, then deliver its
/// signed events and authorized cached bytes back to the same Agent account.
pub(super) async fn owner_change<F, Fut>(f: &mut Fixture, operation: F) -> R<()>
where
    F: FnOnce(Arc<Client>) -> Fut,
    Fut: std::future::Future<Output = R<()>>,
{
    close(f);
    {
        let agent = store::Store::open(f.path.join("agent.sqlite").to_str().unwrap())?;
        ordered(&agent, &f.space)?;
        let owner = store::Store::open(f.path.join("owner.sqlite").to_str().unwrap())?;
        transfer(&agent, &owner, &f.space)?;
    }
    let owner = Client::open(
        f.path.join("owner.sqlite").to_str().unwrap().into(),
        "en".into(),
    )?;
    assert!(owner.unlock(f.owner_vault.clone())?);
    let result = operation(owner.clone()).await;
    drop(owner);
    {
        let owner = store::Store::open(f.path.join("owner.sqlite").to_str().unwrap())?;
        ordered(&owner, &f.space)?;
        let agent = store::Store::open(f.path.join("agent.sqlite").to_str().unwrap())?;
        transfer(&owner, &agent, &f.space)?;
    }
    f.client = canonical_tool_approval_tests::reopen(&f.path, f.vault.clone());
    result
}

pub(super) async fn fixture() -> R<Fixture> {
    let path = std::env::temp_dir().join(types::new_id("owned-scene-host")?);
    std::fs::create_dir_all(&path)?;
    let owner_path = path.join("owner.sqlite");
    let agent_path = path.join("agent.sqlite");
    let owner_vault = Arc::new(Vault::default());
    let human = Client::open(owner_path.to_str().unwrap().into(), "en".into())?;
    let owner = human
        .create_account(
            "Owner".into(),
            "scene_owner".into(),
            "http://127.0.0.1:1".into(),
            owner_vault.clone(),
        )?
        .identity_id;
    let root = [82; 32];
    let agent_id = event_log::Signer::from_secret(&root).id();
    let proof = human.authorize_agent_json(agent_id.clone())?;
    let vault = Arc::new(Vault::default());
    let agent = Client::open(agent_path.to_str().unwrap().into(), "en".into())?;
    agent.create_agent_account(
        "Scene Agent".into(),
        "scene_agent".into(),
        "http://127.0.0.1:1".into(),
        root.to_vec(),
        owner.clone(),
        proof,
        vault.clone(),
    )?;
    drop(agent);
    drop(human);
    let agent_store = store::Store::open(agent_path.to_str().unwrap())?;
    let profile = agent_store
        .identities()?
        .into_iter()
        .find(|(profile, _)| profile.id == agent_id)
        .unwrap()
        .0;
    drop(agent_store);
    let owner_store = store::Store::open(owner_path.to_str().unwrap())?;
    owner_store.put_identity(&profile, None)?;
    drop(owner_store);
    let human = Client::open(owner_path.to_str().unwrap().into(), "en".into())?;
    assert!(human.unlock(owner_vault.clone())?);
    let space = human.create_group_with(
        "Owned scene".into(),
        vec![agent_id.clone()],
        PrivacyDto::Closed,
    )?;
    human.set_trust(agent_id, space.clone(), client_core::TrustLevelDto::Suggest)?;
    let html = b"<!doctype html><p>Owned scene transport fixture</p>";
    let package = human.file_add(
        space.clone(),
        "".into(),
        "editor.html".into(),
        "text/html;profile=mcp-app".into(),
        html.to_vec(),
        None,
    )?;
    let account = human.account().unwrap();
    let installed: serde_json::Value = serde_json::from_str(&human.client_request(json!({
        "op":"app_resource_install","input":{
            "expected_identity_id":account.identity_id,"expected_device_id":account.device_id,
            "space_id":space,"package_item_id":package.id,"package_version":1,
            "package_sha256":hex::encode(Sha256::digest(html)),"title":"Reviewed drawing",
            "mutation_id":"scene_host_install"
        }
    }).to_string()).await?)?;
    let item = installed["id"].as_str().unwrap().to_owned();
    let resource_uri = human.item(item.clone())?.app.unwrap().resource_uri;
    let document = human.app_document_read(item.clone())?;
    let anchor = human.timeline(space.clone())?.into_iter().find(|entry| {
        matches!(&entry.kind, EntryKind::Message { card: Some(card), .. } if card.item_id == item)
    }).unwrap().id;
    drop(human);
    let owner_store = store::Store::open(owner_path.to_str().unwrap())?;
    let mut log = ordered(&owner_store, &space)?;
    let text = "Update this drawing with one reviewed text element";
    let mut reply = types::ReplyRef::thread(anchor);
    reply.artifact = Some(types::ArtifactRequest {
        item_id: item.clone(),
        version: document.version,
        sha256: document.sha256,
        request_id: types::new_id("scene_request")?,
        text_sha256: hex::encode(Sha256::digest(text.as_bytes())),
        agent_id: None,
        intent_sha256: None,
    });
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;
    let source_event = signer(&owner_vault).sign_event(
        &space,
        &types::new_ulid(at)?,
        at,
        log.head(),
        types::EventBody::AppEditRequested {
            message: types::new_id("msg")?,
            text: text.into(),
            reply,
        },
    )?;
    let source = log.sequence(source_event)?.client_id.clone();
    log.verify()?;
    let agent_store = store::Store::open(agent_path.to_str().unwrap())?;
    for (profile, _) in owner_store.identities()? {
        agent_store.put_identity(&profile, None)?;
    }
    for event in log.events() {
        agent_store.append_event(event)?;
        let content = match &event.body {
            types::EventBody::ItemCreated { content, .. }
            | types::EventBody::ItemVersioned { content, .. } => Some(content),
            _ => None,
        };
        let mut files = Vec::new();
        if let Some(types::ItemContent::File(file)) = content {
            files.push(file);
        }
        if let Some(types::ItemContent::App(app)) = content {
            files.extend(app.document.as_ref());
            files.extend(app.view.as_deref());
        }
        for file in files {
            for chunk in &file.chunks {
                let (mime, bytes) = owner_store.media(&chunk.sha256)?.unwrap();
                agent_store.put_media(&chunk.sha256, &mime, &bytes, at)?;
                agent_store.learn_media_key(&chunk.sha256, &chunk.blob, &chunk.key)?;
            }
        }
    }
    drop(owner_store);
    drop(agent_store);
    let client = canonical_tool_approval_tests::reopen(&path, vault.clone());
    assert!(client
        .agent_artifact_request(&space, &source, &owner)?
        .is_some());
    assert_eq!(client.app_document_read(item.clone())?.version, 1);
    Ok(Fixture {
        path,
        client,
        vault,
        owner_vault,
        item,
        space,
        owner,
        source,
        resource_uri,
    })
}

pub(super) fn approve(
    path: &Path,
    space: &str,
    request: &types::AgentRequest,
    owner: &Vault,
) -> R<()> {
    let store = store::Store::open(path.join("agent.sqlite").to_str().unwrap())?;
    let mut log = ordered(&store, space)?;
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;
    let event = signer(owner).sign_event(
        space,
        &types::new_ulid(at)?,
        at,
        log.head(),
        types::EventBody::RequestResolved {
            request: request.id.clone(),
            approved: true,
            content_hash: request.content_hash.clone(),
            resolution: Some(types::RequestResolution {
                message: "Approved the exact owned scene once".into(),
                created_item: None,
                ai_cost_cents: 0,
                standing_grant: None,
                tool_decision: Some(types::ToolDecision::ApproveOnce),
            }),
        },
    )?;
    store.append_event(log.sequence(event)?)?;
    log.verify()?;
    Ok(())
}

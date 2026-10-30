//! Existing synthetic owner/agent fixture construction; original types stay in tests.

use super::*;

pub(crate) fn fixture_at(
    path: std::path::PathBuf,
    root: [u8; 32],
) -> std::result::Result<Fixture, Box<dyn std::error::Error>> {
    fixture_with_privacy_at(path, root, types::Privacy::Closed)
}
pub(crate) fn fixture_with_privacy_at(
    path: std::path::PathBuf,
    root: [u8; 32],
    privacy: types::Privacy,
) -> std::result::Result<Fixture, Box<dyn std::error::Error>> {
    fixture_for_format_at(path, root, privacy, "deckcraft", ORIGINAL)
}

pub(super) fn fixture_for_format_at(
    path: std::path::PathBuf,
    root: [u8; 32],
    privacy: types::Privacy,
    app: &str,
    original: &[u8],
) -> std::result::Result<Fixture, Box<dyn std::error::Error>> {
    let owner_path = path.join("owner.sqlite");
    let owner_vault = Arc::new(Vault::default());
    let human = Client::open(owner_path.to_str().unwrap().into(), "en".into()).unwrap();
    let owner = human
        .create_account(
            "Owner".into(),
            "worker_owner".into(),
            "http://127.0.0.1:1".into(),
            owner_vault.clone(),
        )
        .unwrap()
        .identity_id;
    let id = event_log::Signer::from_secret(&root).id();
    let proof = human.authorize_agent_json(id).unwrap();
    drop(human);
    let owner_store = store::Store::open(owner_path.to_str().unwrap()).unwrap();
    let profile = owner_store
        .identities()
        .unwrap()
        .into_iter()
        .find(|(profile, _)| profile.id == owner)
        .unwrap()
        .0;
    drop(owner_store);
    let db = path.join("agent.sqlite");
    let vault = Arc::new(Vault::default());
    let agent = Client::open(db.to_str().unwrap().into(), "en".into()).unwrap();
    agent
        .create_agent_account(
            "Worker".into(),
            "worker_agent".into(),
            "http://127.0.0.1:1".into(),
            root.to_vec(),
            owner.clone(),
            proof,
            vault.clone(),
        )
        .unwrap();
    drop(agent);
    // Offline fixture only. All source events still pass the real signature
    // and projection code; no relay/service, mocked core or browser actor.
    let key: [u8; 32] = owner_vault
        .load("zoen.identity.v1".into())
        .unwrap()
        .try_into()
        .unwrap();
    let db_store = store::Store::open(db.to_str().unwrap()).unwrap();
    db_store.put_identity(&profile, Some(&key)).unwrap();
    let space = types::new_id("worker_space")?;
    let mut log = event_log::SpaceLog::new(&space);
    let agent_signer = event_log::Signer::from_secret(&root);
    let human_signer = event_log::Signer::from_secret(&key);
    for (time, body) in [
        (
            1_792_510_809_000,
            types::EventBody::SpaceCreated {
                title: "Worker test".into(),
                kind: types::SpaceKind::Group,
                privacy,
            },
        ),
        (
            1_792_510_809_001,
            types::EventBody::MemberAdded {
                identity: owner.clone(),
                role: types::Role::Member,
            },
        ),
    ] {
        db_store
            .append_event(log.append(&agent_signer, time, body)?)
            .unwrap();
    }
    let source_event = log
        .append(
            &human_signer,
            1_792_510_809_002,
            types::EventBody::MessagePosted {
                message: types::new_id("source")?,
                text: "Update this presentation with the reviewed edits".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
                forwarded_author: None,
            },
        )?
        .clone();
    db_store.append_event(&source_event).unwrap();
    let source = source_event.client_id;
    drop(db_store);
    let client = Client::open(db.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(client.unlock(vault.clone()).unwrap());
    let base = client
        .app_binary_create_for(
            app.into(),
            space.clone(),
            "Agent deck".into(),
            STANDARD.encode(original),
            "worker_create".into(),
        )
        .unwrap();
    Ok(Fixture {
        path,
        client,
        vault,
        owner_vault,
        owner,
        source,
        base,
    })
}

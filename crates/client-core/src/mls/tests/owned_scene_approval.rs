//! The owner must project the already accepted, independently reviewed scene.
//! Transport uses genuine MLS envelopes; no provider/service or retained account.
use super::*;
use crate::Client;
use types::ItemContent;
mod fixture;
use fixture::*;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const SPACE: &str = "approved-scene-receiver";
const TEXT: &str = "Change only the drawing background to red";

fn edited_scene() -> String {
    serde_json::json!({"type":"excalidraw","version":2,"elements":[],"files":{},
        "appState":{"viewBackgroundColor":"#ff0000"},"unknown":{"keep":"owner data"}})
    .to_string()
}

#[test]
fn current_shared_scene_once_approval_reaches_owner_and_both_recover_the_same_version() -> TestResult
{
    for legacy in [false, true] {
        let mut f = fixture(legacy)?;
        let before = f.owner.app_document_read(f.item.clone())?;
        let owner_origin = f
            .owner
            .lock()
            .item_state(&f.item)?
            .current()
            .content
            .owned_file()
            .ok_or("owner file")?
            .app_origin
            .clone();
        let agent_origin = f
            .agent
            .lock()
            .item_state(&f.item)?
            .current()
            .content
            .owned_file()
            .ok_or("agent file")?
            .app_origin
            .clone();
        assert_eq!(owner_origin.is_none(), legacy);
        assert!(agent_origin.is_some(), "validated current-share proof");
        let request = stage(&f)?;
        assert!(f.agent.approved_scene_tool_turn(&request.id).is_err());
        approve(&mut f, &request)?;
        let permit = f.agent.approved_scene_tool_turn(&request.id)?;
        let command: crate::agent_scenes::tools::ReplaceScene =
            serde_json::from_str(&permit.proposal().arguments_json)?;
        let accepted = f.agent.agent_scene_replace(permit.turn(), &command)?;
        assert_eq!(accepted.version, before.version + 1);
        assert_eq!(accepted.scene_json, edited_scene());
        let frames = publish(&f.agent, &f.owner, &mut f.blobs)?;
        media(&f.owner, &f.blobs)?;
        assert_eq!(
            f.owner.app_document_read(f.item.clone())?,
            accepted,
            "owner must not silently refuse an already approved effect (legacy={legacy})"
        );
        for frame in frames {
            assert_eq!(f.owner.lock().ingest(frame), Ingest::Duplicate);
        }
        assert_eq!(
            f.agent.agent_scene_replace(permit.turn(), &command)?,
            accepted,
            "lost effect ACK returns the same receipt, never another version"
        );
        assert_eq!(f.agent.lock().outbox_len(), 0);
        for client in [&f.owner, &f.agent] {
            let item = client.item(f.item.clone())?;
            assert_eq!(item.version, accepted.version);
            assert_eq!(item.app.ok_or("App")?.resource_uri, f.pin);
            assert_eq!(
                client.file_bytes(f.item.clone(), Some(before.version))?,
                Some(before.scene_json.as_bytes().to_vec())
            );
            assert_eq!(
                client.file_bytes(f.item.clone(), Some(accepted.version))?,
                Some(edited_scene().into_bytes())
            );
        }
        let owner_vault = f.owner_vault.clone();
        let agent_vault = f.agent_vault.clone();
        let owner = std::mem::replace(&mut f.owner, Client::open(":memory:".into(), "en".into())?);
        let agent = std::mem::replace(&mut f.agent, Client::open(":memory:".into(), "en".into())?);
        drop(permit);
        drop(owner);
        drop(agent);
        for (name, vault) in [("owner", owner_vault), ("agent", agent_vault)] {
            let cold = reopen(&f, name, vault)?;
            assert_eq!(
                cold.app_document_read(f.item.clone())?,
                accepted,
                "{name} cold"
            );
            assert_eq!(cold.item(f.item.clone())?.version, accepted.version);
            assert_eq!(
                cold.file_bytes(f.item.clone(), Some(before.version))?,
                Some(before.scene_json.as_bytes().to_vec())
            );
        }
        let directory = f.directory.clone();
        drop(f);
        std::fs::remove_dir_all(directory)?;
    }
    Ok(())
}

#[test]
fn shared_origin_does_not_relax_frozen_effect_fields_identity_or_approval_deadline() -> TestResult {
    let mut f = fixture(true)?;
    let request = stage(&f)?;
    approve(&mut f, &request)?;
    let permit = f.agent.approved_scene_tool_turn(&request.id)?;
    let command = serde_json::from_str(&permit.proposal().arguments_json)?;
    let accepted = f.agent.agent_scene_replace(permit.turn(), &command)?;
    let effect = f
        .agent
        .lock()
        .store
        .outbox()?
        .into_iter()
        .find(|row| matches!(&row.event.body, EventBody::ItemVersioned {item,..} if item==&f.item))
        .ok_or("accepted effect event")?
        .event;
    let owner = f.owner.lock();
    assert_eq!(
        owner.item_state(&f.item)?.current().number + 1,
        accepted.version
    );
    assert!(crate::app_resources::event_valid(
        &owner.state,
        &effect,
        &owner.identities
    ));
    for change in [
        "origin_author",
        "origin_device",
        "origin_space",
        "origin_mutation",
        "app",
        "renderer",
        "pin",
        "title",
        "state",
        "hash",
        "bytes",
        "mime",
        "path",
        "voice",
        "actor",
        "device",
        "space",
        "expiry",
    ] {
        // This table tests the exact projection predicate against real admitted
        // state; mutated candidates are never signed, ingested or published.
        let mut candidate = effect.clone();
        match change {
            "actor" => candidate.author = "foreign_agent".into(),
            "device" => candidate.device = Some("foreign_device".into()),
            "space" => candidate.space = "foreign_space".into(),
            "expiry" => candidate.at_ms = permit.proposal().expires_at_ms,
            _ => {
                let EventBody::ItemVersioned {
                    content: ItemContent::App(app),
                    ..
                } = &mut candidate.body
                else {
                    return Err("effect App".into());
                };
                match change {
                    "app" => app.app = "foreign_app".into(),
                    "renderer" => app.view = None,
                    "pin" => app.resource_uri.push_str("changed"),
                    "title" => app.title.push_str("changed"),
                    "state" => app.state_json = "{}".into(),
                    _ => {
                        let file = app.document.as_mut().ok_or("file")?;
                        match change {
                            "hash" => file.sha256 = "00".repeat(32),
                            "bytes" => file.bytes += 1,
                            "mime" => file.mime = "text/plain".into(),
                            "path" => file.path = "/outside".into(),
                            "voice" => file.voice = Some(types::VoiceMeta::default()),
                            _ => {
                                let origin = file.app_origin.as_mut().ok_or("shared origin")?;
                                match change {
                                    "origin_author" => origin.author = "foreign".into(),
                                    "origin_device" => origin.device = "foreign".into(),
                                    "origin_space" => origin.space = "foreign".into(),
                                    _ => origin.mutation = "foreign".into(),
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(
            !crate::app_resources::event_valid(&owner.state, &candidate, &owner.identities),
            "{change} must not borrow creation metadata or owner consent"
        );
    }
    // The historical accepted event is evaluated at its signed occurrence time.
    // A later wall-clock read is not fresh authority to run the tool again.
    let mut last_live = effect.clone();
    last_live.at_ms = permit.proposal().expires_at_ms - 1;
    assert!(crate::app_resources::event_valid(
        &owner.state,
        &last_live,
        &owner.identities
    ));
    drop(owner);
    drop(permit);
    let directory = f.directory.clone();
    drop(f);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

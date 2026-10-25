//! The adopted UI and document travel in the current consent, without package history.
use super::*;
use std::collections::HashMap;

const SPACE: &str = "owned-mcp-new-reader";
fn publish(owner: &mut Engine, peer: &mut Engine, blobs: &mut HashMap<String, Vec<u8>>) -> R<()> {
    blobs.extend(owner.pending_uploads(100));
    relay_ready_to_peer(owner, peer).map_err(storage)?;
    Ok(())
}

#[test]
fn owned_ui_crosses_current_consent_without_package_history_and_agent_suggest_cannot_write() -> R<()>
{
    let (mut owner, author, _) = member(":memory:");
    let mut peer = Engine::open(":memory:")?;
    let root = [83; 32];
    let peer_id = Signer::from_secret(&root).id();
    let proof = event_log::agent_owner::authorize(&author, &peer_id)
        .ok_or_else(|| storage("owner authorization"))?;
    peer.create_agent_account(
        "App collaborator",
        "app_collaborator",
        "http://relay.test",
        root,
        &author.identity,
        proof,
    )?;
    let profile = peer
        .identities
        .get(&peer_id)
        .cloned()
        .ok_or_else(|| storage("certified Agent profile"))?;
    assert!(event_log::agent_owner::profile_authorized(&profile));
    owner.store.put_identity(&profile, None)?;
    owner.identities.insert(peer_id.clone(), profile);
    owner.identity_order.push(peer_id.clone());
    let profile = owner
        .identities
        .get(&author.identity)
        .cloned()
        .ok_or_else(|| storage("owner profile"))?;
    peer.store.put_identity(&profile, None)?;
    peer.identity_order.push(profile.id.clone());
    peer.identities.insert(profile.id.clone(), profile);
    for frame in claim_group(&mut owner, SPACE, &mut []).map_err(storage)? {
        assert_eq!(peer.ingest(frame), Ingest::Applied);
    }
    let html = "<!doctype html><p>Explicitly adopted, self-contained MCP UI</p>";
    let package = owner.file_add(
        SPACE,
        "",
        "editor.html",
        crate::app_resources::MIME,
        html.as_bytes(),
        None,
    )?;
    let account = owner
        .account()
        .cloned()
        .ok_or_else(|| storage("owner device"))?;
    let app = owner.install_app_resource(crate::app_resources::Install {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: SPACE.into(),
        package_item_id: package.clone(),
        package_version: 1,
        package_sha256: event_log::sha256_hex(html.as_bytes()),
        title: "Owned App".into(),
        mutation_id: "before_join".into(),
    })?;
    let uri = app
        .app
        .as_ref()
        .ok_or_else(|| storage("App card"))?
        .resource_uri
        .clone();
    let base = owner.app_document_read(&app.id)?;
    let text = r#"{"type":"excalidraw","version":2,"elements":[],"files":{},"appState":{},"kept":"current document"}"#;
    let saved = owner.save_app_document(
        &app.id,
        base.version,
        (&base.sha256, Some(&uri)),
        text,
        "before_join_edit",
    )?;
    let mut blobs = HashMap::new();
    publish(&mut owner, &mut peer, &mut blobs)?;
    assert!(peer.item_state(&package).is_err());
    assert!(peer.item_state(&app.id).is_err());

    // Add the certified participant normally, then deliver a genuine Commit/Welcome.
    let peer_author = peer
        .net
        .author
        .clone()
        .ok_or_else(|| storage("Agent author"))?;
    add_member(&mut owner, &author, SPACE, &peer_author).map_err(storage)?;
    let added = owner.logs[SPACE]
        .events()
        .last()
        .ok_or_else(|| storage("MemberAdded"))?;
    let frame = ordered(&peer, Envelope::plain(added)?).map_err(storage)?;
    assert_eq!(peer.ingest(frame), Ingest::Applied);
    let records = claimed_records(
        std::slice::from_mut(&mut peer),
        std::slice::from_ref(&peer_id),
    );
    let key_package = &records
        .first()
        .ok_or_else(|| storage("Agent KeyPackage"))?
        .data;
    owner.commit_now(
        SPACE,
        std::slice::from_ref(key_package),
        &Default::default(),
    )?;
    publish(&mut owner, &mut peer, &mut blobs)?;
    assert_eq!(owner.mls_status(SPACE), peer.mls_status(SPACE));
    assert!(
        peer.app_document_read(&app.id).is_err(),
        "Welcome alone discloses neither document nor UI"
    );
    let context = owner.app_current_share_context(crate::app_current_share::AppShareScope {
        expected_identity_id: account.identity,
        expected_device_id: account.device,
        space_id: SPACE.into(),
        item_id: app.id.clone(),
        target_id: peer_id.clone(),
    })?;
    assert!(
        owner
            .share_app_current(context, "share_current_owned_app")?
            .applied
    );
    publish(&mut owner, &mut peer, &mut blobs)?;
    assert!(
        peer.app_document_resource(&app.id, &uri).is_err(),
        "metadata is valid while owned bytes are still arriving"
    );
    for wanted in peer.wanted_media() {
        let sealed = blobs
            .get(&wanted.blob)
            .ok_or_else(|| storage("requested ciphertext"))?;
        assert!(peer.media_arrived(&wanted, sealed));
    }
    assert_eq!(peer.app_document_resource(&app.id, &uri)?.text, html);
    assert_eq!(
        peer.app_document_read(&app.id)?.scene_json,
        saved.scene_json
    );
    assert!(
        peer.item_state(&package).is_err(),
        "the package FileItem and its history were not disclosed"
    );
    assert!(peer.file_bytes(&package, None).is_err());
    assert!(
        peer.save_app_document(
            &app.id,
            saved.version,
            (&saved.sha256, Some(&uri)),
            text,
            "unapproved_agent_write"
        )
        .is_err(),
        "Suggest never becomes Act through App access"
    );
    owner.set_trust(&peer_id, SPACE, types::TrustLevel::Act)?;
    publish(&mut owner, &mut peer, &mut blobs)?;
    let current = peer.app_document_read(&app.id)?;
    let edited = r#"{"type":"excalidraw","version":2,"elements":[],"files":{},"appState":{},"kept":"Agent edited the same artifact"}"#;
    let accepted = peer.save_app_document(
        &app.id,
        current.version,
        (&current.sha256, Some(&uri)),
        edited,
        "authorized_agent_edit",
    )?;
    publish(&mut peer, &mut owner, &mut blobs)?;
    for wanted in owner.wanted_media() {
        let sealed = blobs
            .get(&wanted.blob)
            .ok_or_else(|| storage("Agent document ciphertext"))?;
        assert!(owner.media_arrived(&wanted, sealed));
    }
    assert_eq!(
        owner.app_document_read(&app.id)?.scene_json,
        accepted.scene_json
    );
    assert_eq!(owner.app_document_resource(&app.id, &uri)?.text, html);
    Ok(())
}

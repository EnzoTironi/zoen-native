//! One genuine unrelated human message between a signed share and relay echo.
use super::*;
use crate::app_edit_requests::AppEditRequestInput;
const ROOM: &str = "active_group_current_share";
fn fanout(
    sender: &mut Engine,
    first: &mut Engine,
    second: &mut Engine,
    blobs: &mut HashMap<String, Vec<u8>>,
) {
    for frame in publish(sender, first, blobs) {
        assert_eq!(second.ingest(frame), Ingest::Applied);
    }
}
#[test]
fn unrelated_human_text_between_signed_share_and_echo_keeps_document_grant_request_and_cold(
) -> R<()> {
    let dir = std::env::temp_dir().join(types::new_id("active-group-share"));
    std::fs::create_dir_all(&dir).map_err(storage)?;
    let op = dir.join("owner.sqlite").to_string_lossy().into_owned();
    let ap = dir.join("agent.sqlite").to_string_lossy().into_owned();
    let (mut owner, author, owner_secret) = member(&op);
    let (mut human, _, _) = member(":memory:");
    let mut agent = Engine::open(&ap)?;
    let root = [97; 32];
    let agent_id = Signer::from_secret(&root).id();
    let proof = event_log::agent_owner::authorize(&author, &agent_id)
        .ok_or_else(|| storage("owner proof"))?;
    let (_, agent_secret, _) = agent.create_agent_account(
        "Active group agent",
        "active_group_agent",
        "http://relay.test",
        root,
        &author.identity,
        proof,
    )?;
    let profiles: Vec<_> = [&owner, &human, &agent]
        .iter()
        .map(|e| e.identities[&e.me_id().unwrap()].clone())
        .collect();
    for e in [&mut owner, &mut human, &mut agent] {
        e.put_profiles(profiles.clone())?;
    }
    for frame in claim_group(&mut owner, ROOM, std::slice::from_mut(&mut human)) {
        assert_eq!(human.ingest(frame.clone()), Ingest::Applied);
        assert_eq!(agent.ingest(frame), Ingest::Applied);
    }
    let human_id = human.me_id()?;
    let records = claimed_records(
        std::slice::from_mut(&mut human),
        std::slice::from_ref(&human_id),
    );
    owner.commit_now(ROOM, &[records[0].data.clone()], &Default::default())?;
    let mut blobs = HashMap::new();
    fanout(&mut owner, &mut human, &mut agent, &mut blobs);
    let initial = owner.app_binary_create_format(
        Format::Wordcraft,
        ROOM,
        "Existing active Word",
        &base64::engine::general_purpose::STANDARD.encode(wordcraft::EMPTY),
        "active_word",
    )?;
    let app = owner.app_binary_save(
        &initial.item_id,
        1,
        &initial.sha256,
        &base64::engine::general_purpose::STANDARD.encode(EDITED),
        "active_word_two",
        &event_log::sha256_hex(b"active word two"),
    )?;
    fanout(&mut owner, &mut human, &mut agent, &mut blobs);
    receive_wanted(&mut human, &blobs)?;
    assert!(agent.item_state(&app.item_id).is_err());
    let aa = agent
        .net
        .author
        .clone()
        .ok_or_else(|| storage("agent author"))?;
    add_member(&mut owner, &author, ROOM, &aa);
    let e = owner.logs[ROOM]
        .events()
        .last()
        .ok_or_else(|| storage("MemberAdded"))?;
    let added = ordered(&human, Envelope::plain(e));
    assert_eq!(human.ingest(added.clone()), Ingest::Applied);
    assert_eq!(agent.ingest(added), Ingest::Applied);
    let records = claimed_records(
        std::slice::from_mut(&mut agent),
        std::slice::from_ref(&agent_id),
    );
    owner.commit_now(ROOM, &[records[0].data.clone()], &Default::default())?;
    fanout(&mut owner, &mut human, &mut agent, &mut blobs);
    assert!(agent.item_state(&app.item_id).is_err());
    let account = owner.account().ok_or_else(|| storage("owner account"))?;
    let input = AppEditRequestInput {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: ROOM.into(),
        item_id: app.item_id.clone(),
        expected_version: 2,
        expected_sha256: app.sha256.clone(),
        text: "Review this exact current Word".into(),
        mutation_id: "active_group_request".into(),
        share_current: true,
    };
    let request = owner.request_app_edit(input.clone())?;
    let share = owner
        .store
        .outbox()?
        .into_iter()
        .find_map(|r| match r.event.body {
            EventBody::AppCurrentShared { share } => Some((r.event.client_id, *share)),
            _ => None,
        })
        .ok_or_else(|| storage("actual signed share"))?;
    owner.mls_seal_outbox()?;
    let publications = owner.outbox_envelopes();
    assert_eq!(publications.len(), 2);
    // This is another genuine human/device/member with its own sender ratchet.
    // Only an ordinary text is sequenced first; App, capability and epoch do not change.
    human.send_message(ROOM, "A harmless message while the App remains open")?;
    human.mls_seal_outbox()?;
    let text = human.outbox_envelopes();
    assert_eq!(text.len(), 1);
    let text = ordered(&human, text[0].clone());
    assert_eq!(human.ingest(text.clone()), Ingest::Confirmed);
    assert_eq!(owner.ingest(text.clone()), Ingest::Applied);
    assert_eq!(agent.ingest(text), Ingest::Applied);
    for env in publications {
        let frame = ordered(&owner, env);
        assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
        assert_eq!(human.ingest(frame.clone()), Ingest::Applied);
        assert_eq!(agent.ingest(frame), Ingest::Applied);
    }
    receive_wanted(&mut agent, &blobs)?;
    let owner_applied = owner
        .space_state(ROOM)?
        .entries
        .iter()
        .any(|e| e.id() == share.0);
    let agent_applied = agent
        .space_state(ROOM)?
        .entries
        .iter()
        .any(|e| e.id() == share.0);
    let owner_grant = owner
        .state
        .grants
        .iter()
        .any(|g| g.grant.id == share.1.grant.id && !g.revoked);
    let agent_grant = agent
        .state
        .grants
        .iter()
        .any(|g| g.grant.id == share.1.grant.id && !g.revoked);
    let document = agent.app_binary_read(&app.item_id, Some(2), Some(&app.sha256));
    let scoped_request = agent.artifact_request(ROOM, &request.id);
    assert!(owner_applied && agent_applied && owner_grant && agent_grant && document.is_ok() && scoped_request.as_ref().is_ok_and(|r| r.is_some()),
        "ordinary text invalidated signed share: owner_applied={owner_applied} agent_applied={agent_applied} owner_grant={owner_grant} agent_grant={agent_grant} document={document:?} request={scoped_request:?}");
    assert_eq!(document?.data_base64, app.data_base64);
    assert_eq!(owner.outbox_len(), 0);
    let grants = owner.state.grants.len();
    assert_eq!(owner.request_app_edit(input.clone())?.id, request.id);
    assert_eq!(owner.state.grants.len(), grants);
    drop((owner, agent, human));
    let mut owner = Engine::open(&op)?;
    let mut agent = Engine::open(&ap)?;
    assert!(owner.unlock(Some(owner_secret.to_vec()))?);
    assert!(agent.unlock(Some(agent_secret.to_vec()))?);
    assert_eq!(owner.request_app_edit(input)?.id, request.id);
    assert_eq!(owner.outbox_len(), 0);
    assert!(owner
        .space_state(ROOM)?
        .entries
        .iter()
        .any(|e| e.id() == share.0));
    assert_eq!(
        agent
            .app_binary_read(&app.item_id, Some(2), Some(&app.sha256))?
            .data_base64,
        app.data_base64
    );
    assert!(agent.artifact_request(ROOM, &request.id)?.is_some());
    assert!(agent.app_binary_read(&app.item_id, Some(1), None).is_err());
    assert_eq!(agent.trust(&agent_id, ROOM), types::TrustLevel::Suggest);
    drop((owner, agent));
    std::fs::remove_dir_all(dir).map_err(storage)?;
    Ok(())
}

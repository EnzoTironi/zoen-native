//! One genuine unrelated human message between a signed share and relay echo.
use super::*;
use crate::app_edit_requests::AppEditRequestInput;
const ROOM: &str = "active_group_current_share";
fn fanout(
    sender: &mut Engine,
    first: &mut Engine,
    second: &mut Engine,
    blobs: &mut HashMap<String, Vec<u8>>,
) -> R<()> {
    for frame in publish(sender, first, blobs)? {
        assert_eq!(second.ingest(frame), Ingest::Applied);
    }
    Ok(())
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Change {
    Text,
    OtherShare,
    Head,
    Revocation,
    Membership,
    Epoch,
}
fn exercise(change: Change) -> R<()> {
    let dir = std::env::temp_dir().join(types::new_id("active-group-share")?);
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
    for frame in claim_group(&mut owner, ROOM, std::slice::from_mut(&mut human)).map_err(storage)? {
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
    fanout(&mut owner, &mut human, &mut agent, &mut blobs)?;
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
    fanout(&mut owner, &mut human, &mut agent, &mut blobs)?;
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
    let added = ordered(&human, Envelope::plain(e)?).map_err(storage)?;
    assert_eq!(human.ingest(added.clone()), Ingest::Applied);
    assert_eq!(agent.ingest(added), Ingest::Applied);
    let records = claimed_records(
        std::slice::from_mut(&mut agent),
        std::slice::from_ref(&agent_id),
    );
    owner.commit_now(ROOM, &[records[0].data.clone()], &Default::default())?;
    fanout(&mut owner, &mut human, &mut agent, &mut blobs)?;
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
    let original_permission = owner
        .state
        .app_trust_baseline(&app.item_id)
        .ok_or_else(|| storage("original App permission"))?
        .grant
        .id
        .clone();
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
    // Actual independently signed publications move only the canonical prefix.
    // No authority, group state or settled result is fabricated for a control.
    match change {
        Change::Text => {
            human.send_message(ROOM, "A harmless message while the App remains open")?;
            fanout(&mut human, &mut owner, &mut agent, &mut blobs)?;
        }
        Change::OtherShare => {
            let other = human.app_binary_create_format(
                Format::Wordcraft,
                ROOM,
                "Another actual App",
                &base64::engine::general_purpose::STANDARD.encode(wordcraft::EMPTY),
                "other_actual_app",
            )?;
            fanout(&mut human, &mut owner, &mut agent, &mut blobs)?;
            let a = human
                .account()
                .ok_or_else(|| storage("other sharer account"))?;
            let c = human.app_current_share_context(AppShareScope {
                expected_identity_id: a.identity.clone(),
                expected_device_id: a.device.clone(),
                space_id: ROOM.into(),
                item_id: other.item_id,
                target_id: agent_id.clone(),
            })?;
            assert!(human.share_app_current(c, "other_explicit_share")?.applied);
            fanout(&mut human, &mut owner, &mut agent, &mut blobs)?;
        }
        Change::Head => {
            let next = human.app_binary_save(
                &app.item_id,
                2,
                &app.sha256,
                &base64::engine::general_purpose::STANDARD.encode(wordcraft::EMPTY),
                "human_changed_head",
                &event_log::sha256_hex(b"changed head"),
            )?;
            assert_eq!(next.version, 3);
            fanout(&mut human, &mut owner, &mut agent, &mut blobs)?;
        }
        Change::Revocation => {
            owner.revoke_app_device(&original_permission)?;
            let id = owner.store.outbox()?.into_iter().find(|r| matches!(&r.event.body, EventBody::GrantRevoked { grant } if grant == &original_permission)).ok_or_else(|| storage("normal revocation intent"))?.event.client_id;
            owner.mls_seal_outbox()?;
            let env = owner
                .outbox_envelopes()
                .into_iter()
                .find(|e| e.client_id() == id)
                .ok_or_else(|| storage("actual sealed revocation"))?;
            let frame = ordered(&owner, env).map_err(storage)?;
            assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
            assert_eq!(human.ingest(frame.clone()), Ingest::Applied);
            assert_eq!(agent.ingest(frame), Ingest::Applied);
        }
        Change::Membership => {
            owner.remove_member(ROOM, &human_id)?;
            let row = owner.store.outbox()?.into_iter().find(|r| matches!(&r.event.body, EventBody::MemberRemoved { identity } if identity == &human_id)).ok_or_else(|| storage("normal member removal"))?;
            let frame = ordered(&owner, Envelope::plain(&row.event)?).map_err(storage)?;
            assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
            assert_eq!(human.ingest(frame.clone()), Ingest::Applied);
            assert_eq!(agent.ingest(frame), Ingest::Applied);
        }
        Change::Epoch => {
            let before = human
                .mls_status(ROOM)
                .ok_or_else(|| storage("old epoch"))?
                .0;
            // Normal recovery commit creation needs the other members' public,
            // signed agreement records; no private key or group state is copied.
            let mut records = Vec::new();
            for peer in [&mut owner, &mut agent] {
                let public = peer
                    .net
                    .profiles
                    .agreement
                    .as_ref()
                    .ok_or_else(|| storage("genuine account agreement key"))?
                    .public_hex();
                let signer = peer
                    .net
                    .author
                    .as_ref()
                    .ok_or_else(|| storage("genuine account author"))?;
                let signed = event_log::profile::sign_agreement(signer, &public)
                    .ok_or_else(|| storage("genuine public agreement signature"))?;
                records.push(protocol::AgreementKeyRecord {
                    identity: peer.me_id()?,
                    public,
                    signed,
                });
            }
            let asked: Vec<_> = records
                .iter()
                .map(|record| record.identity.clone())
                .collect();
            human.agreement_keys_arrived(records, &asked);
            human.commit_now(ROOM, &[], &Default::default())?;
            fanout(&mut human, &mut owner, &mut agent, &mut blobs)?;
            assert!(
                human
                    .mls_status(ROOM)
                    .ok_or_else(|| storage("actual new epoch"))?
                    .0
                    > before
            );
        }
    }
    for env in publications {
        let frame = ordered(&owner, env).map_err(storage)?;
        assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
        assert_eq!(human.ingest(frame.clone()), Ingest::Applied);
        assert_eq!(agent.ingest(frame), Ingest::Applied);
    }
    receive_wanted(&mut agent, &blobs)?;
    let owner_applied = owner
        .space_state(ROOM)?
        .entries
        .iter()
        .any(|e| e.id() == share.0 && !e.hash.is_empty());
    let agent_applied = agent
        .space_state(ROOM)?
        .entries
        .iter()
        .any(|e| e.id() == share.0 && !e.hash.is_empty());
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
    if !matches!(change, Change::Text | Change::OtherShare) {
        assert!(!owner_applied && !agent_applied && !owner_grant && !agent_grant,
            "changed context must reject snapshot and grant including owner echo: {change:?} owner={owner_applied}/{owner_grant} agent={agent_applied}/{agent_grant}");
        assert!(
            document.is_err() && scoped_request.is_err(),
            "changed source must not become dispatchable: {change:?}"
        );
        drop((owner, agent, human));
        let mut owner = Engine::open(&op)?;
        let mut agent = Engine::open(&ap)?;
        assert!(owner.unlock(Some(owner_secret.to_vec()))?);
        assert!(agent.unlock(Some(agent_secret.to_vec()))?);
        for e in [&owner, &agent] {
            assert!(!e
                .state
                .grants
                .iter()
                .any(|g| g.grant.id == share.1.grant.id));
            assert!(!e
                .space_state(ROOM)?
                .entries
                .iter()
                .any(|entry| entry.id() == share.0));
        }
        assert!(agent.artifact_request(ROOM, &request.id).is_err());
        drop((owner, agent));
        std::fs::remove_dir_all(dir).map_err(storage)?;
        return Ok(());
    }
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
        .any(|e| e.id() == share.0 && !e.hash.is_empty()));
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

#[test]
fn unrelated_human_text_between_signed_share_and_echo_keeps_document_grant_request_and_cold(
) -> R<()> {
    exercise(Change::Text)
}
#[test]
fn another_app_can_be_shared_without_locking_this_app_to_the_old_conversation_tip() -> R<()> {
    exercise(Change::OtherShare)
}
#[test]
fn real_head_change_after_signing_rejects_share_and_grant_including_owner_echo_and_cold() -> R<()> {
    exercise(Change::Head)
}
#[test]
fn real_revocation_after_signing_never_revives_the_share_grant_or_dispatches_the_request() -> R<()>
{
    exercise(Change::Revocation)
}
#[test]
fn real_membership_change_after_signing_rejects_share_and_grant_even_with_readable_ciphertext(
) -> R<()> {
    exercise(Change::Membership)
}
#[test]
fn actual_mls_epoch_change_after_signing_rejects_old_share_including_retained_owner_echo() -> R<()>
{
    exercise(Change::Epoch)
}

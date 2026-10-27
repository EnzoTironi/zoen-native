//! One disclosed send, using the same outbox/savepoint and canonical receipts.
use super::*;
use crate::app_edit_requests::AppEditRequestInput;
pub(super) fn input(f: &LateWord, id: &str, share: bool) -> R<AppEditRequestInput> {
    let account = f.owner.account().ok_or_else(|| storage("owner account"))?;
    Ok(AppEditRequestInput {
        target_agent_id: None,
        expected_target_revision: None,
        expected_resource_uri: None,
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: SPACE.into(),
        item_id: f.original.item_id.clone(),
        expected_version: f.original.version,
        expected_sha256: f.original.sha256.clone(),
        text: "Review the current shared Word".into(),
        mutation_id: id.into(),
        share_current: share,
    })
}
#[test]
fn one_disclosed_send_publishes_share_before_scoped_request_with_no_extra_retry_effect() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    let actor = f.agent.me_id()?;
    f.owner
        .set_trust(&actor, SPACE, types::TrustLevel::Suggest)?;
    publish(&mut f.owner, &mut f.agent, &mut f.blobs)?;
    let mut command = input(&f, "one_disclosed_send", true)?;
    let targets = f.owner.app_edit_targets(&(&command).into())?;
    command.target_agent_id = Some(f.agent.me_id()?);
    command.expected_target_revision = Some(targets.revision);
    let before = counts(&f.owner);
    let first = f.owner.request_app_edit(command.clone())?;
    assert_eq!(f.owner.outbox_len(), 2);
    assert_eq!(f.owner.state.grants.len(), before.3 + 1);
    let pending = counts(&f.owner);
    assert_eq!(f.owner.request_app_edit(command.clone())?.id, first.id);
    assert_eq!(counts(&f.owner), pending);
    let frames = publish(&mut f.owner, &mut f.agent, &mut f.blobs)?;
    receive_wanted(&mut f.agent, &f.blobs)?;
    let bodies: Vec<_> = frames
        .iter()
        .filter_map(|frame| {
            f.agent.logs[SPACE]
                .events()
                .iter()
                .find(|e| e.hash == frame.hash)
        })
        .collect();
    assert_eq!(bodies.len(), 2);
    assert!(matches!(bodies[0].body, EventBody::AppCurrentShared { .. }));
    assert!(matches!(bodies[1].body, EventBody::AppEditRequested { .. }));
    let request = f
        .agent
        .artifact_request(SPACE, &first.id)?
        .ok_or_else(|| storage("scoped request survives share"))?;
    assert_eq!(request.item_id, f.original.item_id);
    assert_eq!(request.agent_id.as_deref(), Some(f.agent.me_id()?.as_str()));
    assert_eq!(request.version, 2);
    assert_eq!(
        f.agent
            .app_binary_read(&request.item_id, Some(2), Some(&request.sha256))?
            .data_base64,
        f.original.data_base64
    );
    assert_eq!(
        f.agent.trust(&f.agent.me_id()?, SPACE),
        types::TrustLevel::Suggest
    );
    let owner = f.owner.me_id()?;
    assert!(f
        .agent
        .agent_binary_authority(&request.item_id, &first.id, &owner, true)
        .is_ok());
    revoke(&mut f)?;
    let revoked = counts(&f.owner);
    assert_eq!(
        f.owner.request_app_edit(command)?.id,
        first.id,
        "request receipt returns before any sharing/admission"
    );
    assert_eq!(counts(&f.owner), revoked);
    Ok(())
}

#[test]
fn second_append_storage_failure_rolls_back_the_normal_share_and_grant_too() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    let command = input(&f, "atomic_disclosed_send", true)?;
    let before = counts(&f.owner);
    f.owner.store.conn().execute_batch("CREATE TEMP TRIGGER reject_request BEFORE INSERT ON outbox WHEN NEW.json LIKE '%AppEditRequested%' BEGIN SELECT RAISE(ABORT,'injected second canonical append failure'); END;").map_err(storage)?;
    assert!(f.owner.request_app_edit(command.clone()).is_err());
    assert_eq!(counts(&f.owner), before);
    assert!(f.agent.item_state(&f.original.item_id).is_err());
    f.owner
        .store
        .conn()
        .execute_batch("DROP TRIGGER reject_request")
        .map_err(storage)?;
    f.owner.request_app_edit(command)?;
    assert_eq!(f.owner.outbox_len(), 2);
    assert_eq!(f.owner.state.grants.len(), before.3 + 1);
    Ok(())
}

#[test]
fn accepted_share_prefix_then_real_cold_retry_keeps_one_share_and_original_pending_request() -> R<()>
{
    let dir = std::env::temp_dir().join(types::new_id("disclosed-send-prefix")?);
    std::fs::create_dir_all(&dir).map_err(storage)?;
    let op = dir.join("owner.sqlite").to_string_lossy().into_owned();
    let ap = dir.join("agent.sqlite").to_string_lossy().into_owned();
    let mut f = late_word(&op, &ap)?;
    let command = input(&f, "cold_disclosed_send", true)?;
    let first = f.owner.request_app_edit(command.clone())?;
    f.owner.mls_seal_outbox()?;
    let envelopes = f.owner.outbox_envelopes();
    assert_eq!(envelopes.len(), 2);
    let frame = ordered(&f.owner, envelopes[0].clone()).map_err(storage)?;
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Confirmed);
    assert_eq!(f.agent.ingest(frame), Ingest::Applied);
    assert_eq!(f.owner.outbox_len(), 1);
    let owner_secret = f.owner_secret;
    drop(f.owner);
    let mut owner = Engine::open(&op)?;
    assert!(owner.unlock(Some(owner_secret.to_vec()))?);
    let before = counts(&owner);
    assert_eq!(owner.request_app_edit(command)?.id, first.id);
    assert_eq!(counts(&owner), before);
    publish(&mut owner, &mut f.agent, &mut f.blobs)?;
    receive_wanted(&mut f.agent, &f.blobs)?;
    assert_eq!(owner.outbox_len(), 0);
    assert_eq!(
        owner.logs[SPACE]
            .events()
            .iter()
            .filter(|e| matches!(e.body, EventBody::AppCurrentShared { .. }))
            .count(),
        1
    );
    assert_eq!(
        owner.logs[SPACE]
            .events()
            .iter()
            .filter(|e| matches!(e.body, EventBody::AppEditRequested { .. }))
            .count(),
        1
    );
    assert!(f.agent.artifact_request(SPACE, &first.id)?.is_some());
    drop((owner, f.agent));
    std::fs::remove_dir_all(dir).map_err(storage)?;
    Ok(())
}

#[test]
fn omitted_disclosure_keeps_the_existing_reply_payload_and_never_issues_share() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    f.owner
        .request_app_edit(input(&f, "legacy_no_disclosure", false)?)?;
    let rows = f.owner.store.outbox()?;
    assert_eq!(rows.len(), 1);
    let EventBody::AppEditRequested { reply, .. } = &rows[0].event.body else {
        panic!("normal request")
    };
    assert_eq!(reply.to, f.original_root.id);
    assert!(reply.thread);
    let wire = serde_json::to_string(&rows[0].event.body).map_err(storage)?;
    assert!(!wire.contains("share_current"));
    assert!(!wire.contains("AppCurrentShared"));
    Ok(())
}

#[test]
fn scoped_owned_picker_is_read_only_for_member_and_direct_without_making_direct_addable() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    let a = f.owner.account().ok_or_else(|| storage("account"))?;
    let (identity, device) = (a.identity.clone(), a.device.clone());
    let id = f.agent.me_id()?;
    let before = counts(&f.owner);
    let found = f
        .owner
        .member_candidates_mode(SPACE, "", &identity, &device, true)?;
    assert_eq!(
        found[0].member_id, id,
        "actual ready reader comes before the genuine personal Agent candidate"
    );
    assert!(found.iter().all(|c| c.owned_agent));
    assert!(found[0].owned_agent && found[0].delivery_ready);
    assert!(!found[0].can_add);
    assert_eq!(counts(&f.owner), before);
    assert!(f
        .owner
        .member_candidates_mode(SPACE, "", &identity, "foreign_device", true)
        .is_err());
    let author = f
        .owner
        .net
        .author
        .clone()
        .ok_or_else(|| storage("signer"))?;
    ingest_signed(
        &mut f.owner,
        &author,
        SPACE,
        EventBody::MemberAdded {
            identity: identity.clone(),
            role: types::Role::Member,
        },
    )
    .map_err(storage)?;
    assert!(f
        .owner
        .member_candidates_mode(SPACE, "", &identity, &device, false)
        .is_err());
    let found = f
        .owner
        .member_candidates_mode(SPACE, "", &identity, &device, true)?;
    assert!(found[0].owned_agent && found[0].delivery_ready);
    assert!(!found[0].can_add && !found[0].can_manage_members);
    let direct = f.owner.create_synced_space(
        "Existing agent DM",
        SpaceKind::Direct,
        Privacy::Closed,
        std::slice::from_ref(&id),
    )?;
    publish(&mut f.owner, &mut f.agent, &mut f.blobs)?;
    let before = (f.owner.store.event_count()?, f.owner.outbox_len());
    let found = f
        .owner
        .member_candidates_mode(&direct, "", &identity, &device, true)?;
    assert!(found[0].owned_agent && found[0].delivery_ready);
    assert!(!found[0].can_add && !found[0].can_remove && !found[0].can_manage_members);
    assert!(f
        .owner
        .member_candidates_mode(&direct, "", &identity, &device, false)
        .is_err());
    assert_eq!((f.owner.store.event_count()?, f.owner.outbox_len()), before);
    Ok(())
}

#[test]
fn missing_owned_agent_disclosed_send_has_closed_conflict_without_share_or_request() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    let command = input(&f, "missing_agent_send", true)?;
    let author = f
        .owner
        .net
        .author
        .clone()
        .ok_or_else(|| storage("owner signer"))?;
    let id = f.agent.me_id()?;
    ingest_signed(
        &mut f.owner,
        &author,
        SPACE,
        EventBody::MemberRemoved { identity: id },
    )
    .map_err(storage)?;
    let before = counts(&f.owner);
    assert!(matches!(
        f.owner.app_edit_conflict_kind(&command),
        crate::app_edit_requests::AppEditConflictKind::MissingOwnedAgent
    ));
    assert!(matches!(
        f.owner.request_app_edit(command),
        Err(crate::CoreError::Stale { .. })
    ));
    assert_eq!(counts(&f.owner), before);
    Ok(())
}

#[test]
fn verified_owned_picker_prioritizes_any_ready_before_the_hundred_candidate_bound() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    let a = f.owner.account().ok_or_else(|| storage("account"))?;
    let (owner, device) = (a.identity.clone(), a.device.clone());
    let ready = f.agent.me_id()?;
    for n in 0..101 {
        let id = f.owner.create_identity(
            types::Identity {
                id: String::new(),
                owner_proof: None,
                kind: types::IdentityKind::Agent,
                name: format!("Owned candidate {n}"),
                handle: format!("owned_candidate_{n}"),
                tint_hex: "#7C5CFF".into(),
                glyph: None,
                owner: Some(owner.clone()),
                bio: String::new(),
            },
            true,
        )?;
        assert!(f.owner.verified_owned_agent(&id, &owner));
    }
    // Directory arrival/display order is controlled; profiles, membership and
    // MLS are genuine. The only ready Agent starts beyond the UI display cap.
    f.owner.identity_order.retain(|id| id != &ready);
    f.owner.identity_order.push(ready.clone());
    let before = counts(&f.owner);
    let result = f
        .owner
        .member_candidates_mode(SPACE, "", &owner, &device, true)?;
    assert_eq!(result.len(), 100);
    assert_eq!(result[0].member_id, ready);
    assert!(result[0].owned_agent && result[0].delivery_ready);
    assert!(result
        .iter()
        .skip(1)
        .all(|c| c.owned_agent && !c.delivery_ready && c.can_add));
    assert_eq!(counts(&f.owner), before);
    Ok(())
}

#[test]
fn losing_public_owner_proof_invalidates_the_existing_add_member_review() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    let a = f.owner.account().ok_or_else(|| storage("account"))?;
    let (owner, device) = (a.identity.clone(), a.device.clone());
    let candidates = f
        .owner
        .member_candidates_mode(SPACE, "", &owner, &device, true)?;
    let candidate = candidates
        .iter()
        .find(|c| c.can_add && c.owned_agent)
        .ok_or_else(|| storage("genuine owned personal Agent candidate"))?;
    let before = counts(&f.owner);
    // Controlled public profile invalidation, not fake membership/MLS. Persona
    // owner/name/kind remain the same, so only the proof fact changes the review.
    f.owner
        .identities
        .get_mut(&candidate.member_id)
        .ok_or_else(|| storage("profile"))?
        .owner_proof = None;
    assert!(!f.owner.verified_owned_agent(&candidate.member_id, &owner));
    let changed = f
        .owner
        .space_member_context(SPACE, &candidate.member_id, &owner, &device)?;
    assert!(!changed.owned_agent);
    assert_ne!(changed.revision, candidate.revision);
    assert!(f
        .owner
        .add_member_reviewed(
            SPACE,
            &candidate.member_id,
            &owner,
            &device,
            &candidate.revision
        )
        .is_err());
    assert_eq!(counts(&f.owner), before);
    Ok(())
}

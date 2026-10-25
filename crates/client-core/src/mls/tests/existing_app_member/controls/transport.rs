//! A projection refusal cannot retract keys already decrypted by a new reader.
use super::*;

fn broadcast(
    owner: &mut Engine,
    reader: &mut Engine,
    newcomer: &mut Engine,
    blobs: &mut HashMap<String, Vec<u8>>,
) {
    for frame in publish(owner, reader, blobs)? {
        assert_eq!(newcomer.ingest(frame), Ingest::Applied);
    }
}

fn changed_audience_before_sealing(already_sealed: bool) -> R<()> {
    let dir = std::env::temp_dir().join(types::new_id("share-audience-cold")?);
    std::fs::create_dir_all(&dir).map_err(storage)?;
    let path = dir.join("owner.sqlite").to_string_lossy().into_owned();
    let (mut owner, author, owner_secret) = member(&path);
    let (mut reader, _, _) = member(":memory:");
    let (mut newcomer, newcomer_author, _) = member(":memory:");
    let profiles: Vec<_> = [&owner, &reader, &newcomer]
        .into_iter()
        .map(|e| e.me_id().map(|id| e.identities[&id].clone()))
        .collect::<R<_>>()?;
    for e in [&mut owner, &mut reader, &mut newcomer] {
        e.put_profiles(profiles.clone())?;
    }
    for frame in
        claim_group(&mut owner, SPACE, std::slice::from_mut(&mut reader)).map_err(storage)?
    {
        assert_eq!(reader.ingest(frame.clone()), Ingest::Applied);
        assert_eq!(newcomer.ingest(frame), Ingest::Applied);
    }
    let reader_id = reader.me_id()?;
    let records = claimed_records(
        std::slice::from_mut(&mut reader),
        std::slice::from_ref(&reader_id),
    );
    owner.commit_now(SPACE, &[records[0].data.clone()], &Default::default())?;
    let mut blobs = HashMap::new();
    broadcast(&mut owner, &mut reader, &mut newcomer, &mut blobs);
    let app = owner.app_binary_create_format(
        Format::Wordcraft,
        SPACE,
        "App consented before a new member",
        &base64::engine::general_purpose::STANDARD.encode(EDITED),
        "transport_before_join",
    )?;
    broadcast(&mut owner, &mut reader, &mut newcomer, &mut blobs);
    assert!(newcomer.item_state(&app.item_id).is_err());
    let account = owner.account().ok_or_else(|| storage("owner account"))?;
    let context = owner.app_current_share_context(AppShareScope {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: SPACE.into(),
        item_id: app.item_id.clone(),
        target_id: reader_id,
    })?;
    let receipt = owner.share_app_current(context, "consent_old_audience")?;
    assert!(receipt.applied);
    let original = owner
        .store
        .outbox()?
        .into_iter()
        .find(|row| row.event.client_id == receipt.entry.id)
        .ok_or_else(|| storage("genuine pending share"))?
        .event;
    let EventBody::AppCurrentShared { share } = &original.body else {
        return Err(storage("normal share payload"));
    };
    let original_file = share
        .content
        .document
        .clone()
        .ok_or_else(|| storage("shared file"))?;
    let original_grant = share.grant.id.clone();
    let old_epoch = share.epoch.ok_or_else(|| storage("consented MLS epoch"))?;
    if already_sealed {
        assert!(owner.mls_seal_outbox()?);
        assert_eq!(owner.sealed_epoch_of(&original.client_id), Some(old_epoch));
    } else {
        assert!(owner.sealed_copy(&original).is_none());
    }

    // Normal membership + genuine KeyPackage/Commit/Welcome arrive before this
    // retained intent is sent. Handshakes pass waiting application rows normally.
    add_member(&mut owner, &author, SPACE, &newcomer_author);
    let added = owner.logs[SPACE]
        .events()
        .last()
        .ok_or_else(|| storage("member added"))?;
    let frame = ordered(&reader, Envelope::plain(added)?).map_err(storage)?;
    assert_eq!(reader.ingest(frame.clone()), Ingest::Applied);
    assert_eq!(newcomer.ingest(frame), Ingest::Applied);
    let id = newcomer.me_id()?;
    let records = claimed_records(
        std::slice::from_mut(&mut newcomer),
        std::slice::from_ref(&id),
    );
    owner.commit_now(SPACE, &[records[0].data.clone()], &Default::default())?;
    for _ in 0..5 {
        for (blob, _) in owner.pending_uploads(100) {
            owner.upload_done(&blob);
        }
        let handshakes: Vec<_> = owner
            .outbox_envelopes()
            .into_iter()
            .filter(|e| e.sealed_kind() != Some(SealedKind::Application))
            .collect();
        if handshakes.is_empty() {
            break;
        }
        for env in handshakes {
            let frame = ordered(&owner, env).map_err(storage)?;
            assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
            assert_eq!(reader.ingest(frame.clone()), Ingest::Applied);
            assert_eq!(newcomer.ingest(frame), Ingest::Applied);
        }
    }
    let epoch = owner
        .ready_epoch(SPACE)
        .ok_or_else(|| storage("actual new ready epoch"))?;
    assert!(epoch > old_epoch);
    assert_eq!(newcomer.ready_epoch(SPACE), Some(epoch));
    assert!(newcomer.item_state(&app.item_id).is_err());

    owner.mls_seal_outbox()?;
    let offer = owner
        .outbox_envelopes()
        .into_iter()
        .find(|e| e.client_id() == original.client_id);
    if let Some(env) = offer {
        let frame = ordered(&owner, env).map_err(storage)?;
        assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
        assert_eq!(reader.ingest(frame.clone()), Ingest::Applied);
        assert_eq!(newcomer.ingest(frame), Ingest::Applied);
    }
    // Inspect the real opened signed event, not only the checked Item projection.
    // Never print the generated chunk keys, even on a failing assertion.
    let leaked = newcomer.logs[SPACE].events().iter().find_map(|event| {
        if event.client_id != original.client_id {
            return None;
        }
        match &event.body {
            EventBody::AppCurrentShared { share } => share.content.document.as_ref(),
            _ => None,
        }
    });
    let exact_keys_opened = leaked.is_some_and(|file| file == &original_file);
    let refused = newcomer.item_state(&app.item_id).is_err()
        && !newcomer
            .state
            .grants
            .iter()
            .any(|g| g.grant.id == original_grant);
    assert!(
        refused,
        "consumer must still refuse the stale snapshot and grant"
    );
    assert!(
        !exact_keys_opened,
        "new member decrypted the exact old-audience snapshot and file keys: prior_seal={already_sealed} old_epoch={old_epoch} new_epoch={epoch}; projection_refused={refused}"
    );
    assert!(owner
        .store
        .outbox()?
        .iter()
        .any(|row| row.event.client_id == original.client_id && row.failed));
    drop(owner);
    let mut owner = Engine::open(&path)?;
    assert!(owner.unlock(Some(owner_secret.to_vec()))?);
    owner.mls_seal_outbox()?;
    assert!(owner
        .outbox_envelopes()
        .iter()
        .all(|env| env.client_id() != original.client_id));
    let account = owner.account().ok_or_else(|| storage("reopened owner"))?;
    let fresh = owner.app_current_share_context(AppShareScope {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: SPACE.into(),
        item_id: app.item_id.clone(),
        target_id: reader.me_id()?,
    })?;
    let next = owner.share_app_current(fresh, "new_explicit_audience_review")?;
    assert!(next.applied);
    assert_ne!(next.entry.id, original.client_id);
    broadcast(&mut owner, &mut reader, &mut newcomer, &mut blobs);
    receive_wanted(&mut newcomer, &blobs)?;
    let document = newcomer.app_binary_read(&app.item_id, Some(app.version), Some(&app.sha256))?;
    assert_eq!(
        document.data_base64,
        base64::engine::general_purpose::STANDARD.encode(EDITED)
    );
    assert!(newcomer.logs[SPACE]
        .events()
        .iter()
        .all(|event| event.client_id != original.client_id));
    drop((owner, reader, newcomer));
    std::fs::remove_dir_all(dir).map_err(storage)?;
    Ok(())
}

#[test]
fn pending_share_is_not_first_sealed_to_a_new_member_after_consent() -> R<()> {
    changed_audience_before_sealing(false)
}

#[test]
fn pending_share_is_not_resealed_to_a_new_member_after_consent() -> R<()> {
    changed_audience_before_sealing(true)
}

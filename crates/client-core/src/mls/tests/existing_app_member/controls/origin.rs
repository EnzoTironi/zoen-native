//! Origin is public envelope metadata; file bytes and past manifests stay private.
use super::*;
fn forget_creation_receipt(f: &LateWord) -> R<()> {
    let a = f.owner.account().ok_or_else(|| storage("owner account"))?;
    let bytes = serde_json::to_vec(&(&a.identity, &a.device, SPACE, Format::Wordcraft.id()))
        .map_err(storage)?;
    f.owner.store.meta_delete(&format!(
        "app_document_create_receipts:{}",
        event_log::sha256_hex(&bytes)
    ))?;
    Ok(())
}
#[test]
fn legacy_creator_receipt_shares_only_current_bytes_and_prior_share_preserves_public_origin(
) -> R<()> {
    let mut f = late_word_mode(":memory:", ":memory:", true)?;
    let id = f.original.item_id.clone();
    assert!(f
        .owner
        .item_state(&id)?
        .current()
        .content
        .owned_file()
        .unwrap()
        .app_origin
        .is_none());
    receipt(&mut f, "legacy_current_only")?;
    publish(&mut f.owner, &mut f.agent, &mut f.blobs)?;
    receive_wanted(&mut f.agent, &f.blobs)?;
    assert_eq!(
        f.agent
            .app_binary_read(&id, Some(2), Some(&f.original.sha256))?
            .data_base64,
        f.original.data_base64
    );
    assert!(f.agent.app_binary_read(&id, Some(1), None).is_err());
    assert!(f
        .agent
        .item_state(&id)?
        .current()
        .content
        .owned_file()
        .unwrap()
        .app_origin
        .is_some());
    forget_creation_receipt(&f)?;
    // Full readers (including the creator after local receipt eviction) retain
    // the public proof from the same canonical share, without an extra journal.
    let c = context(&f)?;
    assert!(f.owner.share_app_current(c, "legacy_share_again")?.applied);
    Ok(())
}
#[test]
fn legacy_without_receipt_or_public_proof_is_explicitly_unavailable_without_writes() -> R<()> {
    let f = late_word_mode(":memory:", ":memory:", true)?;
    forget_creation_receipt(&f)?;
    let before = counts(&f.owner);
    let result = context(&f);
    assert!(matches!(result, Err(crate::CoreError::Stale { .. })));
    assert_eq!(counts(&f.owner), before);
    assert_eq!(
        f.owner
            .app_binary_read(&f.original.item_id, Some(2), None)?
            .data_base64,
        f.original.data_base64
    );
    assert!(f.agent.item_state(&f.original.item_id).is_err());
    Ok(())
}

fn fanout(
    sender: &mut Engine,
    first: &mut Engine,
    second: &mut Engine,
    blobs: &mut HashMap<String, Vec<u8>>,
) -> R<()> {
    let frames = publish(sender, first, blobs)?;
    for frame in frames {
        assert_eq!(second.ingest(frame), Ingest::Applied);
    }
    Ok(())
}
#[test]
fn genuine_non_creator_writer_reuses_new_app_proof_without_creators_local_receipt() -> R<()> {
    let space = "non_creator_writer_origin";
    let (mut owner, author, _) = member(":memory:");
    let (writer, _, _) = member(":memory:");
    let root = [96; 32];
    let agent_id = Signer::from_secret(&root).id();
    let proof = event_log::agent_owner::authorize(&author, &agent_id)
        .ok_or_else(|| storage("owner proof"))?;
    let mut agent = Engine::open(":memory:")?;
    agent.create_agent_account(
        "Writer control agent",
        "writer_control_agent",
        "http://relay.test",
        root,
        &author.identity,
        proof,
    )?;
    let mut peers = [writer, agent];
    let profiles: Vec<_> = std::iter::once(&owner)
        .chain(peers.iter())
        .map(|e| e.identities[&e.me_id().unwrap()].clone())
        .collect();
    owner.put_profiles(profiles.clone())?;
    for peer in &mut peers {
        peer.put_profiles(profiles.clone())?;
    }
    for frame in claim_group(&mut owner, space, &mut peers).map_err(storage)? {
        for peer in &mut peers {
            assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
        }
    }
    let ids: Vec<_> = peers.iter().map(|e| e.me_id().unwrap()).collect();
    let records = claimed_records(&mut peers, &ids);
    owner.commit_now(
        space,
        &records.iter().map(|r| r.data.clone()).collect::<Vec<_>>(),
        &Default::default(),
    )?;
    let [mut writer, mut agent] = peers;
    let mut blobs = HashMap::new();
    fanout(&mut owner, &mut writer, &mut agent, &mut blobs)?;
    let created = owner.app_binary_create_format(
        Format::Wordcraft,
        space,
        "Shared writer control",
        &base64::engine::general_purpose::STANDARD.encode(wordcraft::EMPTY),
        "new_origin",
    )?;
    fanout(&mut owner, &mut writer, &mut agent, &mut blobs)?;
    receive_wanted(&mut writer, &blobs)?;
    receive_wanted(&mut agent, &blobs)?;
    let a = owner.account().ok_or_else(|| storage("creator account"))?;
    assert!(writer
        .app_binary_creation_origin(
            Format::Wordcraft,
            space,
            &created.item_id,
            &a.identity,
            &a.device
        )?
        .is_none());
    let inherited = writer
        .item_state(&created.item_id)?
        .current()
        .content
        .owned_file()
        .unwrap()
        .app_origin
        .clone()
        .unwrap();
    assert_eq!(inherited.author, owner.me_id()?);
    let updated = writer.app_binary_save(
        &created.item_id,
        1,
        &created.sha256,
        &base64::engine::general_purpose::STANDARD.encode(EDITED),
        "writer_save",
        &event_log::sha256_hex(b"writer save"),
    )?;
    fanout(&mut writer, &mut owner, &mut agent, &mut blobs)?;
    receive_wanted(&mut agent, &blobs)?;
    assert_eq!(
        writer
            .item_state(&created.item_id)?
            .current()
            .content
            .owned_file()
            .unwrap()
            .app_origin
            .as_ref(),
        Some(&inherited)
    );
    let a = writer.account().ok_or_else(|| storage("writer account"))?;
    let c = writer.app_current_share_context(AppShareScope {
        expected_identity_id: a.identity.clone(),
        expected_device_id: a.device.clone(),
        space_id: space.into(),
        item_id: created.item_id.clone(),
        target_id: agent_id.clone(),
    })?;
    assert!(writer.share_app_current(c, "non_creator_share")?.applied);
    fanout(&mut writer, &mut owner, &mut agent, &mut blobs)?;
    assert_eq!(
        agent
            .app_binary_read(&created.item_id, Some(2), Some(&updated.sha256))?
            .data_base64,
        updated.data_base64
    );
    assert_eq!(agent.trust(&agent_id, space), types::TrustLevel::Suggest);
    // Creating a new Item from the same container regenerates origin; metadata
    // never enters those DOCX/export bytes or carries old Item authority.
    let copied = owner.app_binary_create_format(
        Format::Wordcraft,
        space,
        "New factory Item",
        &updated.data_base64,
        "fresh_new_item",
    )?;
    assert_ne!(copied.item_id, created.item_id);
    let fresh = owner
        .item_state(&copied.item_id)?
        .current()
        .content
        .owned_file()
        .unwrap()
        .app_origin
        .as_ref()
        .unwrap();
    assert_ne!(fresh, &inherited);
    assert_eq!(fresh.mutation, "fresh_new_item");
    assert_eq!(copied.data_base64, updated.data_base64);
    Ok(())
}

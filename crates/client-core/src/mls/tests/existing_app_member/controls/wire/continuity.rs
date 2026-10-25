//! A refused foreign claim must not poison another App's canonical origin.
use super::*;
const OTHER: &str = "actual-foreign-app-origin";

fn create_foreign(f: &mut LateWord, mutation: &str) -> R<crate::AppBinaryDocumentDto> {
    let created = f.owner.app_binary_create_format(
        Format::Wordcraft,
        OTHER,
        "Foreign retained Word",
        &base64::engine::general_purpose::STANDARD.encode(wordcraft::EMPTY),
        mutation,
    )?;
    let updated = f.owner.app_binary_save(
        &created.item_id,
        1,
        &created.sha256,
        &base64::engine::general_purpose::STANDARD.encode(EDITED),
        &format!("{mutation}_two"),
        &event_log::sha256_hex(mutation.as_bytes()),
    )?;
    publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    Ok(updated)
}
fn foreign_word(f: &mut LateWord, partial: bool) -> R<crate::AppBinaryDocumentDto> {
    for frame in claim_group(&mut f.owner, OTHER, &mut []) {
        assert_eq!(f.agent.ingest(frame), Ingest::Applied);
    }
    let before = if partial {
        Some(create_foreign(f, "before_other_join")?)
    } else {
        None
    };
    let owner_author = f
        .owner
        .net
        .author
        .clone()
        .ok_or_else(|| storage("owner author"))?;
    let agent_author = f
        .agent
        .net
        .author
        .clone()
        .ok_or_else(|| storage("agent author"))?;
    add_member(&mut f.owner, &owner_author, OTHER, &agent_author);
    let added = f.owner.logs[OTHER]
        .events()
        .last()
        .ok_or_else(|| storage("MemberAdded"))?;
    assert_eq!(
        f.agent.ingest(ordered(&f.agent, Envelope::plain(added))),
        Ingest::Applied
    );
    let records = claimed_records(
        std::slice::from_mut(&mut f.agent),
        std::slice::from_ref(&agent_author.identity),
    );
    let package = &records
        .first()
        .ok_or_else(|| storage("real foreign group KeyPackage"))?
        .data;
    f.owner
        .commit_now(OTHER, std::slice::from_ref(package), &Default::default())?;
    publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    assert!(f.agent.device()?.has_group(OTHER));
    if let Some(before) = before {
        assert!(f.agent.item_state(&before.item_id).is_err());
        let a = f.owner.account().ok_or_else(|| storage("owner account"))?;
        let c = f.owner.app_current_share_context(AppShareScope {
            expected_identity_id: a.identity.clone(),
            expected_device_id: a.device.clone(),
            space_id: OTHER.into(),
            item_id: before.item_id.clone(),
            target_id: f.agent.me_id()?,
        })?;
        assert!(
            f.owner
                .share_app_current(c, "legitimate_foreign_share")?
                .applied
        );
        publish(&mut f.owner, &mut f.agent, &mut f.blobs);
        receive_wanted(&mut f.agent, &f.blobs)?;
        assert!(f.agent.item_state(&before.item_id)?.shared.is_some());
        assert!(!f.agent.logs[OTHER].events().iter().any(
            |e| matches!(&e.body, EventBody::ItemCreated { item, .. } if item == &before.item_id)
        ));
        Ok(before)
    } else {
        let full = create_foreign(f, "after_other_join")?;
        receive_wanted(&mut f.agent, &f.blobs)?;
        assert!(f.agent.item_state(&full.item_id)?.shared.is_none());
        Ok(full)
    }
}
fn continuity(partial: bool) -> R<()> {
    let dir = std::env::temp_dir().join(types::new_id("foreign-origin-continuity"));
    std::fs::create_dir_all(&dir).map_err(|e| storage(&e.to_string()))?;
    let op = dir.join("owner.sqlite").to_string_lossy().into_owned();
    let ap = dir.join("agent.sqlite").to_string_lossy().into_owned();
    let mut f = late_word(&op, &ap)?;
    let original = foreign_word(&mut f, partial)?;
    let mut bad = template(&mut f)?;
    bad.item = original.item_id.clone();
    bad.grant.scope = types::GrantScope::Item(original.item_id.clone());
    bad.grant.grantee = Some(format!("app:{}", original.item_id));
    let grants = f.agent.state.grants.len();
    let (frame, intent) = sealed_intent(&f, bad, "refused_foreign_origin")?;
    // A malicious but genuinely signed/sealed wire intent is consumed through
    // normal ingestion and peer paths, never an altered Item/MLS state.
    // An injected row is not an ordinary own pending receipt: Applied is exact.
    f.owner.store.outbox_put(&intent)?;
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Applied);
    assert_eq!(f.agent.ingest(frame), Ingest::Applied);
    assert_eq!(f.agent.state.grants.len(), grants);
    assert_eq!(
        f.agent
            .app_binary_read(&original.item_id, Some(2), Some(&original.sha256))?
            .data_base64,
        original.data_base64
    );
    let os = f.owner_secret;
    let as_ = f.agent_secret;
    drop((f.owner, f.agent));
    let mut owner = Engine::open(&op)?;
    let mut agent = Engine::open(&ap)?;
    assert!(owner.unlock(Some(os.to_vec()))?);
    assert!(agent.unlock(Some(as_.to_vec()))?);
    let retained = agent.app_binary_read(&original.item_id, Some(2), Some(&original.sha256));
    assert!(
        retained.is_ok(),
        "refused foreign share poisoned original partial={partial} after cold: {retained:?}"
    );
    let next = agent.app_binary_save(
        &original.item_id,
        2,
        &original.sha256,
        &base64::engine::general_purpose::STANDARD.encode(wordcraft::EMPTY),
        "legitimate_foreign_three",
        &event_log::sha256_hex(b"legitimate foreign version three"),
    );
    assert!(next.is_ok(), "refused foreign share blocked a legitimate original-space edit partial={partial}: {next:?}");
    let next = next?;
    publish(&mut agent, &mut owner, &mut f.blobs);
    receive_wanted(&mut owner, &f.blobs)?;
    let actual = agent.app_binary_read(&original.item_id, Some(3), Some(&next.sha256))?;
    assert_eq!(actual.data_base64, next.data_base64);
    assert_eq!(agent.item(&original.item_id)?.space_id, OTHER);
    assert_eq!(agent.state.grants.len(), grants);
    drop((owner, agent));
    std::fs::remove_dir_all(dir).map_err(|e| storage(&e.to_string()))?;
    Ok(())
}
#[test]
fn refused_share_cannot_poison_full_foreign_app_cold_or_its_next_real_save() -> R<()> {
    continuity(false)
}
#[test]
fn refused_share_cannot_poison_previously_shared_partial_foreign_app_cold_or_next_save() -> R<()> {
    continuity(true)
}

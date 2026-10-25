//! Admission, a retained post-join head, canonical order and actual cold replay.
use super::*;
use crate::app_current_share::{AppShareContext, AppShareScope};

mod legacy;
mod wire;

fn context(f: &LateWord) -> R<AppShareContext> {
    let account = f.owner.account().ok_or_else(|| storage("owner account"))?;
    f.owner.app_current_share_context(AppShareScope {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: SPACE.into(),
        item_id: f.original.item_id.clone(),
        target_id: f.agent.me_id()?,
    })
}

fn counts(e: &Engine) -> (usize, u64, usize, usize) {
    (
        e.logs[SPACE].events().len(),
        e.outbox_len(),
        e.state.items.len(),
        e.state.grants.len(),
    )
}

fn save(
    f: &mut LateWord,
    base: &crate::AppBinaryDocumentDto,
    bytes: &[u8],
    id: &str,
) -> R<crate::AppBinaryDocumentDto> {
    f.owner.app_binary_save(
        &base.item_id,
        base.version,
        &base.sha256,
        &base64::engine::general_purpose::STANDARD.encode(bytes),
        id,
        &event_log::sha256_hex(id.as_bytes()),
    )
}

fn revoke(f: &mut LateWord) -> R<()> {
    let grant = f
        .owner
        .state
        .app_trust_baseline(&f.original.item_id)
        .ok_or_else(|| storage("normal App capability"))?
        .grant
        .id
        .clone();
    f.owner.revoke_app_device(&grant)
}

fn receipt(
    f: &mut LateWord,
    mutation: &str,
) -> R<(AppShareContext, crate::app_current_share::AppShareReceipt)> {
    let c = context(f)?;
    let r = f.owner.share_app_current(c.clone(), mutation)?;
    assert!(r.applied);
    Ok((c, r))
}

#[test]
fn observed_owner_device_membership_head_and_capability_gate_disclosure_without_writes() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    let c = context(&f)?;
    let before = counts(&f.owner);
    for field in ["owner", "device", "target", "space", "item"] {
        let mut bad = c.scope.clone();
        match field {
            "owner" => bad.expected_identity_id = "foreign".into(),
            "device" => bad.expected_device_id = "foreign".into(),
            "target" => bad.target_id = "foreign".into(),
            "space" => bad.space_id = "foreign".into(),
            _ => bad.item_id = "foreign".into(),
        }
        assert!(f.owner.app_current_share_context(bad).is_err(), "{field}");
        assert_eq!(counts(&f.owner), before);
    }
    for field in ["version", "hash", "revision"] {
        let mut bad = c.clone();
        match field {
            "version" => bad.version += 1,
            "hash" => bad.sha256 = "00".repeat(32),
            _ => bad.revision = "00".repeat(32),
        }
        assert!(
            f.owner.share_app_current(bad, "refused").is_err(),
            "{field}"
        );
        assert_eq!(counts(&f.owner), before);
    }
    let base = f.original.clone();
    save(&mut f, &base, wordcraft::EMPTY, "draft")?;
    let pending = counts(&f.owner);
    assert!(
        context(&f).is_err(),
        "pending drafts are not a canonical snapshot"
    );
    assert!(f.owner.share_app_current(c, "stale").is_err());
    assert_eq!(counts(&f.owner), pending);
    publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    revoke(&mut f)?;
    publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    let revoked = counts(&f.owner);
    assert!(context(&f).is_err());
    assert_eq!(counts(&f.owner), revoked);
    Ok(())
}

#[test]
fn signed_receipt_retry_after_revocation_never_reissues_capability_or_overwrites_head() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    let (c, first) = receipt(&mut f, "one_share")?;
    // Timeout while still pending resolves to precisely the original outbox intent.
    let pending = counts(&f.owner);
    assert_eq!(
        f.owner.share_app_current(c.clone(), "one_share")?.entry.id,
        first.entry.id
    );
    assert_eq!(counts(&f.owner), pending);
    let frames = publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    receive_wanted(&mut f.agent, &f.blobs)?;
    revoke(&mut f)?;
    publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    assert!(!f.agent.app_trust_available(&f.original.item_id));
    let before = counts(&f.owner);
    assert_eq!(
        f.owner.share_app_current(c.clone(), "one_share")?.entry.id,
        first.entry.id
    );
    assert_eq!(counts(&f.owner), before);
    assert!(context(&f).is_err());
    let grant_count = f.agent.state.grants.len();
    for frame in frames {
        assert_eq!(f.agent.ingest(frame), Ingest::Duplicate);
    }
    assert_eq!(f.agent.state.grants.len(), grant_count);
    assert!(!f.agent.app_trust_available(&f.original.item_id));
    let mut changed = c;
    changed.sha256 = "00".repeat(32);
    assert!(f.owner.share_app_current(changed, "one_share").is_err());
    assert_eq!(counts(&f.owner), before);
    Ok(())
}

#[test]
fn append_failure_rolls_back_snapshot_capability_and_intent_then_normal_retry_is_single() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    let c = context(&f)?;
    let before = counts(&f.owner);
    f.owner.store.conn().execute_batch("CREATE TEMP TRIGGER reject_share BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT,'injected canonical append failure'); END;").map_err(|e| storage(&e.to_string()))?;
    assert!(f.owner.share_app_current(c.clone(), "storage_cut").is_err());
    assert_eq!(counts(&f.owner), before);
    f.owner
        .store
        .conn()
        .execute_batch("DROP TRIGGER reject_share;")
        .map_err(|e| storage(&e.to_string()))?;
    let r = f.owner.share_app_current(c, "storage_cut")?;
    assert!(r.applied);
    assert_eq!(f.owner.outbox_len(), 1);
    assert_eq!(f.owner.state.grants.len(), before.3 + 1);
    Ok(())
}

#[test]
fn retained_successor_before_share_bootstraps_real_floor_then_cold_read_export_history_restore(
) -> R<()> {
    let dir = std::env::temp_dir().join(types::new_id("current-share-cold"));
    std::fs::create_dir_all(&dir).map_err(|e| storage(&e.to_string()))?;
    let op = dir.join("owner.sqlite").to_string_lossy().into_owned();
    let ap = dir.join("agent.sqlite").to_string_lossy().into_owned();
    let mut f = late_word(&op, &ap)?;
    let base = f.original.clone();
    let v3 = save(&mut f, &base, wordcraft::EMPTY, "retained_successor")?;
    let frames = publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    assert!(frames.iter().any(|frame| f.agent.logs[SPACE]
        .events()
        .iter()
        .any(|e| e.hash == frame.hash
            && matches!(&e.body, EventBody::ItemVersioned { item, .. } if item == &base.item_id))));
    assert!(
        f.agent.item_state(&base.item_id).is_err(),
        "the causal RED prerequisite"
    );
    assert_eq!(context(&f)?.version, 3);
    let (c, first) = receipt(&mut f, "share_real_three")?;
    let shared = publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    receive_wanted(&mut f.agent, &f.blobs)?;
    for e in [&f.owner, &f.agent] {
        assert_eq!(
            e.app_binary_read(&base.item_id, Some(3), Some(&v3.sha256))?
                .data_base64,
            v3.data_base64
        );
        assert_eq!(
            e.file_bytes(&base.item_id, Some(3))?,
            Some(wordcraft::EMPTY.to_vec())
        );
        assert_eq!(
            e.item_discussion(SPACE, &base.item_id)?.root.map(|r| r.id),
            Some(f.original_root.id.clone())
        );
    }
    let detail = f.agent.item(&base.item_id)?;
    assert_eq!(
        detail.versions.iter().map(|v| v.number).collect::<Vec<_>>(),
        vec![3]
    );
    assert_eq!(detail.created_by.id, "");
    assert!(detail.origin.contains("compartilhada") || detail.origin.contains("shared"));
    let no_write = counts(&f.agent);
    for n in [1, 2] {
        assert!(f
            .agent
            .app_binary_read(&base.item_id, Some(n), None)
            .is_err());
        assert!(f.agent.file_bytes(&base.item_id, Some(n)).is_err());
        assert!(f.agent.item_at(&base.item_id, n).is_err());
        assert!(f
            .agent
            .restore_version(&base.item_id, n, "missing past")
            .is_err());
    }
    assert_eq!(counts(&f.agent), no_write);
    let v4 = save(&mut f, &v3, EDITED, "ordinary_four")?;
    publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    receive_wanted(&mut f.agent, &f.blobs)?;
    // Same ordinary restore consumer extends the absolute number, never len()+1.
    let restored = f
        .owner
        .restore_version(&base.item_id, 3, "normal known version restore")?;
    assert_eq!(restored.version, 5);
    publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    assert_eq!(f.agent.item(&base.item_id)?.version, 5);
    assert_eq!(
        f.agent
            .app_binary_read(&base.item_id, Some(4), Some(&v4.sha256))?
            .data_base64,
        v4.data_base64
    );
    assert_eq!(
        f.agent
            .app_binary_read(&base.item_id, Some(5), None)?
            .data_base64,
        v3.data_base64
    );
    let os = f.owner_secret;
    let as_ = f.agent_secret;
    let id = base.item_id.clone();
    let anchor = f.original_root.id.clone();
    drop(f);
    let mut owner = Engine::open(&op)?;
    let mut agent = Engine::open(&ap)?;
    assert!(owner.unlock(Some(os.to_vec()))?);
    assert!(agent.unlock(Some(as_.to_vec()))?);
    assert_eq!(
        agent
            .item(&id)?
            .versions
            .iter()
            .map(|v| v.number)
            .collect::<Vec<_>>(),
        vec![5, 4, 3]
    );
    assert!(agent.item_at(&id, 2).is_err());
    assert_eq!(
        agent.app_binary_read(&id, Some(5), None)?.data_base64,
        v3.data_base64
    );
    assert_eq!(
        agent.item_discussion(SPACE, &id)?.root.map(|r| r.id),
        Some(anchor)
    );
    assert_eq!(
        agent.trust(&agent.me_id()?, SPACE),
        types::TrustLevel::Suggest
    );
    let before = counts(&agent);
    for frame in shared {
        assert_eq!(agent.ingest(frame), Ingest::Duplicate);
    }
    assert_eq!(counts(&agent), before);
    let before = counts(&owner);
    assert_eq!(
        owner.share_app_current(c, "share_real_three")?.entry.id,
        first.entry.id
    );
    assert_eq!(counts(&owner), before);
    drop((owner, agent));
    std::fs::remove_dir_all(dir).map_err(|e| storage(&e.to_string()))?;
    Ok(())
}

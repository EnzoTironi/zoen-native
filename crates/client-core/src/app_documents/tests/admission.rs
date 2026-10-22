//! Whole admission cases on the same original parent fixture.

use super::*;

pub(super) fn corrupt_cached_bytes_fail_closed_and_revoked_device_cannot_read() {
    let f = fixture(":memory:");
    let initial = f.engine.app_document_read(f.item.clone()).unwrap();
    let sha = f
        .engine
        .lock()
        .item_state(&f.item)
        .unwrap()
        .current()
        .content
        .owned_file()
        .unwrap()
        .chunks[0]
        .sha256
        .clone();
    f.engine
        .lock()
        .store
        .conn()
        .execute(
            "UPDATE media SET bytes=?1 WHERE sha256=?2",
            rusqlite::params![b"corrupt".as_slice(), sha],
        )
        .unwrap();
    assert!(f.engine.app_document_read(f.item.clone()).is_err());
    let device = f.engine.account().unwrap().device_id;
    f.engine
        .lock()
        .store
        .set_meta("devices.revoked", &json!([device]).to_string())
        .unwrap();
    assert!(f.engine.app_document_read(f.item.clone()).is_err());
    assert!(save(&f, &initial, &scene("Denied"), "deny").is_err());
}

pub(super) fn revoked_item_grant_prevents_new_save_without_preventing_read_or_accepted_retry() {
    let f = fixture(":memory:");
    let base = f.engine.app_document_read(f.item.clone()).unwrap();
    let accepted = save(&f, &base, &scene("Accepted"), "accepted").unwrap();
    {
        let mut e = f.engine.lock();
        let me = e.me_id().unwrap();
        let grant = e
            .state
            .grants
            .iter()
            .find(|g| g.grant.grantee.as_deref() == Some(format!("app:{}", f.item).as_str()))
            .unwrap()
            .grant
            .id
            .clone();
        e.append(&f.space, &me, EventBody::GrantRevoked { grant })
            .unwrap();
    }
    let before = counts(&f.engine);
    assert!(save(&f, &accepted, &scene("Denied"), "denied").is_err());
    assert_eq!(
        save(&f, &base, &accepted.scene_json, "accepted").unwrap(),
        accepted
    );
    assert_eq!(
        f.engine.app_document_read(f.item.clone()).unwrap(),
        accepted
    );
    assert_eq!(counts(&f.engine), before);
}

pub(super) fn sync_discovers_owned_app_chunks_and_explicit_history_retains_original_versions() {
    let f = fixture(":memory:");
    let base = f.engine.app_document_read(f.item.clone()).unwrap();
    let accepted = save(&f, &base, &scene("Current"), "current").unwrap();
    let mut e = f.engine.lock();
    e.net.synced.insert(f.space.clone());
    e.store.conn().execute_batch("DELETE FROM media").unwrap();
    assert!(e.app_document_read(&f.item).is_err());
    let wanted = e.wanted_item_blobs();
    let current = e
        .item_state(&f.item)
        .unwrap()
        .current()
        .content
        .owned_file()
        .unwrap();
    assert!(current
        .chunks
        .iter()
        .all(|c| wanted.iter().any(|w| w.blob == c.blob
            && w.space == f.space
            && w.maximum_sealed_bytes == Some(c.bytes + 40))));
    let key = e
        .file_demand_key(&f.item, 1, MAX_SCENE_BYTES as u64)
        .unwrap();
    let _lease = e
        .net
        .file_demands
        .acquire(
            key,
            MAX_SCENE_BYTES as u64,
            std::time::Instant::now() + std::time::Duration::from_secs(1),
        )
        .unwrap();
    let wanted = e.wanted_item_blobs();
    let original = e.item_state(&f.item).unwrap().versions[0]
        .content
        .owned_file()
        .unwrap();
    assert!(original
        .chunks
        .iter()
        .all(|c| wanted.iter().any(|w| w.blob == c.blob)));
    assert_eq!(
        e.item_state(&f.item).unwrap().current().number,
        accepted.version
    );
}

pub(super) fn lost_membership_denies_read_copy_and_save_even_when_all_scene_chunks_are_cached() {
    let f = fixture(":memory:");
    let base = f.engine.app_document_read(f.item.clone()).unwrap();
    {
        let mut e = f.engine.lock();
        let me = e.me_id().unwrap();
        e.append(
            &f.space,
            &me,
            EventBody::MemberRemoved {
                identity: me.clone(),
            },
        )
        .unwrap();
    }
    {
        let mut e = f.engine.lock();
        e.net.synced.insert(f.space.clone());
        e.store.conn().execute_batch("DELETE FROM media").unwrap();
        assert!(e
            .wanted_item_blobs()
            .iter()
            .all(|wanted| wanted.space != f.space));
    }
    let before = counts(&f.engine);
    assert!(f.engine.file_bytes(f.item.clone(), Some(1)).is_err());
    assert!(f.engine.file_info_at(f.item.clone(), Some(1)).is_err());
    assert!(f.engine.app_document_read(f.item.clone()).is_err());
    assert!(save(&f, &base, &scene("No access"), "denied").is_err());
    assert!(f
        .engine
        .app_document_copy(f.item.clone(), scene("No access"), "copy".into())
        .is_err());
    assert_eq!(counts(&f.engine), before);
}

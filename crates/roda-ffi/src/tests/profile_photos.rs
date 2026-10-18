use super::*;
use sha2::{Digest, Sha256};

fn account() -> Arc<RodaEngine> {
    let e = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    e.create_account(
        "Ana".into(),
        "ana_photo".into(),
        "http://127.0.0.1:9".into(),
        Arc::new(MemVault::default()),
    )
    .unwrap();
    e
}

#[test]
fn photo_edit_is_atomic_and_publication_waits_for_encrypted_upload() {
    let e = account();
    let id = e.me().unwrap().id;
    let before = e.get_profile(id.clone()).unwrap();
    let bytes = b"bounded photo".to_vec();
    e.lock().store.conn().execute_batch("CREATE TRIGGER fail_photo BEFORE UPDATE ON profile_keys BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(e
        .update_my_profile(
            "Changed".into(),
            "Bio".into(),
            PhotoChange::Set {
                bytes: bytes.clone(),
                mime: "image/jpeg".into()
            }
        )
        .is_err());
    assert_eq!(e.get_profile(id.clone()).unwrap(), before);
    assert_eq!(e.me().unwrap().name, "Ana");
    assert_eq!(e.lock().store.uploads_pending().unwrap(), 0);
    let sha = hex::encode(Sha256::digest(&bytes));
    assert!(e.media(sha.clone()).unwrap().is_none());
    e.lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_photo")
        .unwrap();
    let after = e
        .update_my_profile(
            "Changed".into(),
            "Bio".into(),
            PhotoChange::Set {
                bytes: bytes.clone(),
                mime: "image/jpeg".into(),
            },
        )
        .unwrap();
    assert_eq!(after.version, before.version + 1);
    assert!(after.photo_ready);
    assert_eq!(after.photo_sha256.as_deref(), Some(sha.as_str()));
    assert_eq!(
        e.profile_photo(id.clone(), sha.clone())
            .unwrap()
            .unwrap()
            .bytes,
        bytes
    );
    assert!(e
        .profile_photo(id.clone(), "00".repeat(32))
        .unwrap()
        .is_none());
    assert!(e
        .profile_photo("someone else".into(), sha.clone())
        .unwrap()
        .is_none());
    let (blob, sealed) = e.lock().pending_uploads(1).pop().unwrap();
    {
        let mut engine = e.lock();
        engine.profiles_session_start(true);
        engine.sealed_profiles_arrived(Vec::new(), std::slice::from_ref(&id));
        assert!(engine.profile_to_upload().is_none());
        engine.upload_done(&blob);
        assert!(engine.profile_to_upload().is_some());
        let photo = engine.fields_of(&id).unwrap().1.photo.unwrap();
        assert_eq!(photo.open(&sealed), Some(bytes));
    }
    e.update_my_profile("Changed".into(), "Bio".into(), PhotoChange::Remove)
        .unwrap();
    assert!(
        e.profile_photo(id, sha).unwrap().is_none(),
        "cached bytes cannot override removal"
    );
}

#[test]
fn rejected_photo_size_or_mime_never_changes_profile_fields() {
    let e = account();
    let id = e.me().unwrap().id;
    let before = e.get_profile(id.clone()).unwrap();
    for (bytes, mime) in [
        (Vec::new(), "image/jpeg"),
        (vec![0; 5 * 1024 * 1024 + 1], "image/jpeg"),
        (vec![1], "image/svg+xml"),
    ] {
        assert!(e
            .update_my_profile(
                "Different".into(),
                "Different".into(),
                PhotoChange::Set {
                    bytes,
                    mime: mime.into()
                }
            )
            .is_err());
        assert_eq!(e.get_profile(id.clone()).unwrap(), before);
        assert_eq!(e.lock().store.uploads_pending().unwrap(), 0);
    }
}

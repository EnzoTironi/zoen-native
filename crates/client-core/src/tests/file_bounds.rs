use super::*;
use sha2::{Digest, Sha256};
use types::{ChunkRef, EventBody, FileDoc, ItemContent};

fn file_fixture(thumbnail: Option<Vec<u8>>) -> (Arc<Client>, String, String) {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina").id;
    let file = e
        .file_add(
            space.clone(),
            "media/original.mp3".into(),
            "original.mp3".into(),
            "audio/mpeg".into(),
            vec![1, 2, 3],
            thumbnail,
        )
        .unwrap();
    (e, space, file.id)
}

fn document(e: &Client, item: &str) -> FileDoc {
    let engine = e.lock();
    match &engine.state.items[item].versions.last().unwrap().content {
        ItemContent::File(file) => file.clone(),
        _ => panic!("file fixture"),
    }
}

fn append_document(e: &Client, item: &str, file: FileDoc) {
    let mut engine = e.lock();
    let space = engine.state.items[item].space.clone();
    let me = engine.me_id().unwrap();
    engine
        .append(
            &space,
            &me,
            EventBody::ItemVersioned {
                item: item.into(),
                content: ItemContent::File(file),
                note: "Authenticated file metadata".into(),
            },
        )
        .unwrap();
}

#[test]
fn bounded_reads_and_metadata_select_the_same_historical_version() {
    let (e, space, item) = file_fixture(None);
    e.file_new_version_typed(
        item.clone(),
        vec![4, 5],
        None,
        "Encoded".into(),
        "encoded.m4a".into(),
        "audio/mp4".into(),
    )
    .unwrap();
    let before = e.item(item.clone()).unwrap();
    let original = e.file_info_at(item.clone(), Some(1)).unwrap();
    let latest = e.file_info_at(item.clone(), None).unwrap();
    assert_eq!(
        (
            original.name.as_str(),
            original.mime.as_str(),
            original.bytes
        ),
        ("original.mp3", "audio/mpeg", 3)
    );
    assert_eq!(
        (latest.name.as_str(), latest.mime.as_str(), latest.bytes),
        ("encoded.m4a", "audio/mp4", 2)
    );
    assert_eq!(
        e.file_bytes_bounded(item.clone(), Some(1), 3).unwrap(),
        Some(vec![1, 2, 3])
    );
    assert_eq!(
        e.file_bytes_bounded(item.clone(), None, 2).unwrap(),
        Some(vec![4, 5])
    );
    assert!(e.file_bytes_bounded(item.clone(), Some(1), 2).is_err());
    assert!(e.file_bytes_bounded(item.clone(), None, 0).is_err());
    assert!(e.file_info_at(item.clone(), Some(0)).is_err());
    assert!(e.file_bytes_bounded(item.clone(), Some(99), 3).is_err());
    assert_eq!(e.item(item).unwrap().version, before.version);
    assert!(e.verify_log(space).valid);
}

#[test]
fn authenticated_extreme_sizes_are_refused_by_both_read_apis() {
    let (e, space, item) = file_fixture(None);
    let mut file = document(&e, &item);
    file.bytes = u64::MAX;
    append_document(&e, &item, file);
    assert!(e.file_bytes_bounded(item.clone(), None, u64::MAX).is_err());
    assert!(e.file_bytes(item.clone(), None).is_err());
    assert!(e.file_info_at(item.clone(), None).is_err());
    assert_eq!(
        e.file_bytes_bounded(item, Some(1), 3).unwrap(),
        Some(vec![1, 2, 3])
    );
    assert!(
        e.verify_log(space).valid,
        "invalid read metadata can still be signed"
    );
}

#[test]
fn malformed_chunk_metadata_cannot_allocate_or_open_a_file() {
    let (e, space, item) = file_fixture(None);
    let original = document(&e, &item);
    let mut invalid = Vec::new();
    let mut file = original.clone();
    file.chunks.clear();
    invalid.push(file);
    let mut file = original.clone();
    file.chunks = vec![file.chunks[0].clone(); 514];
    file.bytes = 3 * 514;
    invalid.push(file);
    let mut file = original.clone();
    file.chunks[0].bytes = 0;
    invalid.push(file);
    let mut file = original.clone();
    file.chunks[0].bytes = u64::MAX;
    invalid.push(file);
    let mut file = original.clone();
    file.bytes += 1;
    invalid.push(file);
    for field in ["sha256", "blob", "key"] {
        let mut file = original.clone();
        match field {
            "sha256" => file.chunks[0].sha256 = "g".repeat(64),
            "blob" => file.chunks[0].blob = "0".repeat(63),
            _ => file.chunks[0].key = "0".repeat(65),
        }
        invalid.push(file);
    }
    let mut file = original.clone();
    file.path = "p".repeat(128 << 10);
    invalid.push(file);
    for file in invalid {
        append_document(&e, &item, file);
        assert!(e.file_bytes_bounded(item.clone(), None, 512 << 20).is_err());
        assert!(e.file_bytes(item.clone(), None).is_err());
        assert!(e.file_info_at(item.clone(), None).is_err());
    }
    assert_eq!(
        e.file_bytes_bounded(item, Some(1), 3).unwrap(),
        Some(vec![1, 2, 3])
    );
    assert!(e.verify_log(space).valid);
}

#[test]
fn oversized_cache_rows_are_refused_without_loading_the_blob() {
    let (e, _, item) = file_fixture(None);
    let sha = document(&e, &item).chunks[0].sha256.clone();
    {
        let engine = e.lock();
        engine
            .store
            .conn()
            .execute(
                "UPDATE media SET bytes = zeroblob(1048576) WHERE sha256 = ?1",
                [&sha],
            )
            .unwrap();
        assert_eq!(engine.store.media_bytes_len(&sha).unwrap(), Some(1 << 20));
        assert!(matches!(
            engine.store.media_bytes_bounded(&sha, 3),
            Err(store::StoreError::MediaTooLarge)
        ));
        assert!(engine
            .store
            .media_bytes_bounded("missing", 3)
            .unwrap()
            .is_none());
    }
    assert!(!e.file_info_at(item.clone(), None).unwrap().ready);
    assert!(e.file_bytes_bounded(item.clone(), None, 3).is_err());
    assert!(e.file_bytes(item, None).is_err());
}

#[test]
fn missing_chunks_return_pending_without_changing_history_or_the_outbox() {
    let (e, space, item) = file_fixture(None);
    let sha = document(&e, &item).chunks[0].sha256.clone();
    let (events, pending) = {
        let engine = e.lock();
        engine
            .store
            .conn()
            .execute("DELETE FROM media WHERE sha256 = ?1", [&sha])
            .unwrap();
        (
            engine.store.events(&space).unwrap().len(),
            engine.store.outbox_len().unwrap(),
        )
    };
    assert_eq!(e.file_bytes_bounded(item.clone(), None, 3).unwrap(), None);
    assert_eq!(e.file_bytes(item.clone(), None).unwrap(), None);
    assert!(!e.file_info_at(item.clone(), None).unwrap().ready);
    assert_eq!(e.item(item).unwrap().version, 1);
    let engine = e.lock();
    assert_eq!(engine.store.events(&space).unwrap().len(), events);
    assert_eq!(engine.store.outbox_len().unwrap(), pending);
}

#[test]
fn cached_chunk_corruption_and_a_false_file_digest_are_errors() {
    let (e, _, item) = file_fixture(None);
    let original = document(&e, &item);
    {
        let engine = e.lock();
        engine
            .store
            .conn()
            .execute(
                "UPDATE media SET bytes = ?1 WHERE sha256 = ?2",
                rusqlite::params![vec![9u8; 3], original.chunks[0].sha256],
            )
            .unwrap();
    }
    assert!(e.file_bytes_bounded(item.clone(), None, 3).is_err());
    e.lock()
        .store
        .cache_imported_media(
            &original.chunks[0].sha256,
            "application/octet-stream",
            &[1, 2, 3],
            0,
        )
        .unwrap();
    let mut wrong_digest = original;
    wrong_digest.sha256 = "0".repeat(64);
    append_document(&e, &item, wrong_digest);
    assert!(e.file_bytes_bounded(item.clone(), None, 3).is_err());
    assert_eq!(
        e.file_bytes_bounded(item, Some(1), 3).unwrap(),
        Some(vec![1, 2, 3])
    );
}

#[test]
fn several_chunks_are_reassembled_in_order_with_an_exact_total_limit() {
    let (e, _, item) = file_fixture(None);
    let mut file = document(&e, &item);
    file.chunks.clear();
    for bytes in [&b"abc"[..], &b"de"[..]] {
        let sha256 = hex::encode(Sha256::digest(bytes));
        e.lock()
            .store
            .put_media(&sha256, "application/octet-stream", bytes, 0)
            .unwrap();
        file.chunks.push(ChunkRef {
            sha256,
            bytes: bytes.len() as u64,
            blob: "0".repeat(64),
            key: "0".repeat(64),
        });
    }
    file.bytes = 5;
    file.sha256 = hex::encode(Sha256::digest(b"abcde"));
    append_document(&e, &item, file);
    assert_eq!(
        e.file_bytes_bounded(item.clone(), None, 5).unwrap(),
        Some(b"abcde".to_vec())
    );
    assert!(e.file_bytes_bounded(item, None, 4).is_err());
}

#[test]
fn thumbnails_are_bounded_and_verified_before_returning_bytes() {
    let (e, space, item) = file_fixture(Some(vec![7, 8, 9]));
    assert_eq!(e.file_thumbnail(item.clone()).unwrap(), Some(vec![7, 8, 9]));
    let mut file = document(&e, &item);
    let sha = file.thumb.as_ref().unwrap().sha256.clone();
    {
        let engine = e.lock();
        engine
            .store
            .conn()
            .execute(
                "UPDATE media SET bytes = zeroblob(2097153) WHERE sha256 = ?1",
                [&sha],
            )
            .unwrap();
    }
    assert!(e.file_thumbnail(item.clone()).is_err());
    file.thumb.as_mut().unwrap().bytes = (2 << 20) + 1;
    append_document(&e, &item, file);
    assert!(e.file_thumbnail(item.clone()).is_err());
    assert!(e.file_info_at(item.clone(), None).is_err());
    assert!(e.verify_log(space).valid);
}

#[test]
fn absent_thumbnail_and_missing_thumbnail_payload_are_pending_without_effects() {
    let (e, _, item) = file_fixture(None);
    assert_eq!(e.file_thumbnail(item).unwrap(), None);
    let (e, _, item) = file_fixture(Some(vec![7, 8, 9]));
    let sha = document(&e, &item).thumb.unwrap().sha256;
    e.lock()
        .store
        .conn()
        .execute("DELETE FROM media WHERE sha256 = ?1", [&sha])
        .unwrap();
    assert_eq!(e.file_thumbnail(item.clone()).unwrap(), None);
    assert_eq!(e.item(item).unwrap().version, 1);
}

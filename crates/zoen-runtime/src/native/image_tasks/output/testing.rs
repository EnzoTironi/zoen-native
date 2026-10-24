//! Existing fixture capability only; no production custody or upload authority.
use super::*;
async fn publication(runtime: &RuntimeAuthority, run: &str) -> Publication {
    let trx = runtime.execution.transaction().await.unwrap();
    let bytes = trx
        .get(&runtime.execution.run(run, "image-output/1"), false)
        .await
        .unwrap()
        .unwrap();
    runtime
        .custody
        .open(&format!("native-image-output/1/{run}"), None, &bytes)
        .unwrap()
}
pub(crate) async fn checkpoint(
    runtime: &RuntimeAuthority,
    run: &str,
) -> Option<relay_admission::managed_images::Checkpoint> {
    publication(runtime, run).await.checkpoint
}
pub(crate) async fn transport_original(
    runtime: &RuntimeAuthority,
    run: &str,
) -> (protocol::Envelope, Vec<types::ChunkRef>) {
    let publication = publication(runtime, run).await;
    let protocol::ClientFrame::Publish { env } =
        protocol::ClientFrame::decode(&publication.wire).unwrap()
    else {
        panic!("retained Publish")
    };
    (
        env,
        publication
            .blobs
            .into_iter()
            .map(|reference| reference.chunk)
            .collect(),
    )
}
pub(crate) async fn bytes(runtime: &RuntimeAuthority, run: &str) -> Result<Vec<u8>, RuntimeError> {
    let publication = publication(runtime, run).await;
    let task = runtime
        .execution
        .read_image_task(run, &runtime.custody)
        .await?
        .0
        .task;
    super::super::blobs::read(
        runtime,
        run,
        &publication.blobs,
        &publication.sha256,
        task.request.max_image_bytes,
    )
    .await
}
pub(crate) async fn remove_chunk(runtime: &RuntimeAuthority, run: &str) -> (Vec<u8>, Vec<u8>) {
    let publication = publication(runtime, run).await;
    super::super::blobs::assert_faults(runtime, run, &publication.blobs, &publication.sha256).await;
    let (blob, root) = super::super::blobs::first_payload(&publication.blobs);
    let marker = b"ZOEN-EVIDENCE-CHUNKS/1\0";
    assert!(root.starts_with(marker));
    let manifest: serde_json::Value = serde_json::from_slice(&root[marker.len()..]).unwrap();
    let subject = format!("native-image-blob/1/{run}");
    let key = runtime.execution.root.pack(&(
        "evidence-chunk/1",
        subject.as_str(),
        blob,
        manifest["sha256"].as_str().unwrap(),
        0_i64,
    ));
    let trx = runtime.execution.transaction().await.unwrap();
    let value = trx.get(&key, false).await.unwrap().unwrap().to_vec();
    trx.clear(&key);
    trx.commit().await.unwrap();
    (key, value)
}
pub(crate) async fn restore_chunk(runtime: &RuntimeAuthority, removed: (Vec<u8>, Vec<u8>)) {
    let trx = runtime.execution.transaction().await.unwrap();
    assert!(trx.get(&removed.0, false).await.unwrap().is_none());
    trx.set(&removed.0, &removed.1);
    trx.commit().await.unwrap();
}

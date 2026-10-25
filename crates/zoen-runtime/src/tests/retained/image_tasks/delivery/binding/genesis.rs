//! A real bootstrap connection proves registration but cannot upload/publish.
use super::*;

pub(in crate::tests::retained::image_tasks::delivery) async fn run(
    f: &Fixture,
    relay: &relay::Relay,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let actor = actor(f);
    let deployment = f
        .world
        .runtime
        .configured_image_deployment(&actor.identity, actor.device.as_deref().unwrap())
        .await
        .unwrap();
    let (sql, fdb) = f
        .world
        .runtime
        .image_checkpoints(&deployment)
        .await
        .unwrap();
    assert_eq!(sql, fdb);
    assert_eq!((sql.generation, sql.sequence), (1, 0));
    let before = head(f).await;
    let scope = expected(f, 61, &sql);
    let mut transport = RelayOutputTransport::connect_bound(
        &format!("http://{}", relay.address),
        &relay.state.relay_name,
        actor.clone(),
        scope,
    )
    .await
    .unwrap();
    let bytes = vec![83; 57];
    let now = f.world.runtime.finance.reply_clock().await.unwrap();
    assert!(matches!(
        transport.upload(&hash(&bytes), bytes, now).await,
        Err(RelayOutputError::Blob(
            client_core::blob_transport::BlobUploadError::Refused(403)
        ))
    ));
    let envelope = Envelope::sealed(
        &actor,
        &f.space,
        &new_id("genesis-no-publish")?,
        now,
        Some(&before),
        protocol::Sealed::new(protocol::SealedKind::Application, 1, vec![1, 2, 3]),
    )?;
    let result = transport
        .publish_managed(&envelope, expected(f, 62, &sql))
        .await;
    assert!(
        matches!(result, Err(RelayOutputError::Refused)),
        "read-only bootstrap Publish must refuse before any effect: {result:?}"
    );
    assert_eq!(head(f).await, before);
    assert!(relay
        .state
        .blobs
        .list(None)
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .is_empty());
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_claims").await,
        0
    );
    println!("managed image bootstrap: actual SQL/FDB/FS genesis MAC binding accepted; real signed HTTP upload and WS Publish refused before effect PASS");
    Ok(())
}

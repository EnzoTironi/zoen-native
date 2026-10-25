//! Real relay HTTP/WS ACK loss, cold native recovery and original output dedupe.
mod binding;
mod proxy;
mod relay;
mod restore;
mod review;
use super::*;
use object_store::ObjectStoreExt;
pub(super) use review::run as review;

async fn head(f: &Fixture) -> Seen {
    // Independent observation through the actual relay log used by the router.
    // This small fixture must fit the bounded read rather than guess its head.
    let entries = f.log.read(&f.space, 1, 256).await.unwrap();
    assert!(entries.len() < 256);
    let observed = entries.last().unwrap();
    Seen {
        seq: observed.seq,
        hash: observed.hash.clone(),
    }
}

pub(super) async fn run(
    f: &mut Fixture,
    run: &str,
    output: &crate::ImageOutputInspection,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let relay = relay::Relay::start(f).await?;
    Box::pin(binding::run(f, run, &relay)).await?;
    let proxy = proxy::Proxy::start(relay.address).await;
    let target = relay.target(f, proxy.address);
    assert_eq!(
        f.world
            .runtime
            .deliver_image_task_output(run, target.clone())
            .await
            .unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    let (envelope, chunks) = proof::transport_original(&f.world.runtime, run).await;
    let checkpoint = proof::output_checkpoint(&f.world.runtime, run)
        .await
        .unwrap();
    crate::tests::managed_images::assert_publish_checkpoint(
        &f.world.runtime,
        f.managed_directory.as_ref().unwrap(),
        &envelope,
        &checkpoint,
    )
    .await;
    let pending = proof::uploads_pending(&f.world.runtime, run).await;
    assert_eq!(pending, chunks.len() as u64);
    assert!(pending > 0);

    // An operator route cannot substitute a different relay source or Challenge.
    let other_source = crate::ImageDeliveryTarget::configured(
        format!("http://{}", proxy.address),
        relay.state.relay_name.clone(),
        "wrong-source-cell".into(),
    )
    .unwrap();
    assert_eq!(
        proof::deliver(&f.world.runtime, run, &other_source)
            .await
            .unwrap_err(),
        RuntimeError::InvalidBinding
    );
    let other_relay = crate::ImageDeliveryTarget::configured(
        format!("http://{}", proxy.address),
        "wrong-relay".into(),
        f.world.relay_cell.clone().unwrap(),
    )
    .unwrap();
    assert_eq!(
        proof::deliver(&f.world.runtime, run, &other_relay)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(proxy.blob_dropped.load(Ordering::SeqCst), 0);

    // Catch-up must observe current permission before any new artifact effect.
    f.encrypted(EventBody::GrantRevoked {
        grant: "native-image-owner-act".into(),
    })
    .await?;
    assert_eq!(
        proof::deliver(&f.world.runtime, run, &target)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(proof::uploads_pending(&f.world.runtime, run).await, pending);
    assert_eq!(proxy.blob_dropped.load(Ordering::SeqCst), 0);
    f.encrypted(EventBody::GrantIssued {
        grant: Grant {
            id: "image-delivery-new-trust".into(),
            grantor: f.owner.id.clone(),
            grantee: Some(f.agent.id.clone()),
            scope: GrantScope::Space(f.space.clone()),
            capability: Capability::Trust(TrustLevel::Act),
            expires_at_ms: None,
        },
    })
    .await?;
    let before = head(f).await;

    // Genuine CREATED after authenticated signed PUT reaches durable object
    // storage; the proxy discards it without returning a replacement response.
    assert_eq!(
        proof::deliver(&f.world.runtime, run, &target)
            .await
            .unwrap_err(),
        RuntimeError::Unavailable
    );
    assert_eq!(proxy.blob_dropped.load(Ordering::SeqCst), 1);
    assert_eq!(proof::uploads_pending(&f.world.runtime, run).await, pending);
    assert_eq!(head(f).await, before);
    let first = &chunks[0];
    let key = object_store::path::Path::from(format!("blobs/{}/{}", &first.blob[..2], first.blob));
    let stored = relay
        .state
        .blobs
        .get(&key)
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(hash(&stored), first.blob);
    assert_eq!(stored.len() as u64, first.bytes + 40);

    output::cold(f).await;
    assert_eq!(
        proof::transport_original(&f.world.runtime, run).await.0,
        envelope
    );
    // The retry gets the real existing-object200, then publishes exactly the
    // retained sealed envelope. The proxy discards the actual Accepted frame.
    assert_eq!(
        proof::deliver(&f.world.runtime, run, &target)
            .await
            .unwrap_err(),
        RuntimeError::Unavailable
    );
    assert_eq!(proxy.publish_dropped.load(Ordering::SeqCst), 1);
    assert_eq!(proof::uploads_pending(&f.world.runtime, run).await, 0);
    let after = head(f).await;
    assert_eq!(after.seq, before.seq + 1);
    let real = f.log.read(&f.space, after.seq, 1).await.unwrap().remove(0);
    assert_eq!(real.env, envelope);
    assert_eq!(
        relay.state.metrics.events_sequenced.load(Ordering::SeqCst),
        1
    );

    output::cold(f).await;
    let accepted = proof::deliver(&f.world.runtime, run, &target)
        .await
        .unwrap();
    assert_eq!(
        accepted,
        crate::ImageDeliveryProgress::Accepted {
            item: output.item.clone(),
            accepted: Seen {
                seq: after.seq,
                hash: after.hash.clone()
            }
        }
    );
    assert_eq!(head(f).await, after);
    let generation = native::testing::generation(
        &f.world.runtime,
        &f.agent.id,
        f.world.agents[0].device.as_deref().unwrap(),
    )
    .await;
    output::cold(f).await;
    assert_eq!(
        proof::deliver(&f.world.runtime, run, &target)
            .await
            .unwrap(),
        accepted
    );
    assert_eq!(
        native::testing::generation(
            &f.world.runtime,
            &f.agent.id,
            f.world.agents[0].device.as_deref().unwrap()
        )
        .await,
        generation
    );
    assert_eq!(
        relay.state.metrics.events_duplicate.load(Ordering::SeqCst),
        0
    );

    // A completed historical receipt is inert even after directory revocation.
    f.head = Some(after.clone());
    super::super::output::revoke_device(f).await;
    assert_eq!(
        proof::deliver(&f.world.runtime, run, &target)
            .await
            .unwrap(),
        accepted
    );
    assert_eq!(head(f).await, after);
    assert_eq!(proxy.blob_dropped.load(Ordering::SeqCst), 1);
    assert_eq!(proxy.publish_dropped.load(Ordering::SeqCst), 1);
    relay.stop().await;
    println!("native image transport: real signed HTTP PUT, lost CREATED and lost real WS Accepted, cold exact original FileDoc/Publish, current revocation refusal and inert receipt replay PASS (production gate closed)");
    Ok(())
}

use super::*;
use object_store::throttle::{ThrottleConfig, ThrottledStore};
use relay_admission::managed_images::AnchorError;
use std::time::Duration;

#[tokio::test(start_paused = true)]
async fn cancelled_handler_retains_generation_until_real_object_store_effect_terminates(
) -> Result<(), Box<dyn std::error::Error>> {
    let (home, deployment, mut writer) = crate::managed_devices::tests::fixture()?;
    let registry =
        crate::managed_devices::ManagedDevices::open([(deployment.clone(), home.0.clone())])?;
    let configured = registry
        .get(&deployment.device)
        .ok_or("missing managed fixture")?;
    let before = writer.reader().checkpoint()?;
    let original = writer.prepare(
        &event_log::sha256_hex(b"original upload"),
        &before,
        &before,
        false,
    )?;
    writer.reconcile(&original.next, &original.next)?;
    let newer = writer.prepare(
        &event_log::sha256_hex(b"unrelated document"),
        &original.next,
        &original.next,
        false,
    )?;
    writer.reconcile(&newer.next, &newer.next)?;
    let checkpoint = newer.next;
    let guard = configured.upload_guard(&original.next)?;
    let store: Arc<dyn ObjectStore> = Arc::new(ThrottledStore::new(
        object_store::memory::InMemory::new(),
        ThrottleConfig {
            wait_put_per_call: Duration::from_secs(60),
            ..Default::default()
        },
    ));
    let key = ObjPath::from("actual-managed-object");
    let caller = tokio::spawn(put_effect(
        store.clone(),
        key.clone(),
        Bytes::from_static(b"actual bytes"),
        PutOptions {
            mode: PutMode::Create,
            ..Default::default()
        },
        Some(guard),
    ));
    // Virtual time permits both the handler and its owned storage task to start.
    tokio::time::sleep(Duration::from_millis(1)).await;
    caller.abort();
    assert!(caller.await.is_err());
    let effect = event_log::sha256_hex(b"retire after cancelled HTTP caller");
    assert!(matches!(
        writer.prepare(&effect, &checkpoint, &checkpoint, true),
        Err(AnchorError::Busy)
    ));
    assert!(matches!(
        store.get(&key).await,
        Err(object_store::Error::NotFound { .. })
    ));
    tokio::time::advance(Duration::from_secs(60)).await;
    tokio::time::sleep(Duration::from_millis(1)).await;
    assert_eq!(
        store.get(&key).await?.bytes().await?.as_ref(),
        b"actual bytes"
    );
    let retired = writer.prepare(&effect, &checkpoint, &checkpoint, true)?;
    assert_eq!(retired.next.generation, checkpoint.generation + 1);
    Ok(())
}

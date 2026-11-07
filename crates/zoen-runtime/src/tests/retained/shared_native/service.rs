//! Hot device enrollment must keep one ledger/anchor and require real route proof.
use super::*;
use crate::tests::{vault::Vault, World};
use crate::{ImageDeliveryTarget, ReplyHostConfig, ReplyRuntimeHost, RuntimeConfig};
use client_core::SecretVault;
use zoen_relay::managed_devices::{ManagedDevices, ManagedPrincipal, ManagedRuntime};

fn config(world: &World) -> RuntimeConfig {
    let mut value = crate::tests::config(&world.db_url, &world.config_namespace, &world.http.base);
    value.relay_cell = world.relay_cell.clone();
    value.price = world.runtime.price.clone();
    value.gateway.limits.max_response_bytes = world.response_limit;
    value
}
async fn reopen_service(f: &mut Fixture, vault: Arc<Vault>) -> TestResult {
    let opened = RuntimeAuthority::open_native_service(config(&f.world), vault, 1).await?;
    let old = Arc::get_mut(&mut f.world.runtime).ok_or("old runtime shared")?;
    old.reply_output = None;
    old.runtime_continuity = None;
    let runtime = opened
        .open_runtime_continuity(&f.shared_directory.as_ref().ok_or("anchor")?.0)
        .await?;
    f.world.runtime.finance.pool.close().await;
    f.world.runtime = Arc::new(runtime);
    Ok(())
}
fn host(
    runtime: Arc<RuntimeAuthority>,
    actor: &Author,
    target: ImageDeliveryTarget,
) -> Result<ReplyRuntimeHost, RuntimeError> {
    ReplyRuntimeHost::configured(
        runtime,
        ReplyHostConfig {
            agent: actor.identity.clone(),
            device: actor.device.clone().ok_or(RuntimeError::InvalidBinding)?,
            target,
        },
        gate::Gate::default(),
    )
}

pub(in crate::tests) async fn run() -> TestResult {
    let mut f = Fixture::with_shared_runtime().await?;
    f.grant("service-owner-grant").await?;
    f.message(ORIGINAL).await?;
    let run = f
        .sync(false)
        .await?
        .runs
        .into_iter()
        .next()
        .ok_or("original")?;
    let first = f.world.agents[0].clone();
    let second = f.world.agents[1].clone();
    let first_device = first.device.as_deref().ok_or("first device")?;
    let second_device = second.device.as_deref().ok_or("second device")?;
    let global = Vault::new()?;
    assert!(global.save(
        RuntimeAuthority::native_capsule_vault_key(&f.world.config_namespace, 1)?,
        vec![73; 32]
    ));
    let first_keys = Vault::new()?;
    let second_keys = Vault::new()?;
    let second_key = RuntimeAuthority::native_device_vault_key(
        &f.world.config_namespace,
        &second.identity,
        second_device,
    )?;
    assert!(first_keys.save(
        RuntimeAuthority::native_device_vault_key(
            &f.world.config_namespace,
            &first.identity,
            first_device
        )?,
        first.key.secret().to_vec()
    ));
    assert!(second_keys.save(second_key.clone(), second.key.secret().to_vec()));
    let before = f
        .world
        .runtime
        .inspect_native_device(&first.identity, first_device)
        .await?;
    reopen_service(&mut f, global.clone()).await?;
    assert!(f.world.runtime.run_model(&run.run).await.is_err());
    f.world
        .runtime
        .attach_native_device(first_keys.clone(), &first.identity, first_device)
        .await?;
    assert!(f.world.runtime.run_model(&run.run).await.is_err());
    assert_eq!(
        f.world
            .runtime
            .inspect_native_device(&first.identity, first_device)
            .await?,
        before
    );
    let managed = f
        .world
        .runtime
        .runtime_continuity
        .as_ref()
        .ok_or("continuity")?
        .lock()
        .await;
    f.log
        .configure_managed_devices(ManagedDevices::open_with_runtimes(
            [],
            [ManagedRuntime {
                deployment: managed.test_deployment(),
                anchor_directory: f.shared_directory.as_ref().ok_or("anchor")?.0.clone(),
                principals: vec![
                    ManagedPrincipal {
                        agent: first.identity.clone(),
                        device: first_device.into(),
                    },
                    ManagedPrincipal {
                        agent: second.identity.clone(),
                        device: second_device.into(),
                    },
                ],
            }],
        )?)?;
    drop(managed);
    let relay = crate::tests::retained::image_tasks::delivery::relay::Relay::start(&f).await?;
    let target = relay.target(&f, relay.address);
    let invalid_target = ImageDeliveryTarget::configured(
        format!("http://{}", relay.address),
        "wrong-relay".into(),
        target_source(&f)?,
    )?;
    let mut runtime = Arc::try_unwrap(f.world.runtime).map_err(|_| "bootstrap runtime shared")?;
    assert!(runtime
        .qualify_shared_reply_output_in_place(invalid_target)
        .await
        .is_err());
    assert!(runtime.reply_output.is_none());
    assert!(runtime.runtime_continuity.is_some());
    runtime
        .qualify_shared_reply_output_in_place(target.clone())
        .await?;
    assert!(runtime
        .qualify_shared_reply_output_in_place(target.clone())
        .await
        .is_err());
    f.world.runtime = Arc::new(runtime);
    let live_first = host(f.world.runtime.clone(), &first, target.clone())?;
    f.world
        .runtime
        .attach_native_device(second_keys.clone(), &second.identity, second_device)
        .await?;
    assert!(host(f.world.runtime.clone(), &second, target.clone()).is_err());
    assert!(f
        .world
        .runtime
        .production_output_scope()?
        .check_device(&second.identity, second_device)
        .is_err());
    f.world
        .runtime
        .initialize_native_device(&second.identity, second_device)
        .await?;
    let second_root = f
        .world
        .runtime
        .inspect_native_device(&second.identity, second_device)
        .await?;
    let before_qualification = checkpoint(&f).await?;
    let wrong = ImageDeliveryTarget::configured(
        format!("http://{}", relay.address),
        "wrong-relay".into(),
        target_source(&f)?,
    )?;
    assert!(f
        .world
        .runtime
        .qualify_shared_reply_device(wrong, &second.identity, second_device)
        .await
        .is_err());
    f.world
        .runtime
        .qualify_shared_reply_device(target.clone(), &second.identity, second_device)
        .await?;
    f.world
        .runtime
        .qualify_shared_reply_device(target.clone(), &second.identity, second_device)
        .await?;
    let live_second = host(f.world.runtime.clone(), &second, target.clone())?;
    assert!(live_second.pending_page(None).await?.runs.is_empty());
    assert!(Box::pin(live_second.advance(&run.run)).await.is_err());
    assert_eq!(checkpoint(&f).await?, before_qualification);
    assert_eq!(
        f.world
            .runtime
            .inspect_native_device(&first.identity, first_device)
            .await?,
        before
    );
    assert_eq!(
        f.world
            .runtime
            .inspect_native_device(&second.identity, second_device)
            .await?,
        second_root
    );
    assert!(second_keys.save(second_key, Signer::generate()?.secret().to_vec()));
    assert!(f
        .world
        .runtime
        .attach_native_device(second_keys, &second.identity, second_device)
        .await
        .is_err());
    let mut tx = f.world.runtime.finance.pool.begin().await?;
    zoen_relay::db::revoke_device(&mut tx, &second.identity, second_device).await?;
    tx.commit().await?;
    assert!(f
        .world
        .runtime
        .qualify_shared_reply_device(target, &second.identity, second_device)
        .await
        .is_err());
    drop(live_first);
    drop(live_second);
    reopen_service(&mut f, global).await?;
    f.world
        .runtime
        .attach_native_device(first_keys, &first.identity, first_device)
        .await?;
    assert_eq!(
        f.world
            .runtime
            .inspect_native_device(&first.identity, first_device)
            .await?,
        before
    );
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_attempts").await,
        0
    );
    relay.stop().await;
    f.finish().await;
    println!("shared native service: hot independent agents, actual relay qualification, retained originals, revoked sibling cold recovery and zero paid effects PASS");
    Ok(())
}
fn target_source(f: &Fixture) -> Result<String, RuntimeError> {
    f.world
        .relay_cell
        .clone()
        .ok_or(RuntimeError::InvalidBinding)
}

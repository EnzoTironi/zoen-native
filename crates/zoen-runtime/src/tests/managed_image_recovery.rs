//! Actual SQL/FDB recovery with an explicitly controlled financial adapter.
//! Native MLS + unpaid real HTTP remain covered by the separate retained journey.
use super::*;
use relay_admission::managed_images::{AnchorError, AnchorReader};

pub(super) async fn run() {
    let mut world = image_tasks::world().await;
    let actor = world.agents[0].clone();
    let device = actor.device.as_deref().unwrap();
    let sqlx::types::Json(agent): sqlx::types::Json<Identity> =
        sqlx::query_scalar("SELECT profile FROM identities WHERE id=$1")
            .bind(&actor.identity)
            .fetch_one(&world.runtime.finance.pool)
            .await
            .unwrap();
    let sqlx::types::Json(owner): sqlx::types::Json<Identity> =
        sqlx::query_scalar("SELECT profile FROM identities WHERE id=$1")
            .bind(&world.owner.identity)
            .fetch_one(&world.runtime.finance.pool)
            .await
            .unwrap();
    Arc::get_mut(&mut world.runtime).unwrap().native = Some(native::testing::custody(
        &world.config_namespace,
        agent,
        owner,
        actor.cert.clone().unwrap(),
        actor.key.secret(),
    ));
    let directory = managed_images::Directory::new();
    let runtime = match Arc::try_unwrap(world.runtime) {
        Ok(runtime) => runtime,
        Err(_) => panic!("fresh recovery fixture unexpectedly shared"),
    };
    world.runtime = Arc::new(
        runtime
            .initialize_managed_image_continuity(&directory.0, &actor.identity, device)
            .await
            .unwrap(),
    );
    let runtime = &world.runtime;
    let deployment = runtime
        .configured_image_deployment(&actor.identity, device)
        .await
        .unwrap();
    let reader = AnchorReader::open(&directory.0, deployment.clone()).unwrap();
    let before = reader.checkpoint().unwrap();
    let (step, task) = image_tasks::image_task(&world).await;
    let provider = image_tasks::Provider {
        task: task.clone(),
        result: image_tasks::result(&task, true),
        sends: Default::default(),
        close: None,
    };
    runtime.fault.store(2, Ordering::SeqCst); // actual FDB commit, before SQL finish
    assert_eq!(
        runtime.complete_image_task(&step, &task, &provider).await,
        Err(RuntimeError::Unavailable)
    );
    runtime.fault.store(0, Ordering::SeqCst);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
    let (sql, fdb) = runtime.image_checkpoints(&deployment).await.unwrap();
    assert_eq!(sql, before);
    assert_eq!(fdb.sequence, 1);
    assert_eq!(reader.checkpoint(), Err(AnchorError::Unreconciled));
    assert_eq!(world.count("SELECT count(*) FROM runtime_claims").await, 1);
    assert_eq!(
        world
            .count("SELECT count(*) FROM runtime_admission_witnesses")
            .await,
        0
    );

    // A changed reference cannot repair SQL, even if the current FDB checkpoint
    // itself still matches PREPARED.next. Preserve and restore the exact record.
    let key = runtime
        .execution
        .root
        .pack(&("managed-images/dispatch/1", fdb.sequence));
    let transaction = runtime.execution.transaction().await.unwrap();
    let original = transaction
        .get(&key, false)
        .await
        .unwrap()
        .unwrap()
        .to_vec();
    let mut changed: serde_json::Value = serde_json::from_slice(&original).unwrap();
    changed["reference"]["nonce"] = "f".repeat(64).into();
    transaction.set(&key, &serde_json::to_vec(&changed).unwrap());
    transaction.commit().await.unwrap();
    assert_eq!(
        runtime.check_image_checkpoint().await,
        Err(RuntimeError::InvalidBinding)
    );
    assert_eq!(
        runtime.image_checkpoints(&deployment).await.unwrap().0,
        before
    );
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
    let transaction = runtime.execution.transaction().await.unwrap();
    transaction.set(&key, &original);
    transaction.commit().await.unwrap();

    runtime.check_image_checkpoint().await.unwrap();
    assert_eq!(
        runtime.image_checkpoints(&deployment).await.unwrap(),
        (fdb.clone(), fdb.clone())
    );
    assert_eq!(reader.checkpoint().unwrap(), fdb);
    assert_eq!(
        world
            .count("SELECT count(*) FROM runtime_admission_witnesses")
            .await,
        0
    );
    assert_eq!(world.count("SELECT count(*) FROM runtime_claims").await, 1);
    assert_eq!(
        world.balance().await,
        PeriodBalance {
            held_units: 10,
            spent_units: 0
        }
    );
    assert_eq!(
        runtime
            .inspect_attempt(&task.binding.context.attempt_id)
            .await
            .unwrap(),
        Some(FinancialState::Claimed)
    );
    assert!(runtime
        .complete_image_task(&step, &task, &provider)
        .await
        .is_err());
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
    assert_eq!(
        world.balance().await,
        PeriodBalance {
            held_units: 10,
            spent_units: 0
        }
    );

    // Recovery unblocks a different original task; it does not resurrect the
    // consumed first claim or manufacture an absent bill/refund for that claim.
    let (next_step, next_task) = image_tasks::image_task(&world).await;
    let next_provider = image_tasks::Provider {
        task: next_task.clone(),
        result: image_tasks::result(&next_task, true),
        sends: Default::default(),
        close: None,
    };
    assert_eq!(
        runtime
            .complete_image_task(&next_step, &next_task, &next_provider)
            .await
            .unwrap(),
        FinancialState::Settled { units: 15 }
    );
    assert_eq!(next_provider.sends.load(Ordering::SeqCst), 1);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
    assert_eq!(world.count("SELECT count(*) FROM runtime_claims").await, 2);
    assert_eq!(
        world
            .count("SELECT count(*) FROM runtime_admission_witnesses")
            .await,
        1
    );
    assert_eq!(
        world.balance().await,
        PeriodBalance {
            held_units: 10,
            spent_units: 15
        }
    );
    assert_eq!(reader.checkpoint().unwrap().sequence, 2);
    assert_eq!(
        runtime.production_output_scope().err(),
        Some(RuntimeError::CoreAuthorityUnavailable)
    );
    world.finish().await;
    println!("managed image recovery: actual FDB admission/SQL-finish cut, altered reference refused, original claim retained without witness/Fresh/refund, second task progresses once PASS (controlled adapter; production gate closed)");
}

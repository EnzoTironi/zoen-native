//! Real SQL/FDB checkpoint assertions inside the original native Images journey.
//! Rewinding marker rows simulates the lineage of a matching old store snapshot;
//! this does not claim a full deployment backup/restore or production admission.
use super::*;
use relay_admission::managed_images::{AnchorReader, Checkpoint, Deployment};
use std::path::PathBuf;

pub(super) struct Directory(pub(super) PathBuf);
impl Directory {
    pub(super) fn new() -> Self {
        Self(std::env::temp_dir().join(types::new_id("managed-image-anchor")))
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) async fn replacement_anchor_refused(
    world: &World,
    actor: &Author,
    agent: &Identity,
    owner: &Identity,
) {
    let mut contender = world.reopen().await;
    Arc::get_mut(&mut contender).unwrap().native = Some(native::testing::custody(
        &world.config_namespace,
        agent.clone(),
        owner.clone(),
        actor.cert.clone().unwrap(),
        actor.key.secret(),
    ));
    let contender = match Arc::try_unwrap(contender) {
        Ok(contender) => contender,
        Err(_) => panic!("bootstrap contender unexpectedly shared"),
    };
    let replacement = Directory::new();
    let result = contender
        .initialize_managed_image_continuity(
            &replacement.0,
            &actor.identity,
            actor.device.as_deref().unwrap(),
        )
        .await;
    assert!(matches!(result, Err(RuntimeError::RestoreUnreconciled)));
    assert!(
        !replacement.0.exists(),
        "even genesis cannot be adopted into a second anchor directory"
    );
    println!("managed image genesis: existing actual SQL/FDB genesis refuses a second independent anchor before attempts or capsules PASS");
}

pub(super) async fn assert_dispatch_checkpoint(
    runtime: &RuntimeAuthority,
    directory: &Directory,
    agent: &str,
    device: &str,
) {
    let deployment = runtime
        .configured_image_deployment(agent, device)
        .await
        .unwrap();
    let reader = AnchorReader::open(&directory.0, deployment.clone()).unwrap();
    let committed = reader.checkpoint().unwrap();
    assert_eq!(committed.generation, 1);
    assert_eq!(
        committed.sequence, 2,
        "original review publication precedes original dispatch"
    );
    let observed = runtime.image_checkpoints(&deployment).await.unwrap();
    assert_eq!(observed, (committed.clone(), committed.clone()));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM runtime_admission_witnesses w JOIN runtime_claims c ON c.attempt=w.attempt AND c.nonce=w.nonce")
        .fetch_one(&runtime.finance.pool).await.unwrap();
    assert_eq!(
        rows, 1,
        "checkpoint shares the original financial admission witness"
    );
    let transaction = runtime.execution.transaction().await.unwrap();
    for sequence in 1..=committed.sequence {
        let bytes = transaction
            .get(
                &runtime
                    .execution
                    .root
                    .pack(&("managed-images/effect/1", sequence)),
                false,
            )
            .await
            .unwrap()
            .unwrap();
        let prepared: relay_admission::managed_images::Prepared =
            serde_json::from_slice(&bytes).unwrap();
        assert_eq!(prepared.next.sequence, sequence);
        if sequence == committed.sequence {
            assert_eq!(prepared.next, committed);
        }
    }
    runtime.check_image_checkpoint().await.unwrap();
    println!("managed image dispatch: actual SQL witness/FDB original admission and independent durable effect checkpoint agree; no extra claim PASS");
}

pub(super) async fn matching_checkpoint_rollback(
    runtime: &RuntimeAuthority,
    directory: &Directory,
    agent: &str,
    device: &str,
) {
    let deployment = runtime
        .configured_image_deployment(agent, device)
        .await
        .unwrap();
    let reader = AnchorReader::open(&directory.0, deployment.clone()).unwrap();
    let original = reader.checkpoint().unwrap();
    // Original genesis derives from the exact actual deployment, not a fixture
    // namespace flag. Both real stores now present the same older checkpoint.
    let before = Checkpoint {
        generation: 1,
        sequence: 0,
        digest: hash(&serde_json::to_vec(&("managed-images/genesis/1", &deployment)).unwrap()),
    };
    replace(runtime, &deployment, &before).await;
    assert_eq!(
        runtime.image_checkpoints(&deployment).await.unwrap(),
        (before.clone(), before)
    );
    assert_eq!(
        runtime.check_image_checkpoint().await,
        Err(RuntimeError::RestoreUnreconciled)
    );
    assert_eq!(
        reader.checkpoint().unwrap(),
        original,
        "store restore cannot rewrite external history"
    );
    // Restore the exact original marker observations to continue this same real
    // unpaid native journey. No new attempt, source, provider or anchor is made.
    replace(runtime, &deployment, &original).await;
    runtime.check_image_checkpoint().await.unwrap();
    println!("managed image restore detector: matching old actual SQL+FDB checkpoint rows rejected by independent anchor; original pair restored, no permission minted PASS (not full deployment backup/restore)");
}

async fn replace(runtime: &RuntimeAuthority, deployment: &Deployment, checkpoint: &Checkpoint) {
    sqlx::query("UPDATE runtime_image_checkpoint SET generation=$1,sequence=$2,digest=$3 WHERE namespace=$4")
        .bind(i64::try_from(checkpoint.generation).unwrap()).bind(i64::try_from(checkpoint.sequence).unwrap())
        .bind(&checkpoint.digest).bind(&deployment.namespace).execute(&runtime.finance.pool).await.unwrap();
    let transaction = runtime.execution.transaction().await.unwrap();
    transaction.set(
        &runtime
            .execution
            .root
            .pack(&("managed-images/checkpoint/1",)),
        &serde_json::to_vec(checkpoint).unwrap(),
    );
    transaction.commit().await.unwrap();
}

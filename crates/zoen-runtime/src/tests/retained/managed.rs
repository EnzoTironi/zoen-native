//! Normal managed constructor and MLS ingress, with real SQL/FDB/OpenMLS.
//! The fresh vault and registered actors are fixtures, not deployed enrollment.
use super::super::vault::Vault;
use super::*;
use client_core::SecretVault;

async fn open(world: &World, vault: Arc<Vault>) -> RuntimeAuthority {
    let actor = &world.agents[0];
    let mut config = config(&world.db_url, &world.config_namespace, &world.http.base);
    config.relay_cell = world.relay_cell.clone();
    let runtime = RuntimeAuthority::open_with_native_device(
        config,
        vault,
        &actor.identity,
        actor.device.as_deref().unwrap(),
        1,
    )
    .await
    .unwrap();
    assert_eq!(runtime.continuity(), Err(RuntimeError::RestoreUnreconciled));
    runtime
}
async fn reopen(world: &mut World, vault: Arc<Vault>) {
    let runtime = open(world, vault).await;
    world.runtime.finance.pool.close().await;
    world.runtime = Arc::new(runtime);
}

async fn fixture() -> (Fixture, Arc<Vault>) {
    let mut world = World::with_relay().await;
    let actor = &world.agents[0];
    let vault = Vault::new();
    assert!(vault.save(
        RuntimeAuthority::native_capsule_vault_key(&world.config_namespace, 1).unwrap(),
        vec![63; 32]
    ));
    assert!(vault.save(
        RuntimeAuthority::native_device_vault_key(
            &world.config_namespace,
            &actor.identity,
            actor.device.as_deref().unwrap()
        )
        .unwrap(),
        actor.key.secret().to_vec()
    ));
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
    let second = actor.clone();
    reopen(&mut world, vault.clone()).await;
    let log = FdbLog::open(
        std::env::var("FDB_CLUSTER_FILE").ok().as_deref(),
        world.relay_cell.as_deref().unwrap(),
    )
    .unwrap();
    let mut store = Store::open(":memory:").unwrap();
    mls::migrate(store.conn_mut()).unwrap();
    (
        Fixture {
            world,
            log: Arc::new(log),
            store,
            second,
            agent,
            owner,
            space: new_id("managed-ingress"),
            head: None,
            managed_directory: None,
        },
        vault,
    )
}

async fn publication(f: &mut Fixture, vault: Arc<Vault>) -> Vec<u8> {
    let actor = f.world.agents[0].clone();
    let agent = &actor.identity;
    let device = actor.device.as_deref().unwrap();
    f.world
        .runtime
        .initialize_native_device(agent, device)
        .await
        .unwrap();
    f.world.runtime.fault.store(65, Ordering::SeqCst);
    assert_eq!(
        f.world
            .runtime
            .prepare_native_key_packages(agent, device)
            .await
            .unwrap_err(),
        RuntimeError::Unavailable
    );
    assert_eq!(
        f.world
            .runtime
            .inspect_native_device(agent, device)
            .await
            .unwrap()
            .generation,
        1
    );
    assert_eq!(f.world.count("SELECT count(*) FROM key_packages").await, 0);
    f.world.runtime.fault.store(0, Ordering::SeqCst);
    let contender = open(&f.world, vault.clone()).await;
    let (one, two) = tokio::join!(
        f.world.runtime.prepare_native_key_packages(agent, device),
        contender.prepare_native_key_packages(agent, device)
    );
    let prepared = match (one, two) {
        (Ok(one), Ok(two)) => {
            assert_eq!(one, two);
            one
        }
        (Ok(one), Err(error)) | (Err(error), Ok(one)) => {
            assert!(matches!(
                error,
                RuntimeError::Denied | RuntimeError::Unavailable
            ));
            one
        }
        (Err(one), Err(two)) => panic!("neither package worker succeeded: {one:?}, {two:?}"),
    };
    contender.finance.pool.close().await;
    drop(contender);
    assert_eq!(prepared.native.generation, 2);
    assert_eq!(prepared.packages, 32);
    assert!(!prepared.published);
    assert_eq!(f.world.count("SELECT count(*) FROM key_packages").await, 0);
    reopen(&mut f.world, vault.clone()).await;
    assert_eq!(
        f.world
            .runtime
            .prepare_native_key_packages(agent, device)
            .await
            .unwrap(),
        prepared
    );
    // Actual successful SQL publication whose acknowledgement is discarded.
    // On retry, the real relay publication receipts prevent replenishing a
    // package that another caller has already destructively claimed.
    f.world.runtime.fault.store(64, Ordering::SeqCst);
    assert_eq!(
        f.world
            .runtime
            .publish_native_key_packages(agent, device)
            .await
            .unwrap_err(),
        RuntimeError::Unavailable
    );
    assert_eq!(f.world.count("SELECT count(*) FROM key_packages").await, 32);
    assert_eq!(
        f.world
            .count("SELECT count(*) FROM key_package_publications")
            .await,
        32
    );
    let operation = types::new_ulid(
        zoen_relay::key_package_claims::clock_ms(&f.world.runtime.finance.pool)
            .await
            .unwrap(),
    );
    let mut claimed = zoen_relay::key_package_claims::claim(
        &f.world.runtime.finance.pool,
        &f.owner.id,
        f.world.owner.device.as_deref().unwrap(),
        &operation,
        std::slice::from_ref(agent),
    )
    .await
    .unwrap();
    assert_eq!(claimed.len(), 1);
    let package = claimed.pop().unwrap();
    assert_eq!(&package.identity, agent);
    assert_eq!(package.device, device);
    let retried_claim = zoen_relay::key_package_claims::claim(
        &f.world.runtime.finance.pool,
        &f.owner.id,
        f.world.owner.device.as_deref().unwrap(),
        &operation,
        std::slice::from_ref(agent),
    )
    .await
    .unwrap();
    assert_eq!(retried_claim.len(), 1);
    assert_eq!(retried_claim[0].data, package.data);

    assert_eq!(f.world.count("SELECT count(*) FROM key_packages").await, 31);
    reopen(&mut f.world, vault.clone()).await;
    let published = f
        .world
        .runtime
        .publish_native_key_packages(agent, device)
        .await
        .unwrap();
    assert!(published.published);
    assert_eq!(published.native.generation, 3);
    assert_eq!(f.world.count("SELECT count(*) FROM key_packages").await, 31);
    let reinserted: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM key_packages WHERE data=$1)")
            .bind(&package.data)
            .fetch_one(&f.world.runtime.finance.pool)
            .await
            .unwrap();
    assert!(!reinserted);
    assert_eq!(
        f.world
            .runtime
            .publish_native_key_packages(agent, device)
            .await
            .unwrap(),
        published
    );
    assert_eq!(
        f.world
            .runtime
            .prepare_native_key_packages(agent, device)
            .await
            .unwrap(),
        published
    );

    package.data
}

async fn join_and_recover(f: &mut Fixture, vault: Arc<Vault>, package: Vec<u8>) {
    let actor = f.world.agents[0].clone();
    let agent = &actor.identity;
    let device = actor.device.as_deref().unwrap();
    f.clear(EventBody::SpaceCreated {
        title: "Managed MLS ingress".into(),
        kind: SpaceKind::Direct,
        privacy: Privacy::EndToEnd,
    })
    .await;
    f.clear(EventBody::MemberAdded {
        identity: agent.clone(),
        role: Role::Member,
    })
    .await;
    let owner = f.device();
    owner.create_group(&f.space).unwrap();
    let commit = owner
        .commit(&f.space, &[package], &std::collections::BTreeSet::new())
        .unwrap();
    let roster = std::collections::BTreeSet::from([f.owner.id.clone(), f.agent.id.clone()]);
    assert_eq!(
        owner
            .open(&f.space, &commit.commit, &roster, owner.leaf())
            .unwrap(),
        Opened::Commit { epoch: 1 }
    );
    drop(owner);
    f.sealed(SealedKind::Commit, commit.commit).await;
    f.sealed(SealedKind::Welcome, commit.welcome.unwrap()).await;
    let joined = f.sync(false).await.unwrap();
    assert!(joined.caught_up);
    assert!(joined.runs.is_empty());
    f.grant("managed-initial-trust").await;
    let trigger = f.message("Private managed ingress request").await;
    let discovered = f.sync(false).await.unwrap();
    assert!(discovered.caught_up);
    assert_eq!(discovered.runs.len(), 1);
    let run = discovered.runs[0].run.clone();
    assert!(!trigger.is_empty());
    assert_eq!(
        f.world.runtime.run_model(&run).await.unwrap_err(),
        RuntimeError::RestoreUnreconciled
    );
    assert_eq!(
        f.world
            .runtime
            .prepare_reply_output(&run)
            .await
            .unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    reopen(&mut f.world, vault.clone()).await;
    let cold = f.sync(false).await.unwrap();
    assert!(cold.caught_up && cold.runs.is_empty());
    assert_eq!(
        f.world.runtime.pending_reply_runs(16).await.unwrap().len(),
        1
    );
    assert_eq!(
        f.world
            .runtime
            .publish_native_key_packages(agent, device)
            .await
            .unwrap()
            .native,
        cold.native
    );
    assert_eq!(f.world.count("SELECT count(*) FROM key_packages").await, 31);
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_attempts").await,
        0
    );
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
}

async fn revoke(f: &Fixture) {
    let actor = &f.world.agents[0];
    let agent = &actor.identity;
    let device = actor.device.as_deref().unwrap();
    let other = &f.world.agents[1];
    assert_eq!(
        f.world
            .runtime
            .publish_native_key_packages(&other.identity, other.device.as_deref().unwrap())
            .await
            .unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    let mut tx = f.world.runtime.finance.pool.begin().await.unwrap();
    assert!(zoen_relay::db::revoke_device(&mut tx, agent, device)
        .await
        .unwrap());
    tx.commit().await.unwrap();
    assert_eq!(
        f.world
            .runtime
            .prepare_native_key_packages(agent, device)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(
        f.world
            .runtime
            .publish_native_key_packages(agent, device)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(f.world.count("SELECT count(*) FROM key_packages").await, 0);
}

pub(super) async fn run() {
    let (mut f, vault) = Box::pin(fixture()).await;
    println!("managed MLS ingress: fresh managed fixture opened");
    let package = Box::pin(publication(&mut f, vault.clone())).await;
    println!("managed MLS ingress: retained publication and recoverable claim verified");
    Box::pin(join_and_recover(&mut f, vault.clone(), package)).await;
    println!("managed MLS ingress: actual join and cold discovery verified");
    Box::pin(revoke(&f)).await;
    f.world.finish().await;
    drop(vault);
    println!("managed MLS ingress: public custody, retained initial packages, real SQL ACK loss/claim/replay, actual Welcome/Commit/Application through RuntimeSource, cold discovery, revocation PASS; continuity/model/output remain closed");
}

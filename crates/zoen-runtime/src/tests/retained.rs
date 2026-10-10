//! Actual certified directory, OpenMLS leaves and relay-owned FDB log. Fixture
//! custody is explicit; this does not provision a production vault or WS host.
mod admission;
use super::*;
use roda_log::content::{InnerEvent, Sealed, SealedKind};
use roda_mls::{Device, Opened, SUITE_ID};
use roda_proto::{Envelope, Sequenced};
use roda_store::Store;
use roda_types::*;
use zoen_relay::log::{fdb::FdbLog, LogStore, Sequencing};

const ORIGINAL: &str = "PRIVATE-RETAINED-NATIVE-ORIGINAL-7c0";
const LATER: &str = "PRIVATE-RETAINED-NATIVE-LATER-89a";

struct Fixture {
    world: World,
    log: FdbLog,
    store: Store,
    second: Author,
    agent: Identity,
    owner: Identity,
    space: String,
    head: Option<Seen>,
}

impl Fixture {
    async fn new() -> Self {
        let mut world = World::with_relay().await;
        let actor = world.agents[0].clone();
        let device = actor.device.clone().unwrap();
        let second = Author::device(&world.agent_roots[0], Signer::generate());
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
        let mut tx = zoen_relay::db::authorize_device(
            &world.runtime.finance.pool,
            &actor.identity,
            &device,
            true,
        )
        .await
        .unwrap()
        .unwrap();
        zoen_relay::db::enroll_device(
            &mut tx,
            &agent.id,
            second.device.as_deref().unwrap(),
            second.cert.as_deref().unwrap(),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let mut custody = native::testing::custody(
            &world.config_namespace,
            agent.clone(),
            owner.clone(),
            actor.cert.clone().unwrap(),
            actor.key.secret(),
        );
        native::testing::add_device(
            &mut custody,
            agent.clone(),
            owner.clone(),
            second.cert.clone().unwrap(),
            second.key.secret(),
        );
        Arc::get_mut(&mut world.runtime).unwrap().native = Some(custody);
        let first_package =
            native::testing::provision_packages(&world.runtime, &actor.identity, &device)
                .await
                .remove(0);
        let second_package = native::testing::provision_packages(
            &world.runtime,
            &second.identity,
            second.device.as_deref().unwrap(),
        )
        .await
        .remove(0);
        let log = FdbLog::open(
            std::env::var("FDB_CLUSTER_FILE").ok().as_deref(),
            world.relay_cell.as_deref().unwrap(),
        )
        .unwrap();
        let mut store = Store::open(":memory:").unwrap();
        roda_mls::migrate(store.conn_mut()).unwrap();
        let mut fixture = Self {
            world,
            log,
            store,
            second,
            agent,
            owner,
            space: new_id("retained-direct"),
            head: None,
        };
        fixture
            .clear(EventBody::SpaceCreated {
                title: "Retained owner reply".into(),
                kind: SpaceKind::Direct,
                privacy: Privacy::EndToEnd,
            })
            .await;
        fixture
            .clear(EventBody::MemberAdded {
                identity: fixture.agent.id.clone(),
                role: Role::Member,
            })
            .await;
        let owner = fixture.device();
        owner.create_group(&fixture.space).unwrap();
        let commit = owner
            .commit(
                &fixture.space,
                &[first_package, second_package],
                &std::collections::BTreeSet::new(),
            )
            .unwrap();
        let roster =
            std::collections::BTreeSet::from([fixture.owner.id.clone(), fixture.agent.id.clone()]);
        assert_eq!(
            owner
                .open(&fixture.space, &commit.commit, &roster, owner.leaf())
                .unwrap(),
            Opened::Commit { epoch: 1 }
        );
        drop(owner);
        fixture.sealed(SealedKind::Commit, commit.commit).await;
        fixture
            .sealed(SealedKind::Welcome, commit.welcome.unwrap())
            .await;
        fixture
    }

    fn device(&self) -> Device<'_> {
        Device::new(
            self.store.conn(),
            [79; 32],
            &self.world.owner.identity,
            self.world.owner.key.secret(),
            self.world.owner.cert.as_deref().unwrap(),
        )
        .unwrap()
    }

    async fn append(&mut self, envelope: Envelope) -> Sequenced {
        let entry = match self.log.append(&envelope, true).await.unwrap() {
            Sequencing::New { ev, .. } => ev,
            Sequencing::Duplicate { .. } => panic!("fixture uses fresh actual relay client IDs"),
        };
        self.head = Some(Seen {
            seq: entry.seq,
            hash: entry.hash.clone(),
        });
        entry
    }

    async fn clear(&mut self, body: EventBody) {
        let now = self.world.runtime.finance.reply_clock().await.unwrap();
        let event = self.world.owner.sign_event(
            &self.space,
            &new_id("clear"),
            now,
            self.head.clone(),
            body,
        );
        self.append(Envelope::plain(&event)).await;
    }

    async fn sealed(&mut self, kind: SealedKind, data: Vec<u8>) {
        let now = self.world.runtime.finance.reply_clock().await.unwrap();
        let envelope = Envelope::sealed(
            &self.world.owner,
            &self.space,
            &new_id("handshake"),
            now,
            self.head.as_ref(),
            Sealed::new(kind, SUITE_ID, data),
        );
        self.append(envelope).await;
    }

    async fn encrypted(&mut self, body: EventBody) -> String {
        let now = self.world.runtime.finance.reply_clock().await.unwrap();
        let client = new_id("inner");
        let event = self
            .world
            .owner
            .sign_event(&self.space, &client, now, self.head.clone(), body);
        let bytes = self
            .device()
            .seal(
                &self.space,
                &InnerEvent {
                    content: event.content,
                    sig: event.sig,
                }
                .encode(),
            )
            .unwrap();
        let envelope = Envelope::sealed(
            &self.world.owner,
            &self.space,
            &client,
            now,
            self.head.as_ref(),
            Sealed::new(SealedKind::Application, SUITE_ID, bytes),
        );
        self.append(envelope).await.hash
    }

    async fn grant(&mut self, grant: &str) {
        self.grant_until(grant, None).await;
    }

    async fn grant_until(&mut self, grant: &str, expires_at_ms: Option<i64>) {
        self.encrypted(EventBody::GrantIssued {
            grant: Grant {
                id: grant.into(),
                grantor: self.owner.id.clone(),
                grantee: Some(self.agent.id.clone()),
                scope: GrantScope::Space(self.space.clone()),
                capability: Capability::Trust(TrustLevel::Listen),
                expires_at_ms,
            },
        })
        .await;
    }

    async fn message(&mut self, text: &str) -> String {
        self.encrypted(EventBody::MessagePosted {
            message: new_id("message"),
            text: text.into(),
            attaches: None,
            reply: None,
        })
        .await
    }

    async fn sync(&self, second: bool) -> Result<ReplySync, RuntimeError> {
        let actor = if second {
            &self.second
        } else {
            &self.world.agents[0]
        };
        self.world
            .runtime
            .sync_reply_runs(
                &actor.identity,
                actor.device.as_deref().unwrap(),
                &self.space,
            )
            .await
    }

    fn restore_custody(&self) -> native::NativeCustody {
        let actor = &self.world.agents[0];
        let mut custody = native::testing::custody(
            &self.world.config_namespace,
            self.agent.clone(),
            self.owner.clone(),
            actor.cert.clone().unwrap(),
            actor.key.secret(),
        );
        native::testing::add_device(
            &mut custody,
            self.agent.clone(),
            self.owner.clone(),
            self.second.cert.clone().unwrap(),
            self.second.key.secret(),
        );
        custody
    }

    async fn finish(self) {
        self.log.drop_cell().await.unwrap();
        self.world.finish().await;
    }
}

pub(super) async fn run() {
    let mut fixture = Fixture::new().await;
    fixture.grant("retained-owner-trust").await;
    let trigger = fixture.message(ORIGINAL).await;
    let actor = fixture.world.agents[0].clone();
    let device = actor.device.clone().unwrap();
    for (cut, expected) in [
        (18, RuntimeError::InvalidBinding),
        (19, RuntimeError::InvalidBinding),
        (16, RuntimeError::Unavailable),
    ] {
        fixture.world.runtime.fault.store(cut, Ordering::SeqCst);
        assert_eq!(fixture.sync(false).await.unwrap_err(), expected);
        assert_eq!(
            native::testing::generation(&fixture.world.runtime, &actor.identity, &device).await,
            1
        );
        assert!(fixture
            .world
            .runtime
            .pending_reply_runs(64)
            .await
            .unwrap()
            .is_empty());
        native::testing::expire_abandoned_stage(&fixture.world.runtime, &actor.identity, &device)
            .await;
        println!("retained reply cut {cut}: root unchanged, no run or wake PASS");
    }
    fixture.world.runtime.fault.store(20, Ordering::SeqCst);
    let runtime = fixture.world.runtime.clone();
    let (agent, target_device, space) = (
        actor.identity.clone(),
        device.clone(),
        fixture.space.clone(),
    );
    let paused = tokio::spawn(async move {
        runtime
            .sync_reply_runs(&agent, &target_device, &space)
            .await
    });
    tokio::time::timeout(
        Duration::from_secs(3),
        fixture.world.runtime.native_cut_entered.notified(),
    )
    .await
    .unwrap();
    fixture
        .clear(EventBody::DeviceJoining {
            device: fixture.world.owner.device.clone().unwrap(),
        })
        .await;
    fixture.world.runtime.native_cut_resume.notify_one();
    assert_eq!(paused.await.unwrap().unwrap_err(), RuntimeError::Denied);
    assert_eq!(
        native::testing::generation(&fixture.world.runtime, &actor.identity, &device).await,
        1
    );
    assert!(fixture
        .world
        .runtime
        .pending_reply_runs(64)
        .await
        .unwrap()
        .is_empty());
    native::testing::expire_abandoned_stage(&fixture.world.runtime, &actor.identity, &device).await;
    println!("retained reply source cut: real relay advanced after SQL admission, no activation/run/wake PASS");
    fixture.world.runtime.fault.store(17, Ordering::SeqCst);
    assert_eq!(
        fixture.sync(false).await.unwrap_err(),
        RuntimeError::Unavailable
    );
    fixture.world.runtime.fault.store(0, Ordering::SeqCst);
    assert_eq!(
        native::testing::generation(&fixture.world.runtime, &actor.identity, &device).await,
        2
    );
    let pending = fixture.world.runtime.pending_reply_runs(64).await.unwrap();
    assert_eq!(pending.len(), 1);
    let original = pending[0].clone();
    let replay = fixture.sync(false).await.unwrap();
    assert!(replay.caught_up && replay.discovery_complete && replay.runs.is_empty());
    assert_eq!(
        replay.native.generation, 2,
        "no-op discovery does not consume retention generations"
    );
    assert_eq!(
        fixture.world.runtime.pending_reply_runs(64).await.unwrap(),
        pending
    );
    println!("retained reply journey: known commit ACK suppressed, one immutable run and durable wake PASS");

    let second = fixture.sync(true).await.unwrap();
    assert_eq!(
        second.runs, pending,
        "another certified device finds the original stable run"
    );
    assert_eq!(
        fixture.world.runtime.pending_reply_runs(64).await.unwrap(),
        pending
    );
    assert_eq!(
        fixture
            .world
            .runtime
            .inspect_reply_run(&original.run)
            .await
            .unwrap(),
        original
    );
    let (record_before, sealed_before) =
        native::testing::reply_original(&fixture.world.runtime, &original.run).await;
    assert_eq!(record_before, format!("{ORIGINAL}\n"));
    println!("retained reply journey: two actual joined devices deduplicate the same source trigger PASS");

    let mut reopened = fixture.world.reopen().await;
    assert_eq!(
        reopened.inspect_reply_run(&original.run).await.unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    assert_eq!(reopened.pending_reply_runs(64).await.unwrap(), pending);
    Arc::get_mut(&mut reopened).unwrap().native = Some(fixture.restore_custody());
    assert_eq!(
        reopened.inspect_reply_run(&original.run).await.unwrap(),
        original
    );
    fixture.world.runtime.finance.pool.close().await;
    fixture.world.runtime = reopened;
    for source in [None, Some("replacement-relay-cell".into())] {
        let mut changed = config(
            &fixture.world.db_url,
            &fixture.world.config_namespace,
            &fixture.world.http.base,
        );
        changed.relay_cell = source;
        assert!(matches!(
            RuntimeAuthority::open(changed).await,
            Err(RuntimeError::DeploymentMismatch)
        ));
    }
    let source_root = foundationdb::tuple::Subspace::all()
        .subspace(&("zoen", fixture.world.relay_cell.as_deref().unwrap()));
    let entry_key = source_root.pack(&(
        "s",
        fixture.space.as_str(),
        "log",
        fixture.head.as_ref().unwrap().seq as i64,
    ));
    let trx = fixture.world.runtime.execution.transaction().await.unwrap();
    let retained_wire = trx.get(&entry_key, false).await.unwrap().unwrap().to_vec();
    trx.clear(&entry_key);
    trx.commit().await.unwrap();
    assert_eq!(fixture.sync(false).await.unwrap_err(), RuntimeError::Denied);
    assert_eq!(
        native::testing::generation(&fixture.world.runtime, &actor.identity, &device).await,
        2
    );
    let trx = fixture.world.runtime.execution.transaction().await.unwrap();
    trx.set(&entry_key, &retained_wire);
    trx.commit().await.unwrap();
    println!("retained reply source cut: changed cell and missing real source entry refuse progress PASS");
    let later_trigger = fixture.message(LATER).await;
    assert_ne!(later_trigger, trigger);
    assert_eq!(
        fixture
            .world
            .runtime
            .inspect_reply_run(&original.run)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    let caught_up = fixture.sync(false).await.unwrap();
    assert!(caught_up.caught_up && caught_up.discovery_complete);
    assert_eq!(caught_up.runs.len(), 1);
    assert_ne!(caught_up.runs[0], original);
    assert_eq!(
        fixture
            .world
            .runtime
            .inspect_reply_run(&original.run)
            .await
            .unwrap(),
        original
    );
    let (text_after, sealed_after) =
        native::testing::reply_original(&fixture.world.runtime, &original.run).await;
    assert_eq!(text_after, record_before);
    assert_eq!(
        sealed_after, sealed_before,
        "cold restore and later heads do not rewrite original request or binding"
    );
    native::testing::reply_fences(&fixture.world.runtime, &original.run).await;
    println!("retained reply journey: cold reconstruction, actual source currentness and original input survive later heads PASS");

    fixture
        .encrypted(EventBody::GrantRevoked {
            grant: "retained-owner-trust".into(),
        })
        .await;
    assert_eq!(
        fixture
            .world
            .runtime
            .inspect_reply_run(&original.run)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert!(fixture.sync(false).await.unwrap().runs.is_empty());
    assert_eq!(
        fixture
            .world
            .runtime
            .inspect_reply_run(&original.run)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    fixture.grant("new-owner-trust").await;
    fixture
        .message("A genuinely new request after regrant")
        .await;
    assert_eq!(fixture.sync(false).await.unwrap().runs.len(), 1);
    assert_eq!(
        fixture
            .world
            .runtime
            .inspect_reply_run(&original.run)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(
        fixture
            .world
            .runtime
            .run_model(&original.run)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_attempts")
            .await,
        0
    );
    assert_eq!(fixture.world.http.sends.load(Ordering::SeqCst), 0);
    println!("retained reply journey: native revocation and regrant do not renew old intent; no financial hold or model send PASS");
    fixture.finish().await;

    let mut partial = Fixture::new().await;
    partial.grant("bounded-owner-trust").await;
    partial.message("Request inside a bounded catch-up").await;
    for _ in 0..70 {
        partial
            .clear(EventBody::DeviceJoining {
                device: partial.world.owner.device.clone().unwrap(),
            })
            .await;
    }
    let first = partial.sync(false).await.unwrap();
    assert!(!first.caught_up && first.runs.is_empty() && !first.discovery_complete);
    assert_eq!(first.scanned, 0);
    assert!(partial
        .world
        .runtime
        .pending_reply_runs(64)
        .await
        .unwrap()
        .is_empty());
    let second = partial.sync(false).await.unwrap();
    assert!(second.caught_up && !second.discovery_complete);
    assert_eq!(second.scanned, 64);
    assert_eq!(second.runs.len(), 1);
    let third = partial.sync(false).await.unwrap();
    assert!(third.caught_up && third.discovery_complete && third.runs.is_empty());
    assert_eq!(
        partial.world.runtime.pending_reply_runs(64).await.unwrap(),
        second.runs
    );
    assert_eq!(
        partial
            .world
            .count("SELECT count(*) FROM runtime_attempts")
            .await,
        0
    );
    assert_eq!(partial.world.http.sends.load(Ordering::SeqCst), 0);
    println!("retained reply journey: bounded partial source creates no run, authenticated discovery resumes once PASS");
    partial.finish().await;
    Box::pin(admission::run()).await;
}

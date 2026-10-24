//! One actual retained native workspace shared by authority and delivery journeys.
use super::*;

pub(super) struct Fixture {
    pub(super) world: World,
    pub(super) log: Arc<FdbLog>,
    pub(super) store: Store,
    pub(super) second: Author,
    pub(super) agent: Identity,
    pub(super) owner: Identity,
    pub(super) space: String,
    pub(super) head: Option<Seen>,
    pub(super) managed_directory: Option<crate::tests::managed_images::Directory>,
}

impl Fixture {
    pub(super) async fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        Self::build(false, false).await
    }

    pub(super) async fn with_output() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        Self::build(true, false).await
    }

    pub(super) async fn with_managed_output(
    ) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        Self::build(true, true).await
    }

    async fn build(
        output: bool,
        managed: bool,
    ) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let mut world = World::with_relay().await?;
        if output {
            let capability = native::testing::output::provision(&world.runtime).await;
            Arc::get_mut(&mut world.runtime).unwrap().output_fixture = Some(capability);
        }
        let actor = world.agents[0].clone();
        let device = actor.device.clone().unwrap();
        let second = Author::device(&world.agent_roots[0], Signer::generate()?);
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
        let managed_directory = managed
            .then(crate::tests::managed_images::Directory::new)
            .transpose()?;
        if let Some(directory) = &managed_directory {
            let runtime = match Arc::try_unwrap(world.runtime) {
                Ok(runtime) => runtime,
                Err(_) => panic!("fresh fixture runtime unexpectedly shared"),
            };
            world.runtime = Arc::new(
                runtime
                    .initialize_managed_image_continuity(&directory.0, &actor.identity, &device)
                    .await
                    .unwrap(),
            );
            assert_eq!(
                world.runtime.production_output_scope().err(),
                Some(RuntimeError::CoreAuthorityUnavailable)
            );
            crate::tests::managed_images::replacement_anchor_refused(
                &world, &actor, &agent, &owner,
            )
            .await?;
        }
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
        let log = Arc::new(
            FdbLog::open(
                std::env::var("FDB_CLUSTER_FILE").ok().as_deref(),
                world.relay_cell.as_deref().unwrap(),
            )
            .unwrap(),
        );
        if let Some(directory) = &managed_directory {
            let deployment = world
                .runtime
                .configured_image_deployment(&actor.identity, &device)
                .await
                .unwrap();
            let registry = zoen_relay::managed_devices::ManagedDevices::open([(
                deployment,
                directory.0.clone(),
            )])
            .unwrap();
            log.configure_managed_devices(registry).unwrap();
        }
        let mut store = Store::open(":memory:").unwrap();
        mls::migrate(store.conn_mut()).unwrap();
        let mut fixture = Self {
            world,
            log,
            store,
            second,
            agent,
            owner,
            space: new_id("retained-direct")?,
            head: None,
            managed_directory,
        };
        fixture
            .clear(EventBody::SpaceCreated {
                title: "Retained owner reply".into(),
                kind: SpaceKind::Direct,
                privacy: Privacy::EndToEnd,
            })
            .await?;
        fixture
            .clear(EventBody::MemberAdded {
                identity: fixture.agent.id.clone(),
                role: Role::Member,
            })
            .await?;
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
        fixture.sealed(SealedKind::Commit, commit.commit).await?;
        fixture
            .sealed(SealedKind::Welcome, commit.welcome.unwrap())
            .await?;
        Ok(fixture)
    }

    pub(super) fn device(&self) -> Device<'_> {
        Device::new(
            self.store.conn(),
            [79; 32],
            &self.world.owner.identity,
            self.world.owner.key.secret(),
            self.world.owner.cert.as_deref().unwrap(),
        )
        .unwrap()
    }

    pub(super) async fn append(&mut self, envelope: Envelope) -> Sequenced {
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

    pub(super) async fn clear(
        &mut self,
        body: EventBody,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let now = self.world.runtime.finance.reply_clock().await.unwrap();
        let event = self.world.owner.sign_event(
            &self.space,
            &new_id("clear")?,
            now,
            self.head.clone(),
            body,
        )?;
        self.append(Envelope::plain(&event)).await;
        Ok(())
    }

    pub(super) async fn sealed(
        &mut self,
        kind: SealedKind,
        data: Vec<u8>,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let now = self.world.runtime.finance.reply_clock().await.unwrap();
        let envelope = Envelope::sealed(
            &self.world.owner,
            &self.space,
            &new_id("handshake")?,
            now,
            self.head.as_ref(),
            Sealed::new(kind, SUITE_ID, data),
        );
        self.append(envelope).await;
        Ok(())
    }

    pub(super) async fn encrypted(
        &mut self,
        body: EventBody,
    ) -> std::result::Result<String, Box<dyn std::error::Error>> {
        let now = self.world.runtime.finance.reply_clock().await.unwrap();
        let client = new_id("inner")?;
        let event =
            self.world
                .owner
                .sign_event(&self.space, &client, now, self.head.clone(), body)?;
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
        Ok(self.append(envelope).await.hash)
    }

    pub(super) async fn grant(
        &mut self,
        grant: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        self.grant_until(grant, None).await?;
        Ok(())
    }

    pub(super) async fn grant_until(
        &mut self,
        grant: &str,
        expires_at_ms: Option<i64>,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
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
        .await?;
        Ok(())
    }

    pub(super) async fn message(
        &mut self,
        text: &str,
    ) -> std::result::Result<String, Box<dyn std::error::Error>> {
        Ok(self
            .encrypted(EventBody::MessagePosted {
                message: new_id("message")?,
                text: text.into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            })
            .await?)
    }

    pub(super) async fn sync(&self, second: bool) -> Result<ReplySync, RuntimeError> {
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

    pub(super) fn restore_custody(&self) -> native::NativeCustody {
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

    pub(super) async fn finish(self) {
        self.log.drop_cell().await.unwrap();
        self.world.finish().await;
    }
}

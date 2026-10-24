//! Agent citizens in the directory (migration 0028): who may claim, define, pause and
//! revoke an agent, and the runtime's cursors as the single source of truth.
use event_log::{agent_owner, Author, Signer};
use protocol::AgentAction;
use sqlx::{Connection, PgConnection, PgPool};
use types::{Identity, IdentityKind};
use zoen_relay::agents::{self, AgentCitizenError, CitizenClaim};
use zoen_relay::db;

struct Database {
    pool: PgPool,
    admin: String,
    name: String,
}

impl Database {
    async fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let admin = std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG");
        let name = format!("zoen_agents_{}", &Signer::generate()?.id()[..16]);
        let mut c = PgConnection::connect(&admin).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
            .execute(&mut c)
            .await
            .unwrap();
        let base = admin.rsplit_once('/').unwrap().0;
        let pool = PgPool::connect(&format!("{base}/{name}")).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        Ok(Self { pool, admin, name })
    }

    async fn finish(self) {
        self.pool.close().await;
        let mut c = PgConnection::connect(&self.admin).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE {} WITH (FORCE)",
            self.name
        )))
        .execute(&mut c)
        .await
        .unwrap();
    }

    async fn register(&self, profile: &Identity, author: &Author) {
        let mut tx = self.pool.begin().await.unwrap();
        db::register(
            &mut tx,
            profile,
            &profile.handle,
            author.device.as_deref().unwrap(),
            author.cert.as_deref().unwrap(),
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }

    async fn person(
        &self,
        handle: &str,
    ) -> std::result::Result<(Identity, Author), Box<dyn std::error::Error>> {
        let root = Signer::generate()?;
        let author = Author::device(&root, Signer::generate()?);
        let p = Identity {
            owner_proof: None,
            id: root.id(),
            kind: IdentityKind::Person,
            name: handle.into(),
            handle: handle.into(),
            tint_hex: "#123456".into(),
            glyph: None,
            owner: None,
            bio: String::new(),
        };
        self.register(&p, &author).await;
        Ok((p, author))
    }

    /// An agent registered from its own runtime device, authorized by `owner`'s device.
    async fn agent(
        &self,
        handle: &str,
        owner: &Identity,
        owner_dev: &Author,
    ) -> std::result::Result<(Identity, Author), Box<dyn std::error::Error>> {
        let root = Signer::generate()?;
        let author = Author::device(&root, Signer::generate()?);
        let proof = agent_owner::authorize(owner_dev, &root.id()).unwrap();
        let p = Identity {
            owner_proof: Some(Box::new(proof)),
            id: root.id(),
            kind: IdentityKind::Agent,
            name: handle.into(),
            handle: handle.into(),
            tint_hex: "#19B5A5".into(),
            glyph: Some("calendar".into()),
            owner: Some(owner.id.clone()),
            bio: String::new(),
        };
        self.register(&p, &author).await;
        Ok((p, author))
    }

    async fn claim(&self, agent: &str, device: &str) -> Result<(), AgentCitizenError> {
        let mut tx = self.pool.begin().await.unwrap();
        let r = agents::claim_citizen(
            &mut tx,
            &CitizenClaim {
                agent: agent.into(),
                device: device.into(),
                avatar_style: "hand-drawn-animated".into(),
                avatar_glyph: None,
                avatar_tint: None,
                avatar_asset: None,
            },
        )
        .await;
        tx.commit().await.unwrap();
        r
    }
}

fn dev(a: &Author) -> &str {
    a.device.as_deref().unwrap()
}

#[tokio::test]
async fn only_the_agents_own_active_device_claims_it(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let d = Database::new().await?;
    let (bruno, bruno_dev) = d.person("bruno").await?;
    let (carla, carla_dev) = d.person("carla").await?;
    let (bot, bot_dev) = d.agent("agenda_bruno", &bruno, &bruno_dev).await?;

    // Registration already claimed it, owner-bound, with no definition yet.
    let st = agents::state(&d.pool, &bot.id, dev(&bot_dev), &[])
        .await
        .unwrap();
    assert_eq!(st.status, "active");
    assert_eq!(st.definition_version, 0);
    assert!(st.kind.is_empty());

    // A non-owner's device can't claim the agent; a person can't be claimed at all.
    assert!(matches!(
        d.claim(&bot.id, dev(&carla_dev)).await,
        Err(AgentCitizenError::Device)
    ));
    assert!(matches!(
        d.claim(&carla.id, dev(&carla_dev)).await,
        Err(AgentCitizenError::NotAgent)
    ));

    // A revoked device can't claim, even the agent's own.
    let mut tx = d.pool.begin().await.unwrap();
    assert!(db::revoke_device(&mut tx, &bot.id, dev(&bot_dev))
        .await
        .unwrap());
    tx.commit().await.unwrap();
    assert!(matches!(
        d.claim(&bot.id, dev(&bot_dev)).await,
        Err(AgentCitizenError::Device)
    ));
    d.finish().await;
    Ok(())
}

#[tokio::test]
async fn definitions_are_versioned_and_owner_controlled(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let d = Database::new().await?;
    let (bruno, bruno_dev) = d.person("bruno").await?;
    let (carla, _) = d.person("carla").await?;
    let (bot, bot_dev) = d.agent("agenda_bruno", &bruno, &bruno_dev).await?;
    let bd = dev(&bot_dev);
    let bruno_d = dev(&bruno_dev);

    // The runtime can't make itself a Creator, not even with its first definition.
    assert!(matches!(
        agents::define(&d.pool, &bot.id, bd, &bot.id, 1, "Creator", b"{}").await,
        Err(AgentCitizenError::Owner)
    ));
    // The runtime records its first definition at registration, once.
    agents::define(&d.pool, &bot.id, bd, &bot.id, 1, "User", b"{\"v\":1}")
        .await
        .unwrap();
    assert!(matches!(
        agents::define(&d.pool, &bot.id, bd, &bot.id, 2, "Creator", b"{}").await,
        Err(AgentCitizenError::Owner)
    ));
    // Strangers can't; the owner can, and versions only grow.
    assert!(matches!(
        agents::define(&d.pool, &carla.id, "x", &bot.id, 2, "User", b"{}").await,
        Err(AgentCitizenError::Owner)
    ));
    assert!(matches!(
        agents::define(&d.pool, &bruno.id, bruno_d, &bot.id, 1, "User", b"{}").await,
        Err(AgentCitizenError::Version)
    ));
    agents::define(
        &d.pool,
        &bruno.id,
        bruno_d,
        &bot.id,
        2,
        "User",
        b"{\"v\":2}",
    )
    .await
    .unwrap();
    let st = agents::state(&d.pool, &bot.id, bd, &[]).await.unwrap();
    assert_eq!((st.kind.as_str(), st.definition_version), ("User", 2));
    assert_eq!(st.definition, b"{\"v\":2}");
    let history: i64 =
        sqlx::query_scalar("SELECT count(*) FROM agent_definitions WHERE agent = $1")
            .bind(&bot.id)
            .fetch_one(&d.pool)
            .await
            .unwrap();
    assert_eq!(history, 2);

    // Pause and revoke: owner only; revoke is final and takes the runtime device too.
    assert!(matches!(
        agents::control(&d.pool, &carla.id, &bot.id, AgentAction::Pause).await,
        Err(AgentCitizenError::Owner)
    ));
    agents::control(&d.pool, &bruno.id, &bot.id, AgentAction::Pause)
        .await
        .unwrap();
    assert_eq!(
        agents::state(&d.pool, &bot.id, bd, &[])
            .await
            .unwrap()
            .status,
        "paused"
    );
    // The owner's app reads the same status; nobody else can.
    let seen = agents::owner_status(&d.pool, &bruno.id, &bot.id)
        .await
        .unwrap();
    assert_eq!(seen.status, "paused");
    assert_eq!(seen.definition, b"{\"v\":2}");
    assert!(seen.cursors.is_empty());
    assert!(matches!(
        agents::owner_status(&d.pool, &carla.id, &bot.id).await,
        Err(AgentCitizenError::Owner)
    ));
    agents::control(&d.pool, &bruno.id, &bot.id, AgentAction::Resume)
        .await
        .unwrap();
    assert_eq!(
        agents::owner_status(&d.pool, &bruno.id, &bot.id)
            .await
            .unwrap()
            .status,
        "active"
    );
    agents::control(&d.pool, &bruno.id, &bot.id, AgentAction::Revoke)
        .await
        .unwrap();
    assert_eq!(
        agents::state(&d.pool, &bot.id, bd, &[])
            .await
            .unwrap()
            .status,
        "revoked"
    );
    let device_revoked: bool =
        sqlx::query_scalar("SELECT revoked_at IS NOT NULL FROM devices WHERE device = $1")
            .bind(bd)
            .fetch_one(&d.pool)
            .await
            .unwrap();
    assert!(device_revoked);
    assert_eq!(
        agents::owner_status(&d.pool, &bruno.id, &bot.id)
            .await
            .unwrap()
            .status,
        "revoked"
    );
    assert!(matches!(
        agents::advance_cursor(&d.pool, &bot.id, bd, "sp", 1).await,
        Err(AgentCitizenError::Revoked)
    ));
    assert!(matches!(
        agents::control(&d.pool, &bruno.id, &bot.id, AgentAction::Resume).await,
        Err(AgentCitizenError::Revoked)
    ));
    d.finish().await;
    Ok(())
}

#[tokio::test]
async fn cursors_never_move_back_and_leave_with_the_space(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let d = Database::new().await?;
    let (bruno, bruno_dev) = d.person("bruno").await?;
    let (bot, bot_dev) = d.agent("agenda_bruno", &bruno, &bruno_dev).await?;
    let bd = dev(&bot_dev);
    agents::advance_cursor(&d.pool, &bot.id, bd, "sp_a", 5)
        .await
        .unwrap();
    agents::advance_cursor(&d.pool, &bot.id, bd, "sp_a", 3)
        .await
        .unwrap();
    agents::advance_cursor(&d.pool, &bot.id, bd, "sp_b", 2)
        .await
        .unwrap();
    // Only the claimed runtime device moves cursors.
    assert!(
        agents::advance_cursor(&d.pool, &bot.id, dev(&bruno_dev), "sp_a", 9)
            .await
            .is_err()
    );
    let st = agents::state(&d.pool, &bot.id, bd, &["sp_a".into()])
        .await
        .unwrap();
    assert_eq!(st.cursors, vec![("sp_a".to_string(), 5)]);
    // Reads filter membership but cannot destroy a physical high watermark; an
    // unproven timestamp cleanup fence must not erase a concurrent Advance.
    agents::advance_cursor(&d.pool, &bot.id, bd, "sp_b", 9)
        .await
        .unwrap();
    let restored = agents::state(&d.pool, &bot.id, bd, &["sp_b".into()])
        .await
        .unwrap();
    assert_eq!(restored.cursors, vec![("sp_b".into(), 9)]);
    d.finish().await;
    Ok(())
}

#[tokio::test]
async fn competing_exact_next_definitions_do_not_overwrite_and_owner_reads_exact_bounded_bytes(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let d = Database::new().await?;
    let (owner, owner_device) = d.person("definition_owner").await?;
    let (stranger, _) = d.person("definition_stranger").await?;
    let (agent, runtime) = d.agent("definition_agent", &owner, &owner_device).await?;
    agents::define(
        &d.pool,
        &agent.id,
        dev(&runtime),
        &agent.id,
        1,
        "User",
        b"initial",
    )
    .await
    .unwrap();
    let (a, b) = tokio::join!(
        agents::define(
            &d.pool,
            &owner.id,
            dev(&owner_device),
            &agent.id,
            2,
            "User",
            b"reviewed-a"
        ),
        agents::define(
            &d.pool,
            &owner.id,
            dev(&owner_device),
            &agent.id,
            2,
            "User",
            b"reviewed-b"
        )
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(matches!(
        if a.is_err() { a } else { b },
        Err(AgentCitizenError::Version)
    ));
    let current = agents::owner_status(&d.pool, &owner.id, &agent.id)
        .await
        .unwrap();
    assert_eq!(current.definition_version, 2);
    assert!(current.definition == b"reviewed-a" || current.definition == b"reviewed-b");
    assert!(current.cursors.is_empty());
    assert!(matches!(
        agents::owner_status(&d.pool, &stranger.id, &agent.id).await,
        Err(AgentCitizenError::Owner)
    ));
    agents::define(
        &d.pool,
        &owner.id,
        dev(&owner_device),
        &agent.id,
        3,
        "User",
        b"concurrent-forward",
    )
    .await
    .unwrap();
    assert!(matches!(
        agents::define(
            &d.pool,
            &owner.id,
            dev(&owner_device),
            &agent.id,
            2,
            "User",
            b"stale-next"
        )
        .await,
        Err(AgentCitizenError::Version)
    ));
    assert_eq!(
        agents::owner_status(&d.pool, &owner.id, &agent.id)
            .await
            .unwrap()
            .definition,
        b"concurrent-forward"
    );
    // Direct storage-layer legacy/oversized input cannot produce a partial read.
    agents::define(
        &d.pool,
        &owner.id,
        dev(&owner_device),
        &agent.id,
        4,
        "User",
        &vec![0; 65537],
    )
    .await
    .unwrap();
    let oversized = agents::owner_status(&d.pool, &owner.id, &agent.id)
        .await
        .unwrap();
    assert_eq!(oversized.definition_version, 4);
    assert!(oversized.definition.is_empty() && oversized.cursors.is_empty());
    d.finish().await;
    Ok(())
}

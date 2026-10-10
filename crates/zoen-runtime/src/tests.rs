//! Real service primitives; seeded core scopes are not certified MLS integration.
mod deployment;
mod failure_cuts;
mod financial;
mod http;
mod journeys;
mod retained;
mod stores;
use super::*;
use roda_log::{Author, Signer};
use roda_types::{Identity, IdentityKind};
use sqlx::{Connection, PgConnection, PgPool};
use std::time::Duration;
use zoen_models::{EndpointPolicy, InputMessage, ModelLimits, Operation};

const PROMPT: &str = "PRIVATE-RUNTIME-PROMPT-CANARY-91f";
const API_KEY: &str = "PUBLIC-FIXTURE-KEY-CANARY-62a";
const BODY: &str = "PRIVATE-RUNTIME-REPLY-CANARY-23b";

struct World {
    runtime: Arc<RuntimeAuthority>,
    http: http::Provider,
    owner: Author,
    agents: Vec<Author>,
    agent_roots: Vec<Signer>,
    signed: SignedOwnerPolicy,
    policy_digest: String,
    admin: String,
    database: String,
    config_namespace: String,
    db_url: String,
    relay_cell: Option<String>,
}
impl World {
    async fn new() -> Self {
        Self::with_initial_workers(1, false).await
    }
    async fn with_initial_workers(workers: usize, competing_namespaces: bool) -> Self {
        Self::build(workers, competing_namespaces, false).await
    }
    async fn with_relay() -> Self {
        Self::build(1, false, true).await
    }
    async fn build(workers: usize, competing_namespaces: bool, relay: bool) -> Self {
        let admin = std::env::var("ZOEN_TEST_PG").expect("real Postgres fixture required");
        let database = format!("zoen_authority_{}", &Signer::generate().id()[..16]);
        let mut connection = PgConnection::connect(&admin).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {database}")))
            .execute(&mut connection)
            .await
            .unwrap();
        let db_url = format!("{}/{}", admin.rsplit_once('/').unwrap().0, database);
        let pool = PgPool::connect(&db_url).await.unwrap();
        sqlx::migrate!("../zoen-relay/migrations")
            .run(&pool)
            .await
            .unwrap();
        let http = http::Provider::new().await;
        let namespace = roda_types::new_id("authority");
        let mut openers = Vec::new();
        for _ in 0..workers {
            let candidate = if competing_namespaces {
                roda_types::new_id("authority")
            } else {
                namespace.clone()
            };
            let mut initial = config(&db_url, &candidate, &http.base);
            initial.relay_cell = relay.then(|| candidate.clone());
            openers.push(tokio::spawn(async move {
                (candidate, RuntimeAuthority::open(initial).await)
            }));
        }
        let mut successful = Vec::new();
        for opener in openers {
            let (candidate, result) = opener.await.unwrap();
            match result {
                Ok(runtime) => successful.push((candidate, runtime)),
                Err(error) => {
                    assert!(competing_namespaces);
                    assert_eq!(error, RuntimeError::DeploymentMismatch);
                }
            }
        }
        assert_eq!(
            successful.len(),
            if competing_namespaces { 1 } else { workers }
        );
        let (namespace, runtime) = successful.remove(0);
        let runtime = Arc::new(runtime);
        for (_, worker) in successful {
            worker.finance.pool.close().await;
        }
        pool.close().await;
        let owner = Author::device(&Signer::generate(), Signer::generate());
        register(&runtime.finance.pool, &owner, None, "owner").await;
        let mut agents = Vec::new();
        let mut agent_roots = Vec::new();
        for handle in ["agent_one", "agent_two"] {
            let root = Signer::generate();
            let agent = Author::device(&root, Signer::generate());
            register(&runtime.finance.pool, &agent, Some(&owner), handle).await;
            agents.push(agent);
            agent_roots.push(root);
        }
        let (year, month, _, end) = runtime.finance.current_period().await;
        let signed = roda_log::owner_budget::sign(
            &owner,
            OwnerPeriodPolicy {
                owner: owner.identity.clone(),
                year,
                month,
                version: 1,
                previous_digest: None,
                currency: "USD".into(),
                scale: 6,
                limit_units: 1000,
                max_attempt_units: 1000,
                expires_at_ms: end,
                enabled: true,
                allowed_profiles: vec![runtime.price.digest().unwrap()],
            },
        )
        .unwrap();
        let policy_digest = runtime.install_policy(signed.clone()).await.unwrap();
        // Explicit fresh isolated fixture continuity, never a production entry point.
        runtime.continuity.store(true, Ordering::SeqCst);
        Self {
            runtime,
            http,
            owner,
            agents,
            agent_roots,
            signed,
            policy_digest,
            admin,
            database,
            config_namespace: namespace.clone(),
            db_url,
            relay_cell: relay.then(|| namespace.clone()),
        }
    }
    async fn step(&self, agent: usize) -> VerifiedStep {
        let actor = &self.agents[agent];
        let period_start = finance::period(self.signed.policy.year, self.signed.policy.month)
            .unwrap()
            .0;
        let step = VerifiedStep::Fixture(Box::new(FixtureStep {
            request: ModelRequest {
                context: AttemptContext {
                    attempt_id: roda_types::new_id("attempt"),
                    run_id: roda_types::new_id("run"),
                    owner: self.owner.identity.clone(),
                    agent: actor.identity.clone(),
                    device: actor.device.clone().unwrap(),
                    space: roda_types::new_id("sp"),
                    authority_version: "fixture-frontier-v1".into(),
                    definition_version: "fixture-definition-v1".into(),
                    price_version: "fixture-price-v1".into(),
                },
                operation: Operation::ChatCompletion,
                messages: vec![InputMessage::User {
                    text: PROMPT.into(),
                }],
                tools: Vec::new(),
                max_output_tokens: 3,
            },
            device_cert: actor.cert.clone().unwrap(),
            frontier: "fixture-frontier-v1".into(),
            policy_digest: self.policy_digest.clone(),
            period_start,
            run_fence: execution::Fence {
                holder: "fixture-worker".into(),
                token: 1,
            },
            device_fence: execution::Fence {
                holder: "fixture-worker".into(),
                token: 1,
            },
        }));
        self.runtime
            .execution
            .seed(&step, &self.runtime.custody)
            .await
            .unwrap();
        step
    }
    fn binding(&self, step: &VerifiedStep) -> Binding {
        let step = step.fixture();
        Binding {
            context: step.request.context.clone(),
            device_cert: step.device_cert.clone(),
            frontier: step.frontier.clone(),
            period_start: step.period_start,
            policy_digest: step.policy_digest.clone(),
            request_digest: hash(b"explicit-finance-fixture"),
            requested_output_tokens: 3,
            hold_units: 13,
            price: self.runtime.price.clone(),
        }
    }
    async fn balance(&self) -> PeriodBalance {
        self.runtime
            .balance(
                &self.owner.identity,
                self.signed.policy.year,
                self.signed.policy.month,
            )
            .await
            .unwrap()
    }
    async fn reopen(&self) -> Arc<RuntimeAuthority> {
        let mut reopened_config = config(&self.db_url, &self.config_namespace, &self.http.base);
        reopened_config.relay_cell = self.relay_cell.clone();
        let runtime = Arc::new(RuntimeAuthority::open(reopened_config).await.unwrap());
        // Test continuity retained outside the two store snapshots. This proves
        // process reopening, not a production nonrollback restore witness.
        runtime.continuity.store(true, Ordering::SeqCst);
        runtime
    }
    async fn count(&self, query: &'static str) -> i64 {
        sqlx::query_scalar(query)
            .fetch_one(&self.runtime.finance.pool)
            .await
            .unwrap()
    }
    async fn audit(&self) {
        let mismatch: i64 = sqlx::query_scalar("SELECT count(*) FROM runtime_periods r WHERE r.held_units::NUMERIC <> (SELECT coalesce(sum(p.units::NUMERIC),0) FROM runtime_postings p JOIN runtime_attempts a USING(attempt) WHERE a.owner=r.owner AND a.period_start=r.period_start AND p.account='held') OR r.spent_units::NUMERIC <> (SELECT coalesce(sum(p.units::NUMERIC),0) FROM runtime_postings p JOIN runtime_attempts a USING(attempt) WHERE a.owner=r.owner AND a.period_start=r.period_start AND p.account='expense')")
            .fetch_one(&self.runtime.finance.pool).await.unwrap();
        assert_eq!(mismatch, 0);
        let unbalanced: i64 = sqlx::query_scalar("SELECT count(*) FROM (SELECT attempt,kind FROM runtime_postings GROUP BY attempt,kind HAVING sum(units::NUMERIC) <> 0) q")
            .fetch_one(&self.runtime.finance.pool).await.unwrap();
        assert_eq!(unbalanced, 0);
    }
    async fn finish(self) {
        self.audit().await;
        let trx = self.runtime.execution.db.create_trx().unwrap();
        let (begin, end) = self.runtime.execution.root.range();
        trx.clear_range(&begin, &end);
        trx.commit().await.unwrap();
        self.runtime.finance.pool.close().await;
        let mut connection = PgConnection::connect(&self.admin).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE {} WITH (FORCE)",
            self.database
        )))
        .execute(&mut connection)
        .await
        .unwrap();
    }
}
fn config(url: &str, namespace: &str, base: &str) -> RuntimeConfig {
    RuntimeConfig {
        postgres_url: url.into(),
        fdb_cluster_file: std::env::var("FDB_CLUSTER_FILE").ok(),
        namespace: namespace.into(),
        relay_cell: None,
        evidence_key: [71; 32],
        price: PriceProfile {
            version: "fixture-price-v1".into(),
            endpoint: format!("{base}/chat/completions"),
            model: "fixture-model".into(),
            credential_ref: "fixture-credential".into(),
            currency: "USD".into(),
            scale: 6,
            input_units_per_million: 1_000_000,
            output_units_per_million: 1_000_000,
            max_billable_input_tokens: 10,
            max_output_tokens: 8,
            max_request_bytes: 32768,
            max_response_bytes: 16384,
        },
        gateway: GatewayConfig {
            base_url: base.into(),
            endpoint_policy: EndpointPolicy::LoopbackFixture,
            model: "fixture-model".into(),
            credential_ref: "fixture-credential".into(),
            api_key: API_KEY.into(),
            limits: ModelLimits {
                max_request_bytes: 32768,
                max_response_bytes: 16384,
                max_output_tokens: 8,
                timeout: Duration::from_secs(3),
                connect_timeout: Duration::from_secs(1),
            },
        },
    }
}
async fn register(pool: &PgPool, actor: &Author, owner: Option<&Author>, handle: &str) {
    let profile = Identity {
        id: actor.identity.clone(),
        kind: if owner.is_some() {
            IdentityKind::Agent
        } else {
            IdentityKind::Person
        },
        name: handle.into(),
        handle: handle.into(),
        tint_hex: "#123456".into(),
        glyph: None,
        owner: owner.map(|a| a.identity.clone()),
        bio: String::new(),
        owner_proof: owner
            .map(|a| Box::new(roda_log::agent_owner::authorize(a, &actor.identity).unwrap())),
    };
    let mut tx = pool.begin().await.unwrap();
    zoen_relay::db::register(
        &mut tx,
        &profile,
        handle,
        actor.device.as_deref().unwrap(),
        actor.cert.as_deref().unwrap(),
        true,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}
#[test]
fn real_postgres_fdb_http_authority_journeys() {
    let network = unsafe { foundationdb::boot() };
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(journeys::run());
    drop(rt);
    drop(network);
}

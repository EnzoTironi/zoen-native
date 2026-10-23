//! Actual connection and HTTP gates, including a split WS/HTTP route.
use super::*;
use client_core::relay_output_transport::{RelayOutputError, RelayOutputTransport};
use futures_util::TryStreamExt;
use protocol::runtime_binding::{BlobBinding, Expectation, Request, BLOB_HEADER};

fn actor(f: &Fixture) -> Author {
    f.world.agents[0].clone()
}

fn expected(f: &Fixture, nonce: u8) -> Expectation {
    Expectation::new(
        &f.world.runtime.execution.deployment,
        Request {
            namespace: f.world.config_namespace.clone(),
            nonce: vec![nonce; 32],
        },
        f.world.relay_cell.as_deref().unwrap(),
        "image-output-real-relay-355",
        &actor(f).identity,
        &actor(f).key.id(),
    )
    .expect("actual pairing fixture")
}

async fn bound_put(
    relay: &relay::Relay,
    actor: &Author,
    binding: Option<&BlobBinding>,
    signature: &str,
    ts: i64,
    bytes: &[u8],
) -> u16 {
    let mut request = reqwest::Client::new()
        .put(format!("http://{}/v1/blobs/{}", relay.address, hash(bytes)))
        .header("x-zoen-device", actor.key.id())
        .header("x-zoen-ts", ts.to_string())
        .header("x-zoen-sig", signature);
    if let Some(binding) = binding {
        request = request.header(BLOB_HEADER, serde_json::to_string(binding).unwrap());
    }
    request
        .body(bytes.to_vec())
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

pub(super) async fn run(f: &Fixture, relay: &relay::Relay) {
    let url = format!("http://{}", relay.address);
    let transport = f
        .world
        .runtime
        .connect_bound_output(&url, &relay.state.relay_name, actor(f))
        .await
        .unwrap();
    // A real bound upload, through the production handler and object store.
    let bytes = vec![91; 47];
    let ts = f.world.runtime.finance.reply_clock().await.unwrap();
    assert_eq!(
        transport
            .upload(&hash(&bytes), bytes.clone(), ts)
            .await
            .unwrap()
            .blob(),
        hash(&bytes)
    );
    let scope = expected(f, 41);
    let binding = BlobBinding {
        request: scope.request().clone(),
        proof: scope.proof(),
        identity: actor(f).identity.clone(),
    };
    let signature = actor(f).key.sign(
        &binding
            .message(&hash(&bytes), ts, &relay.state.relay_name)
            .unwrap(),
    );
    assert_eq!(
        bound_put(relay, &actor(f), Some(&binding), &signature, ts, &bytes).await,
        200
    );
    // Removing the header cannot convert a bound signature to a legacy PUT.
    assert_eq!(
        bound_put(relay, &actor(f), None, &signature, ts, &bytes).await,
        401
    );
    let mut replayed = binding.clone();
    replayed.request.nonce[0] ^= 1;
    let signature = actor(f).key.sign(
        &replayed
            .message(&hash(&bytes), ts, &relay.state.relay_name)
            .unwrap(),
    );
    assert_eq!(
        bound_put(relay, &actor(f), Some(&replayed), &signature, ts, &bytes).await,
        403
    );

    // The real endpoint cannot answer for a different private SQL pairing.
    let wrong = Expectation::new(
        &"af".repeat(32),
        scope.request().clone(),
        &scope.proof().cell,
        &relay.state.relay_name,
        &actor(f).identity,
        &actor(f).key.id(),
    )
    .unwrap();
    assert!(matches!(
        RelayOutputTransport::connect_bound(&url, &relay.state.relay_name, actor(f), wrong).await,
        Err(RelayOutputError::Refused)
    ));

    let other_log = Arc::new(
        FdbLog::open(
            std::env::var("FDB_CLUSTER_FILE").ok().as_deref(),
            &new_id("binding-other-cell"),
        )
        .unwrap(),
    );
    let pool = sqlx::PgPool::connect(&f.world.db_url).await.unwrap();
    let other_cell = relay::Relay::start_with(f, pool, other_log).await;
    assert!(matches!(
        RelayOutputTransport::connect_bound(
            &format!("http://{}", other_cell.address),
            &other_cell.state.relay_name,
            actor(f),
            expected(f, 42)
        )
        .await,
        Err(RelayOutputError::Refused)
    ));
    let signature = actor(f).key.sign(
        &binding
            .message(&hash(&bytes), ts, &relay.state.relay_name)
            .unwrap(),
    );
    assert_eq!(
        bound_put(
            &other_cell,
            &actor(f),
            Some(&binding),
            &signature,
            ts,
            &bytes
        )
        .await,
        403
    );
    assert!(other_cell
        .state
        .blobs
        .list(None)
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .is_empty());
    other_cell.stop().await;

    // A split HTTP route into another actual SQL directory also refuses, even
    // while the correct websocket already passed the same-cell challenge.
    let other = Box::pin(World::new()).await;
    let pool = sqlx::PgPool::connect(&other.db_url).await.unwrap();
    let other_sql = relay::Relay::start_with(f, pool, f.log.clone()).await;
    assert_eq!(
        bound_put(
            &other_sql,
            &actor(f),
            Some(&binding),
            &signature,
            ts,
            &bytes
        )
        .await,
        403
    );
    assert!(other_sql
        .state
        .blobs
        .list(None)
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .is_empty());
    other_sql.stop().await;
    other.finish().await;
    // A previously valid socket/proof cannot keep a revoked device uploading.
    let retired = Author::device(&f.world.agent_roots[0], Signer::generate());
    let mut tx = zoen_relay::db::authorize_device(
        &f.world.runtime.finance.pool,
        &actor(f).identity,
        &actor(f).key.id(),
        true,
    )
    .await
    .unwrap()
    .unwrap();
    zoen_relay::db::enroll_device(
        &mut tx,
        &retired.identity,
        &retired.key.id(),
        retired.cert.as_deref().unwrap(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let retired_transport = f
        .world
        .runtime
        .connect_bound_output(&url, &relay.state.relay_name, retired.clone())
        .await
        .unwrap();
    sqlx::query("UPDATE devices SET revoked_at=now() WHERE device=$1")
        .bind(retired.key.id())
        .execute(&f.world.runtime.finance.pool)
        .await
        .unwrap();
    let refused = vec![92; 51];
    assert!(matches!(
        retired_transport
            .upload(&hash(&refused), refused.clone(), ts)
            .await,
        Err(RelayOutputError::Blob(
            client_core::blob_transport::BlobUploadError::Refused(403)
        ))
    ));
    let key = object_store::path::Path::from(format!(
        "blobs/{}/{}",
        &hash(&refused)[..2],
        hash(&refused)
    ));
    assert!(matches!(
        relay.state.blobs.get(&key).await,
        Err(object_store::Error::NotFound { .. })
    ));
    println!("runtime transport binding: real SQL/FDB/WS proof, signed bound PUT, wrong cell/SQL, challenge replay and header downgrade refusal PASS");
}

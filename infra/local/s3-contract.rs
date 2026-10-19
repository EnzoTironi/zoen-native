//! Real native object-store proof, compiled only in the owned local proof image.
//! Uses the relay's actual S3 builder. No fake store, public endpoint or credential argv.
use std::{fs::OpenOptions, io::Write, path::Path, sync::Arc, time::Duration};

use anyhow::{bail, ensure, Context, Result};
use futures_util::TryStreamExt;
use object_store::{
    path::Path as ObjectPath, ObjectStore, ObjectStoreExt, PutMode, PutOptions, PutPayload,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SIZE: usize = 256 * 1024 + 7;
const RECIPE: &str = "zoen-owned-s3-create-v1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    schema: u8,
    recipe: String,
    owner: String,
    bucket: String,
    key: String,
    bytes: usize,
    sha256: [String; 2],
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u8,
    phase: String,
    owner: String,
    key: String,
    winner: usize,
    sha256: String,
    bytes: usize,
    concurrent_created: u8,
    concurrent_already_exists: u8,
    retry_already_exists: u8,
    immutable_readback: bool,
    // The owner must separately prove an actual storage stop/restart. This
    // process observes retained bytes; it cannot attest that external action.
    storage_restart_performed_by_this_process: bool,
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn key(owner: &str) -> String {
    format!("__proofs/create-only/{owner}/object")
}

fn payload(owner: &str, variant: usize) -> Vec<u8> {
    let seed = Sha256::digest(format!("{RECIPE}:{owner}:{variant}").as_bytes());
    (0..SIZE)
        .map(|i| seed[i % seed.len()].wrapping_add((i.wrapping_mul(31) >> 8) as u8))
        .collect()
}

fn validate(plan: &Plan, bucket: &str) -> Result<[Vec<u8>; 2]> {
    ensure!(
        plan.owner.len() == 32
            && plan
                .owner
                .bytes()
                .all(|v| matches!(v, b'0'..=b'9' | b'a'..=b'f')),
        "owner must be a fresh 16-byte lowercase hex nonce"
    );
    ensure!(
        plan.schema == 1
            && plan.recipe == RECIPE
            && plan.bucket == bucket
            && plan.key == key(&plan.owner)
            && plan.bytes == SIZE,
        "invalid owned plan"
    );
    let bodies = [payload(&plan.owner, 0), payload(&plan.owner, 1)];
    ensure!(
        bodies[0] != bodies[1],
        "contenders must have different bytes"
    );
    ensure!(
        plan.sha256 == [sha(&bodies[0]), sha(&bodies[1])],
        "plan body hashes do not match the fixed recipe"
    );
    Ok(bodies)
}

fn write_new(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn owned_store() -> Result<(Arc<dyn ObjectStore>, String)> {
    let bucket =
        std::env::var("ZOEN_S3_BUCKET").context("S3 bucket required; local fallback forbidden")?;
    ensure!(
        bucket == "encrypted",
        "only the owned local encrypted bucket is supported"
    );
    let endpoint = std::env::var("AWS_ENDPOINT_URL_S3")
        .or_else(|_| std::env::var("AWS_ENDPOINT_URL"))
        .context("owned Docker S3 endpoint required")?;
    let endpoint = reqwest::Url::parse(&endpoint)?;
    ensure!(
        endpoint.scheme() == "http"
            && endpoint.host_str() == Some("storage")
            && endpoint.port() == Some(8333)
            && endpoint.path() == "/"
            && endpoint.username().is_empty()
            && endpoint.password().is_none()
            && endpoint.query().is_none()
            && endpoint.fragment().is_none(),
        "probe may only use http://storage:8333 on the owned private Docker network"
    );
    let (store, label) =
        zoen_relay::blobs::store_from_env(Path::new("/nonexistent-no-local-fallback"))?;
    ensure!(
        label == format!("s3://{bucket}"),
        "native S3 backend was not selected"
    );
    Ok((store, bucket))
}

async fn create(
    store: &dyn ObjectStore,
    location: &ObjectPath,
    body: &[u8],
) -> object_store::Result<()> {
    store
        .put_opts(
            location,
            PutPayload::from_bytes(body.to_vec().into()),
            PutOptions {
                mode: PutMode::Create,
                ..Default::default()
            },
        )
        .await?;
    Ok(())
}

async fn read_exact(store: &dyn ObjectStore, location: &ObjectPath, expected: &[u8]) -> Result<()> {
    let response = store.get(location).await?;
    ensure!(
        response.meta.size == SIZE as u64,
        "unexpected stored length"
    );
    let mut stream = response.into_stream();
    let mut bytes = Vec::with_capacity(SIZE);
    while let Some(part) = stream.try_next().await? {
        ensure!(
            part.len() <= SIZE - bytes.len(),
            "stored body exceeds owned fixed bound"
        );
        bytes.extend_from_slice(&part);
    }
    ensure!(
        bytes == expected,
        "immutable readback differs from winning contender"
    );
    ensure!(sha(&bytes) == sha(expected), "stored content hash changed");
    Ok(())
}

fn refused(result: object_store::Result<()>) -> Result<()> {
    match result {
        Err(object_store::Error::AlreadyExists { .. }) => Ok(()),
        Ok(()) => bail!("create-only retry overwrote an existing object"),
        Err(error) => {
            Err(error).context("expected native AlreadyExists, not a generic storage failure")
        }
    }
}

async fn run(args: &[String]) -> Result<()> {
    let (store, bucket) = owned_store()?;
    match args {
        [mode, owner, plan_path, result_path] if mode == "create" => {
            let bodies = [payload(owner, 0), payload(owner, 1)];
            let plan = Plan {
                schema: 1, recipe: RECIPE.into(), owner: owner.clone(), bucket,
                key: key(owner), bytes: SIZE, sha256: [sha(&bodies[0]), sha(&bodies[1])],
            };
            let bodies = validate(&plan, &plan.bucket)?;
            ensure!(!Path::new(result_path).exists(), "result path must be fresh");
            // Preserve the exact intended key/body recipe before the first write.
            write_new(Path::new(plan_path), &plan)?;
            let location = ObjectPath::from(plan.key.clone());
            match store.head(&location).await {
                Err(object_store::Error::NotFound { .. }) => {},
                Ok(_) => bail!("owned nonce key already exists; no write attempted"),
                Err(error) => return Err(error).context("fresh-key precondition failed"),
            }
            let start = tokio::sync::Barrier::new(2);
            let left = async {
                start.wait().await;
                create(store.as_ref(), &location, &bodies[0]).await
            };
            let right = async {
                start.wait().await;
                create(store.as_ref(), &location, &bodies[1]).await
            };
            let (left, right) = tokio::join!(left, right);
            let winner = match (left, right) {
                (Ok(()), Err(object_store::Error::AlreadyExists { .. })) => 0,
                (Err(object_store::Error::AlreadyExists { .. }), Ok(())) => 1,
                other => bail!("expected exactly one created/one AlreadyExists; actual {other:?}"),
            };
            read_exact(store.as_ref(), &location, &bodies[winner]).await?;
            // A loser cannot replace the winner; an exact retry is also create-only.
            refused(create(store.as_ref(), &location, &bodies[1 - winner]).await)?;
            refused(create(store.as_ref(), &location, &bodies[winner]).await)?;
            read_exact(store.as_ref(), &location, &bodies[winner]).await?;
            write_new(Path::new(result_path), &Receipt {
                schema: 1, phase: "create".into(), owner: plan.owner, key: plan.key,
                winner, sha256: plan.sha256[winner].clone(), bytes: SIZE,
                concurrent_created: 1, concurrent_already_exists: 1, retry_already_exists: 2,
                immutable_readback: true, storage_restart_performed_by_this_process: false,
            })?;
        },
        [mode, plan_path, create_path, result_path] if mode == "verify" => {
            let plan: Plan = serde_json::from_slice(&std::fs::read(plan_path)?)?;
            let bodies = validate(&plan, &bucket)?;
            let before: Receipt = serde_json::from_slice(&std::fs::read(create_path)?)?;
            ensure!(
                before.schema == 1 && before.phase == "create" && before.owner == plan.owner
                    && before.key == plan.key && before.winner < 2 && before.bytes == SIZE
                    && before.concurrent_created == 1 && before.concurrent_already_exists == 1
                    && before.retry_already_exists == 2 && before.immutable_readback
                    && !before.storage_restart_performed_by_this_process,
                "invalid original create receipt"
            );
            ensure!(before.sha256 == plan.sha256[before.winner], "create receipt hash mismatch");
            ensure!(!Path::new(result_path).exists(), "result path must be fresh");
            let location = ObjectPath::from(plan.key.clone());
            read_exact(store.as_ref(), &location, &bodies[before.winner]).await?;
            refused(create(store.as_ref(), &location, &bodies[1 - before.winner]).await)?;
            refused(create(store.as_ref(), &location, &bodies[before.winner]).await)?;
            read_exact(store.as_ref(), &location, &bodies[before.winner]).await?;
            write_new(Path::new(result_path), &Receipt { phase: "verify-retained".into(), ..before })?;
        },
        _ => bail!("usage: native-s3-contract create OWNER32HEX PLAN_NEW.json CREATE_NEW.json | verify PLAN.json CREATE.json VERIFY_NEW.json"),
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    // Total bound includes retries, request send and response stream consumption.
    let args: Vec<_> = std::env::args().skip(1).collect();
    tokio::time::timeout(Duration::from_secs(45), run(&args))
        .await
        .context("owned S3 contract deadline exceeded")??;
    Ok(())
}

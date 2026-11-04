//! Real SQL/FDB and retained local filesystem assertions. Row rewinds below
//! are scoped failure cuts, not a claim of full deployment backup/restore.
use super::*;
use relay_admission::managed_images::AnchorReader;
use std::path::Path;
mod recovery;
pub(super) mod snapshot;
type TestResult = Result<(), Box<dyn std::error::Error>>;
type Home = managed_images::Directory;

async fn fresh(w: &World) -> Result<RuntimeAuthority, RuntimeError> {
    fresh_at(w, &w.db_url).await
}
async fn fresh_at(w: &World, url: &str) -> Result<RuntimeAuthority, RuntimeError> {
    let mut cfg = config(url, &w.config_namespace, &w.http.base);
    cfg.relay_cell = w.relay_cell.clone();
    RuntimeAuthority::open(cfg).await
}

async fn empty_world() -> Result<World, Box<dyn std::error::Error>> {
    World::without_policy().await
}

async fn reader(runtime: &RuntimeAuthority, path: &Path) -> Result<AnchorReader, RuntimeError> {
    let binding = runtime
        .runtime_continuity
        .as_ref()
        .ok_or(RuntimeError::RestoreUnreconciled)?
        .lock()
        .await;
    AnchorReader::open_runtime(path, binding.test_deployment())
        .map_err(crate::managed_images::error)
}

fn revision(
    w: &World,
    previous: &SignedOwnerPolicy,
    enabled: bool,
) -> Result<SignedOwnerPolicy, Box<dyn std::error::Error>> {
    let mut next = previous.policy.clone();
    next.version += 1;
    next.previous_digest = event_log::owner_budget::digest(&previous.policy);
    next.enabled = enabled;
    Ok(event_log::owner_budget::sign(&w.owner, next).ok_or("fixture policy signature")?)
}

async fn no_financial_effects(w: &World) -> TestResult {
    for query in [
        "SELECT count(*) FROM runtime_attempts",
        "SELECT count(*) FROM runtime_claims",
        "SELECT count(*) FROM runtime_admission_witnesses",
        "SELECT count(*) FROM runtime_journals",
        "SELECT count(*) FROM runtime_postings",
        "SELECT count(*) FROM runtime_dispositions",
    ] {
        assert_eq!(
            w.count(query).await,
            0,
            "policy recovery must not manufacture financial authority: {query}"
        );
    }
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    Ok(())
}

pub(super) async fn run() -> TestResult {
    recovery::orphan_period_refuses_bootstrap().await?;
    let w = empty_world().await?;
    let home = Home::new()?;
    let runtime = fresh(&w)
        .await?
        .initialize_runtime_continuity(&home.0)
        .await?;
    let reader = reader(&runtime, &home.0).await?;
    let genesis = reader.checkpoint()?;
    assert_eq!(runtime.continuity(), Err(RuntimeError::RestoreUnreconciled));
    assert!(runtime.production_output_scope().is_err());
    assert_eq!(
        runtime.install_policy(w.signed.clone()).await?,
        w.policy_digest
    );
    assert_eq!(reader.checkpoint()?.sequence, 1);
    let snapshot = snapshot::Snapshot::capture(&w)?;
    let disabled = revision(&w, &w.signed, false)?;
    assert_eq!(
        w.runtime.install_policy(disabled.clone()).await,
        Err(RuntimeError::RestoreUnreconciled),
        "an unconfigured runtime cannot bypass a managed policy checkpoint"
    );
    assert_eq!(
        owner_policy_controls::guard(&w, &runtime)
            .await?
            .install_owner_policy(disabled.clone())
            .await,
        Err(RuntimeError::RestoreUnreconciled)
    );
    let installed = runtime
        .install_continuous_owner_policy(
            disabled.clone(),
            owner_policy_controls::guard(&w, &runtime).await?,
        )
        .await?;
    let latest = event_log::owner_budget::digest(&disabled.policy).ok_or("policy digest")?;
    assert_eq!(
        installed,
        OwnerPolicyInstallation::Installed(latest.clone())
    );
    let second_agent = &w.agents[1];
    let sqlx::types::Json(agent): sqlx::types::Json<Identity> =
        sqlx::query_scalar("SELECT profile FROM identities WHERE id=$1")
            .bind(&second_agent.identity)
            .fetch_one(&runtime.finance.pool)
            .await?;
    let sqlx::types::Json(owner): sqlx::types::Json<Identity> =
        sqlx::query_scalar("SELECT profile FROM identities WHERE id=$1")
            .bind(&w.owner.identity)
            .fetch_one(&runtime.finance.pool)
            .await?;
    let guard = runtime
        .finance
        .native_directory(
            &agent,
            &owner,
            second_agent.device.as_deref().ok_or("device")?,
            second_agent.cert.as_deref().ok_or("certificate")?,
        )
        .await?;
    assert_eq!(
        runtime
            .install_continuous_owner_policy(disabled.clone(), guard)
            .await?,
        OwnerPolicyInstallation::Installed(latest.clone())
    );
    let tip = reader.checkpoint()?;
    assert_eq!(tip.sequence,2,"a second agent observes the same owner policy rather than creating another balance/checkpoint");
    assert_eq!(reader.subject_commitments()?.len(), 1);
    assert_eq!(
        runtime.install_policy(w.signed.clone()).await?,
        w.policy_digest,
        "historical observation must not restore the old latest policy"
    );
    assert_eq!(reader.checkpoint()?, tip);
    let raw = std::fs::read_to_string(home.0.join("checkpoint.json"))?;
    assert!(!raw.contains("\"limit_units\""));
    assert!(!raw.contains(&w.owner.identity));
    let other = Home::new()?;
    assert!(matches!(
        fresh(&w)
            .await?
            .initialize_runtime_continuity(&other.0)
            .await,
        Err(RuntimeError::RestoreUnreconciled)
    ));
    assert!(!other.0.exists());
    assert!(
        fresh(&w)
            .await?
            .open_runtime_continuity(&home.0)
            .await
            .is_err(),
        "the real writer lock must remain exclusive"
    );
    drop(runtime);
    let runtime = fresh(&w).await?.open_runtime_continuity(&home.0).await?;
    assert_eq!(reader.checkpoint()?, tip);
    assert_eq!(runtime.install_policy(disabled.clone()).await?, latest);
    no_financial_effects(&w).await?;
    drop(runtime);
    snapshot::restored_policy_refuses(&w, &home, &snapshot, &tip).await?;
    recovery::matching_old_pair(&w, &home, &genesis, &tip).await?;
    w.finish().await;
    for cut in [80, 81, 82] {
        Box::pin(recovery::original_policy(cut)).await?;
    }
    Box::pin(recovery::revoked_owner_cannot_recover_policy()).await?;
    println!("shared policy continuity: original signed intents survive all three store cuts; policy-only/matching-old marker rewinds refuse; agents share one owner checkpoint; no paid work/output/refund authority PASS");
    Ok(())
}

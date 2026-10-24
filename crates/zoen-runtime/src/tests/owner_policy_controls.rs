//! Actual SQL scope/observation only; the separate IPC journey proves native MLS.
use super::*;

pub(super) async fn guard(
    w: &World,
    runtime: &RuntimeAuthority,
) -> Result<crate::finance::NativeDirectoryGuard, RuntimeError> {
    let actor = &w.agents[0];
    let sqlx::types::Json(agent): sqlx::types::Json<Identity> =
        sqlx::query_scalar("SELECT profile FROM identities WHERE id=$1")
            .bind(&actor.identity)
            .fetch_one(&runtime.finance.pool)
            .await
            .unwrap();
    let sqlx::types::Json(owner): sqlx::types::Json<Identity> =
        sqlx::query_scalar("SELECT profile FROM identities WHERE id=$1")
            .bind(&w.owner.identity)
            .fetch_one(&runtime.finance.pool)
            .await
            .unwrap();
    runtime
        .finance
        .native_directory(
            &agent,
            &owner,
            actor.device.as_deref().unwrap(),
            actor.cert.as_deref().unwrap(),
        )
        .await
}

pub(super) async fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new().await?;
    account_policy_context::run(&w).await;
    let now = w.runtime.finance.reply_clock().await.unwrap();
    let mut expiring = w.signed.policy.clone();
    expiring.version = 2;
    expiring.previous_digest = Some(w.policy_digest.clone());
    expiring.enabled = false;
    expiring.expires_at_ms = now + 3_000;
    let expiring = event_log::owner_budget::sign(&w.owner, expiring).unwrap();
    // Actual known SQL commit, deliberately discard the return before restart.
    w.runtime.install_policy(expiring.clone()).await.unwrap();
    let reopened = w.reopen().await;
    while reopened.finance.reply_clock().await.unwrap() < expiring.policy.expires_at_ms {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        reopened.install_policy(expiring.clone()).await.unwrap_err(),
        RuntimeError::InvalidPolicy,
        "generic live admission must still reject expiry"
    );
    let digest = event_log::owner_budget::digest(&expiring.policy).unwrap();
    assert_eq!(
        guard(&w, &reopened)
            .await
            .unwrap()
            .install_owner_policy(expiring.clone())
            .await
            .unwrap(),
        OwnerPolicyInstallation::Installed(digest.clone()),
        "historical ACK must observe the actual original row"
    );
    let mut conflict = expiring.policy.clone();
    conflict.limit_units += 1;
    let conflict = event_log::owner_budget::sign(&w.owner, conflict).unwrap();
    let conflicting_digest = event_log::owner_budget::digest(&conflict.policy).unwrap();
    assert_eq!(
        guard(&w, &reopened)
            .await
            .unwrap()
            .install_owner_policy(conflict)
            .await
            .unwrap(),
        OwnerPolicyInstallation::Refused(conflicting_digest)
    );
    let mut binding = w.binding(&w.step(0).await?);
    binding.policy_digest = digest.clone();
    assert_eq!(
        reopened.finance.reserve(&binding).await.unwrap_err(),
        RuntimeError::Denied,
        "historical Installed cannot reactivate an expired disabled policy"
    );

    let mut tx = reopened.finance.pool.begin().await.unwrap();
    assert!(zoen_relay::db::revoke_device(
        &mut tx,
        &w.owner.identity,
        w.owner.device.as_deref().unwrap()
    )
    .await
    .unwrap());
    tx.commit().await.unwrap();
    assert_eq!(
        guard(&w, &reopened)
            .await
            .unwrap()
            .install_owner_policy(expiring.clone())
            .await
            .unwrap(),
        OwnerPolicyInstallation::Installed(digest)
    );
    let previous = event_log::owner_budget::digest(&expiring.policy).unwrap();
    let mut successor = expiring.policy;
    successor.version = 3;
    successor.previous_digest = Some(previous);
    successor.expires_at_ms = w.signed.policy.expires_at_ms;
    let successor = event_log::owner_budget::sign(&w.owner, successor).unwrap();
    assert!(
        matches!(
            guard(&w, &reopened)
                .await
                .unwrap()
                .install_owner_policy(successor)
                .await
                .unwrap(),
            OwnerPolicyInstallation::Refused(_)
        ),
        "a revoked human device cannot install a new version"
    );
    let mut tx = reopened.finance.pool.begin().await.unwrap();
    assert!(zoen_relay::db::revoke_device(
        &mut tx,
        &w.agents[0].identity,
        w.agents[0].device.as_deref().unwrap()
    )
    .await
    .unwrap());
    tx.commit().await.unwrap();
    assert!(
        matches!(guard(&w, &reopened).await, Err(RuntimeError::Denied)),
        "current native directory refusal is an error, not a known policy Refused result"
    );
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 2);
    for query in [
        "SELECT count(*) FROM runtime_attempts",
        "SELECT count(*) FROM runtime_claims",
        "SELECT count(*) FROM runtime_journals",
        "SELECT count(*) FROM runtime_postings",
    ] {
        assert_eq!(w.count(query).await, 0);
    }
    assert_eq!(
        w.balance().await,
        PeriodBalance {
            held_units: 0,
            spent_units: 0
        }
    );
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    reopened.finance.pool.close().await;
    w.finish().await;
    println!("owner scoped policy: actual commit/return loss -> cold expiry historical ACK, conflicting version refused, disabled latest stays closed, revoked original cannot install successor and current native directory denial stays unknown; zero financial/provider effects PASS");
    Ok(())
}

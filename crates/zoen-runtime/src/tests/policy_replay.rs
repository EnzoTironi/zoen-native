//! Exact install observations are not active-budget or financial authority.
use super::*;

async fn unchanged(w: &World, policies: i64, latest: &str) {
    assert_eq!(w.count("SELECT count(*) FROM runtime_periods").await, 1);
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_policies").await,
        policies
    );
    let actual: String =
        sqlx::query_scalar("SELECT digest FROM runtime_policies ORDER BY version DESC LIMIT 1")
            .fetch_one(&w.runtime.finance.pool)
            .await
            .unwrap();
    assert_eq!(actual, latest);
    for query in [
        "SELECT count(*) FROM runtime_attempts",
        "SELECT count(*) FROM runtime_claims",
        "SELECT count(*) FROM runtime_admission_witnesses",
        "SELECT count(*) FROM runtime_journals",
        "SELECT count(*) FROM runtime_postings",
        "SELECT count(*) FROM runtime_dispositions",
        "SELECT count(*) FROM runtime_evidence",
    ] {
        assert_eq!(
            w.count(query).await,
            0,
            "policy replay must not create financial rows: {query}"
        );
    }
    assert_eq!(
        w.balance().await,
        PeriodBalance {
            held_units: 0,
            spent_units: 0
        }
    );
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
}

pub(super) async fn exact_restart_and_historical_replay(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new().await?;
    // The real SQL commit succeeds; its return is deliberately not retained.
    // This is lost-return simulation, not a PostgreSQL wire-level ACK cut.
    w.runtime.install_policy(w.signed.clone()).await.unwrap();
    let reopened = w.reopen().await;
    assert_eq!(
        reopened.install_policy(w.signed.clone()).await.unwrap(),
        w.policy_digest
    );
    let mut replays = Vec::new();
    for _ in 0..2 {
        let runtime = reopened.clone();
        let original = w.signed.clone();
        replays.push(tokio::spawn(async move {
            runtime.install_policy(original).await
        }));
    }
    for replay in replays {
        assert_eq!(replay.await.unwrap().unwrap(), w.policy_digest);
    }
    unchanged(&w, 1, &w.policy_digest).await;

    let mut disabled = w.signed.policy.clone();
    disabled.version = 2;
    disabled.previous_digest = Some(w.policy_digest.clone());
    disabled.enabled = false;
    let disabled = event_log::owner_budget::sign(&w.owner, disabled).unwrap();
    let latest = reopened.install_policy(disabled.clone()).await.unwrap();
    assert_ne!(latest, w.policy_digest);
    // An exact historical digest can be observed; it does not replace latest.
    assert_eq!(
        reopened.install_policy(w.signed.clone()).await.unwrap(),
        w.policy_digest
    );
    assert_eq!(
        reopened.install_policy(disabled.clone()).await.unwrap(),
        latest
    );
    let binding = w.binding(&w.step(0).await?);
    assert_eq!(
        reopened.finance.reserve(&binding).await.unwrap_err(),
        RuntimeError::Denied
    );
    let mut disabled_binding = binding.clone();
    disabled_binding.policy_digest = latest.clone();
    assert_eq!(
        reopened
            .finance
            .reserve(&disabled_binding)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    unchanged(&w, 2, &latest).await;

    let mut conflict = w.signed.policy.clone();
    conflict.limit_units += 1;
    let conflict = event_log::owner_budget::sign(&w.owner, conflict).unwrap();
    assert_eq!(
        reopened.install_policy(conflict).await.unwrap_err(),
        RuntimeError::InvalidBinding
    );
    let mut skipped = disabled.policy;
    skipped.version = 4;
    skipped.previous_digest = Some(latest.clone());
    let skipped = event_log::owner_budget::sign(&w.owner, skipped).unwrap();
    assert_eq!(
        reopened.install_policy(skipped).await.unwrap_err(),
        RuntimeError::InvalidPolicy
    );
    unchanged(&w, 2, &latest).await;

    let mut transaction = w.runtime.finance.pool.begin().await.unwrap();
    assert!(zoen_relay::db::revoke_device(
        &mut transaction,
        &w.owner.identity,
        w.owner.device.as_deref().unwrap()
    )
    .await
    .unwrap());
    transaction.commit().await.unwrap();
    assert_eq!(
        reopened.install_policy(w.signed.clone()).await.unwrap_err(),
        RuntimeError::Denied
    );
    unchanged(&w, 2, &latest).await;
    reopened.finance.pool.close().await;
    w.finish().await;
    println!("owner policy replay: actual SQL known-return loss, cold restart and concurrent exact observation, historical digest does not revive disabled latest, conflicting/skipped/revoked replay refuses with no financial/provider rows PASS");
    Ok(())
}

pub(super) async fn expired_exact_replay_does_not_reactivate(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new().await?;
    let now = w.runtime.finance.reply_clock().await.unwrap();
    let mut expiring = w.signed.policy.clone();
    expiring.version = 2;
    expiring.previous_digest = Some(w.policy_digest.clone());
    expiring.expires_at_ms = now + 3_000;
    let expiring = event_log::owner_budget::sign(&w.owner, expiring).unwrap();
    let digest = w.runtime.install_policy(expiring.clone()).await.unwrap();
    let deadline = expiring.policy.expires_at_ms;
    // Use the real SQL clock, not a changed policy row or fake expiry clock.
    loop {
        let now = w.runtime.finance.reply_clock().await.unwrap();
        if now >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(
            u64::try_from(deadline - now).unwrap() + 1,
        ))
        .await;
    }
    let reopened = w.reopen().await;
    assert_eq!(
        reopened.install_policy(expiring).await.unwrap_err(),
        RuntimeError::InvalidPolicy
    );
    let mut binding = w.binding(&w.step(0).await?);
    binding.policy_digest = digest.clone();
    assert_eq!(
        reopened.finance.reserve(&binding).await.unwrap_err(),
        RuntimeError::Denied
    );
    unchanged(&w, 2, &digest).await;
    reopened.finance.pool.close().await;
    w.finish().await;
    println!("owner policy expiry replay: real SQL deadline, cold exact replay refused, expired latest remains historical and creates no financial/provider rows PASS");
    Ok(())
}

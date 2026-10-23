//! No Request: paused actual agent host, explicit disable and cold current read.
use super::*;
use client_core::owner_policy::OwnerPolicyScope;

pub(super) async fn run(
    w: &World,
    path: &Path,
    owner: &Client,
    config: &config::Config,
    space: &str,
    provider: &Provider,
) {
    assert!(owner
        .requests()
        .iter()
        .all(|request| request.status != RequestStatus::Pending));
    let account = owner.account().unwrap();
    let scope = OwnerPolicyScope {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: space.into(),
        agent_id: config.agent.clone(),
    };
    owner
        .agent_control(config.agent.clone(), "pause".into())
        .await
        .unwrap();
    let mut host = Host::start(w, path).await;
    let sends = provider.sends.load(Ordering::SeqCst);
    let attempts = w.count("SELECT count(*) FROM runtime_attempts").await;
    let policies = w.count("SELECT count(*) FROM runtime_policies").await;
    let cursor = w
        .scalar("SELECT coalesce(max(last_seq),0)::text FROM agent_space_cursors")
        .await;
    let first = owner.request_account_policy_options(scope.clone()).unwrap();
    assert_eq!(first.state, "pending");
    assert!(first.context.is_none());
    let before = options(owner, &scope, &first.source, &mut host).await;
    assert_eq!(
        before.image_profile_digest,
        config.image_price.digest().unwrap()
    );
    assert_eq!(
        before.reservation_units,
        config.image_price.reservation_units.to_string()
    );
    let review = owner
        .prepare_owner_policy(
            scope.clone(),
            first.source,
            before.options_digest,
            before.current_limit_units.clone().unwrap(),
            before.current_max_attempt_units.clone().unwrap(),
            false,
        )
        .unwrap();
    let signed: zoen_runtime::OwnerPeriodPolicy =
        serde_json::from_str(&review.canonical_policy_json).unwrap();
    assert!(!signed.enabled);
    assert_eq!(signed.previous_digest, before.current_policy_digest);
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_policies").await,
        policies,
        "review did not install anything"
    );
    let submission = owner
        .submit_owner_policy(
            scope.clone(),
            review.canonical_policy_json,
            review.confirmation_digest,
        )
        .unwrap();
    assert_eq!(submission.state, "pending");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        host.alive();
        let status = owner
            .owner_policy_status(scope.clone(), submission.source.clone())
            .unwrap();
        assert_eq!(status.policy_digest, submission.policy_digest);
        assert_ne!(status.state, "refused");
        if status.state == "installed" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "disable receipt never arrived: {}",
            host.output()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    drop(host);
    let mut cold = Host::start(w, path).await;
    let again = owner.request_account_policy_options(scope.clone()).unwrap();
    let current = options(owner, &scope, &again.source, &mut cold).await;
    assert_eq!(current.current_enabled, Some(false));
    assert_eq!(
        current.current_policy_digest.as_deref(),
        Some(submission.policy_digest.as_str())
    );
    assert_eq!(current.current_limit_units, before.current_limit_units);
    assert_eq!(
        current.current_max_attempt_units,
        before.current_max_attempt_units
    );
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_policies").await,
        policies + 1
    );
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_attempts").await,
        attempts
    );
    assert_eq!(provider.sends.load(Ordering::SeqCst), sends);
    assert_eq!(
        w.scalar("SELECT coalesce(max(last_seq),0)::text FROM agent_space_cursors")
            .await,
        cursor
    );
    assert!(owner
        .requests()
        .iter()
        .all(|request| request.status != RequestStatus::Pending));
    drop(cold);
    println!("account budget product: no pending Request -> paused private typed inquiry -> actual quote/latest policy -> explicit signed disable -> exact Runtime Installed -> cold disabled current-context read, no model/provider/financial effects PASS");
}
async fn options(
    owner: &Client,
    scope: &OwnerPolicyScope,
    source: &str,
    host: &mut Host,
) -> client_core::owner_policy::OwnerPolicyContext {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        host.alive();
        let status = owner
            .owner_policy_options(scope.clone(), source.into())
            .unwrap();
        assert_ne!(status.state, "refused");
        if let Some(context) = status.context {
            return context;
        }
        assert!(
            Instant::now() < deadline,
            "account policy context absent: {}",
            host.output()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

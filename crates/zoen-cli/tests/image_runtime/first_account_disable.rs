//! First owner OFF confirmation travels through actual citizen IPC/Runtime SQL.
use super::*;
use client_core::owner_policy::OwnerPolicyScope;

pub(super) async fn run(
    w: &World,
    path: &Path,
    owner: Arc<Client>,
    config: &config::Config,
    space: &str,
    provider: &Provider,
) -> Result<(Arc<Client>, String), Box<dyn std::error::Error>> {
    let account = owner.account().ok_or("fixture owner account missing")?;
    let scope = OwnerPolicyScope {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: space.into(),
        agent_id: config.agent.clone(),
    };
    let pending = owner.request_account_policy_options(scope.clone())?;
    let mut host = Host::start(w, path).await?;
    let context = account_controls::options(&owner, &scope, &pending.source, &mut host).await;
    assert_eq!(context.current_policy_digest, None);
    assert_eq!(context.current_enabled, None);
    assert_eq!(context.existing_profile_count, 0);
    for (limit, attempt, enabled) in [("0", "1", false), ("1", "0", false), ("0", "0", true)] {
        assert!(owner
            .prepare_owner_policy(
                scope.clone(),
                pending.source.clone(),
                context.options_digest.clone(),
                limit.into(),
                attempt.into(),
                enabled,
            )
            .is_err());
    }
    let review = owner.prepare_owner_policy(
        scope.clone(),
        pending.source,
        context.options_digest,
        "0".into(),
        "0".into(),
        false,
    )?;
    let signed: zoen_runtime::OwnerPeriodPolicy =
        serde_json::from_str(&review.canonical_policy_json)?;
    assert_eq!(signed.owner, scope.expected_identity_id);
    assert!(!signed.enabled);
    assert_eq!((signed.limit_units, signed.max_attempt_units), (0, 0));
    assert_eq!(signed.version, 1);
    assert_eq!(signed.previous_digest, None);
    assert_eq!(signed.allowed_profiles, vec![config.image_price.digest()?]);
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_policies").await,
        0,
        "preparing the review must not install a policy"
    );
    assert!(
        owner
            .submit_owner_policy(
                scope.clone(),
                review.canonical_policy_json.clone(),
                "0".repeat(64),
            )
            .is_err(),
        "a nonmatching explicit confirmation must not submit"
    );
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 0);
    let submission = owner.submit_owner_policy(
        scope.clone(),
        review.canonical_policy_json,
        review.confirmation_digest,
    )?;
    assert_eq!(
        submission.state, "pending",
        "queued is not Runtime Installed"
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        host.alive();
        let status = owner.owner_policy_status(scope.clone(), submission.source.clone())?;
        assert_eq!(status.policy_digest, submission.policy_digest);
        assert_ne!(status.state, "refused");
        if status.state == "installed" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "first OFF receipt absent: {}",
            host.output()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 1);
    drop(host);
    owner.stop_sync();
    drop(owner);

    // Reopen both real Core state and the ordinary host/Runtime executable.
    // Cached owner options or a fabricated receipt cannot satisfy this read.
    let owner = owner_client(w);
    let mut cold = Host::start(w, path).await?;
    let installed = owner.owner_policy_status(scope.clone(), submission.source.clone())?;
    assert_eq!(installed.state, "installed");
    assert_eq!(installed.policy_digest, submission.policy_digest);
    let pending = owner.request_account_policy_options(scope.clone())?;
    let current = account_controls::options(&owner, &scope, &pending.source, &mut cold).await;
    assert_eq!(current.current_enabled, Some(false));
    assert_eq!(
        current.current_policy_digest,
        Some(submission.policy_digest.clone())
    );
    assert_eq!(current.current_limit_units.as_deref(), Some("0"));
    assert_eq!(current.current_max_attempt_units.as_deref(), Some("0"));
    assert_eq!(current.existing_profile_count, 1);
    assert_eq!(current.image_profile_digest, config.image_price.digest()?);
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_policies").await,
        1,
        "cold recovery must not install a duplicate"
    );
    for query in [
        "SELECT count(*) FROM runtime_attempts",
        "SELECT count(*) FROM runtime_claims",
        "SELECT count(*) FROM runtime_journals",
        "SELECT count(*) FROM runtime_postings",
    ] {
        assert_eq!(w.count(query).await, 0);
    }
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
    assert!(owner.requests().is_empty());
    drop(cold);
    println!("first account OFF: None -> exact service/zero limits signed once -> real Runtime Installed -> cold Core/Runtime same policy, zero attempts/provider/financial effects PASS");
    Ok((owner, submission.policy_digest))
}

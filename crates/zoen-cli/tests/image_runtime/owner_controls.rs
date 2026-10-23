//! Real encrypted owner inquiry -> Runtime quote -> explicit signed install.
use super::*;
use client_core::OwnerPolicyScope;

pub(super) async fn run(
    w: &World,
    path: &Path,
    owner: &Client,
    config: &config::Config,
    request: &AgentRequestDto,
    host: &mut Host,
    provider: &Provider,
) {
    let account = owner.account().unwrap();
    let scope = OwnerPolicyScope {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: request.space_id.clone(),
        agent_id: config.agent.clone(),
    };
    // Start the actual ordinary host while already paused: its model loop
    // must retain its cursor, while independently authenticated owner controls
    // can still read/change the budget. No sleep or forged agent status.
    host.child.kill().unwrap();
    host.child.wait().unwrap();
    owner
        .agent_control(config.agent.clone(), "pause".into())
        .await
        .unwrap();
    *host = Host::start(w, path).await;
    let model_cursor = w
        .scalar("SELECT coalesce(max(last_seq),0)::text FROM agent_space_cursors")
        .await;
    let hash = request.tool_approval.as_ref().unwrap().content_hash.clone();
    assert!(owner
        .request_owner_policy_options(scope.clone(), request.id.clone(), "0".repeat(64))
        .is_err());
    let pending = owner
        .request_owner_policy_options(scope.clone(), request.id.clone(), hash)
        .unwrap();
    assert_eq!(pending.state, "pending");
    assert!(pending.context.is_none());
    let deadline = Instant::now() + Duration::from_secs(60);
    let context = loop {
        host.alive();
        let status = owner
            .owner_policy_options(scope.clone(), pending.source.clone())
            .unwrap();
        assert_ne!(status.state, "refused", "typed Runtime options refused");
        if let Some(context) = status.context {
            break context;
        }
        assert!(
            Instant::now() < deadline,
            "actual owner options never arrived: {}",
            host.output()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(context.currency, config.image_price.currency);
    assert_eq!(context.scale, config.image_price.scale);
    assert_eq!(
        context.reservation_units,
        config.image_price.reservation_units.to_string()
    );
    assert_eq!(
        context.image_profile_digest,
        config.image_price.digest().unwrap()
    );
    assert_eq!(context.current_limit_units.as_deref(), Some("100"));
    assert!(context.current_policy_digest.is_some());
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 1);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
    let review = owner
        .prepare_owner_policy(
            scope.clone(),
            pending.source,
            context.options_digest,
            "200".into(),
            "20".into(),
            true,
        )
        .unwrap();
    let canonical: zoen_runtime::OwnerPeriodPolicy =
        serde_json::from_str(&review.canonical_policy_json).unwrap();
    assert_eq!(canonical.version, 2);
    assert_eq!(canonical.previous_digest, context.current_policy_digest);
    assert!(owner
        .submit_owner_policy(
            scope.clone(),
            review.canonical_policy_json.clone(),
            "0".repeat(64)
        )
        .is_err());
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 1);
    let submission = owner
        .submit_owner_policy(
            scope.clone(),
            review.canonical_policy_json,
            review.confirmation_digest,
        )
        .unwrap();
    assert_eq!(
        submission.state, "pending",
        "queued is not an installation receipt"
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        host.alive();
        let status = owner
            .owner_policy_status(scope.clone(), submission.source.clone())
            .unwrap();
        assert_eq!(status.policy_digest, submission.policy_digest);
        assert_ne!(
            status.state, "refused",
            "actual Runtime refused the exact owner policy"
        );
        if status.state == "installed" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "installation receipt absent: {}",
            host.output()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 2);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
    assert_eq!(
        w.scalar("SELECT coalesce(max(last_seq),0)::text FROM agent_space_cursors")
            .await,
        model_cursor,
        "paused model cursor advanced while owner controls were reconciled"
    );
    owner
        .agent_control(config.agent.clone(), "resume".into())
        .await
        .unwrap();
    println!("owner policy product: actual encrypted inquiry -> configured image quote/current signed version -> explicit digest confirm -> Runtime IPC install -> authenticated Installed receipt, zero attempts/provider sends PASS");
}

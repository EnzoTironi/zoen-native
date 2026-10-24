//! First typed account read, before native source discovery, policy or image setup.
use super::*;
use client_core::owner_policy::OwnerPolicyScope;

pub(super) async fn run(
    w: &World,
    path: &Path,
    owner: &Client,
    config: &config::Config,
    space: &str,
    provider: &Provider,
) -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 0);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
    assert!(owner.requests().is_empty());
    let account = owner.account().ok_or("fixture owner account missing")?;
    let scope = OwnerPolicyScope {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: space.into(),
        agent_id: config.agent.clone(),
    };
    let pending = owner.request_account_policy_options(scope.clone())?;
    let mut host = Host::start(w, path).await;
    let first = account_controls::options(owner, &scope, &pending.source, &mut host).await;
    assert_eq!(first.image_profile_digest, config.image_price.digest()?);
    assert_eq!(first.current_policy_digest, None);
    assert_eq!(first.current_enabled, None);
    drop(host);

    let mut cold = Host::start(w, path).await;
    let next = owner.request_account_policy_options(scope.clone())?;
    assert_ne!(next.source, pending.source);
    let current = account_controls::options(owner, &scope, &next.source, &mut cold).await;
    assert_eq!(current.image_profile_digest, first.image_profile_digest);
    assert_eq!(current.current_policy_digest, None);
    assert_eq!(current.current_enabled, None);
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 0);
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
    println!("first account context: real citizen/IPC/native MLS/directory/finance plus cold read, no prior image/policy/Request or provider/financial effects PASS");
    Ok(())
}

//! The worker and dedicated retained native device have independent MLS readiness.
use super::*;
use crate::owner_policy::OwnerPolicyContextRejection as Rejection;
use types::owner_policy_context::SignedOwnerPolicyInquiry;

fn inquiry(
    f: &mut Fixture,
    scope: &OwnerPolicyScope,
) -> StdResult<SignedOwnerPolicyInquiry, Box<dyn std::error::Error>> {
    let pending = f.owner.request_account_policy_options(scope)?;
    deliver_owner(f)?;
    Ok(f.core
        .engine
        .owner_policy_inquiry(&scope.space_id, &pending.source)?
        .signed()
        .clone())
}

#[test]
fn retained_account_context_uses_the_same_read_only_gate_after_cold_restore() -> TestResult {
    let (mut f, scope, _) = setup()?;
    let signed = inquiry(&mut f, &scope)?;
    let now = crate::engine::now_ms();
    let image = f.core.image()?;
    assert_eq!(f.core.owner_policy_account_admission(&signed, now), Ok(()));
    f.core.owner_policy_account_context(&signed, now)?;
    let cold = DeviceCore::restore(image, f.credential())?;
    let before = cold.image()?;
    assert_eq!(cold.owner_policy_account_admission(&signed, now), Ok(()));
    cold.owner_policy_account_context(&signed, now)?;
    assert_eq!(cold.image()?.as_bytes(), before.as_bytes());
    Ok(())
}

#[test]
fn worker_eligible_inquiry_does_not_prove_retained_native_group_readiness() -> TestResult {
    let (mut f, scope, _) = setup()?;
    let pending = f.owner.request_account_policy_options(&scope)?;
    deliver_owner(&mut f)?;
    let worker = DeviceCore::restore(f.core.image()?, f.credential())?;
    let query = worker
        .engine
        .owner_policy_inquiry(&scope.space_id, &pending.source)?;
    let now = crate::engine::now_ms();
    assert_eq!(
        worker.owner_policy_account_admission(query.signed(), now),
        Ok(())
    );
    f.core.engine.device()?.forget(&scope.space_id)?;
    assert_eq!(
        f.core.owner_policy_account_admission(query.signed(), now),
        Err(Rejection::MlsGroup)
    );
    assert!(f
        .core
        .owner_policy_account_context(query.signed(), now)
        .is_err());
    let cold = DeviceCore::restore(f.core.image()?, f.credential())?;
    let before = cold.image()?;
    assert_eq!(
        cold.owner_policy_account_admission(query.signed(), now),
        Err(Rejection::MlsGroup)
    );
    assert!(worker
        .engine
        .owner_policy_inquiry(&scope.space_id, &pending.source)
        .is_ok());
    assert_eq!(cold.image()?.as_bytes(), before.as_bytes());
    Ok(())
}

#[test]
fn canonical_reader_role_remains_denied_by_both_account_context_interfaces_after_reopen(
) -> TestResult {
    let (mut f, scope, _) = setup()?;
    let signed = inquiry(&mut f, &scope)?;
    f.clear(EventBody::MemberAdded {
        identity: scope.agent_id.clone(),
        role: Role::Reader,
    })?;
    let cold = DeviceCore::restore(f.core.image()?, f.credential())?;
    let before = cold.image()?;
    assert_eq!(
        cold.owner_policy_account_admission(&signed, crate::engine::now_ms()),
        Err(Rejection::Writer)
    );
    assert!(cold
        .owner_policy_account_context(&signed, crate::engine::now_ms())
        .is_err());
    assert_eq!(cold.image()?.as_bytes(), before.as_bytes());
    Ok(())
}

#[test]
fn account_admission_reports_expiry_at_exact_boundary_and_signature_without_refusal() -> TestResult
{
    let (mut f, scope, _) = setup()?;
    let signed = inquiry(&mut f, &scope)?;
    let now = signed.inquiry.at_ms;
    let ttl = types::owner_policy_context::CONTEXT_TTL_MS;
    assert_eq!(
        f.core.owner_policy_account_admission(&signed, now + ttl),
        Ok(())
    );
    assert_eq!(
        f.core
            .owner_policy_account_admission(&signed, now + ttl + 1),
        Err(Rejection::Freshness)
    );
    let mut forged = signed;
    forged.signature = "00".repeat(64);
    assert_eq!(
        f.core.owner_policy_account_admission(&forged, now),
        Err(Rejection::Signature)
    );
    Ok(())
}

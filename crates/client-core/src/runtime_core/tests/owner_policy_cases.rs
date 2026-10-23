//! Actual certified accounts + MLS transport; no synthetic install authority.
use super::*;
use crate::{
    owner_policy::{OwnerPolicyScope, OwnerPolicyStatus},
    CoreError,
};
use std::result::Result as StdResult;
use types::owner_budget::OwnerPeriodPolicy;
type TestResult = StdResult<(), Box<dyn std::error::Error>>;

fn setup() -> StdResult<(Fixture, OwnerPolicyScope, String), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new();
    fixture.owner.set_registered(true)?;
    fixture.core.engine.set_registered(true)?;
    let account = fixture.owner.account().ok_or("missing fixture account")?;
    let scope = OwnerPolicyScope {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: fixture.space.clone(),
        agent_id: fixture.agent_profile.id.clone(),
    };
    let policy = OwnerPeriodPolicy {
        owner: account.identity.clone(),
        year: 2026,
        month: 10,
        version: 1,
        previous_digest: None,
        currency: "USD".into(),
        scale: 6,
        limit_units: 12_340_000,
        max_attempt_units: 200_000,
        expires_at_ms: crate::engine::now_ms()
            .checked_add(3_600_000)
            .ok_or("clock overflow")?,
        enabled: true,
        allowed_profiles: vec!["11".repeat(32)],
    };
    Ok((fixture, scope, serde_json::to_string(&policy)?))
}
fn submit(
    f: &mut Fixture,
    scope: &OwnerPolicyScope,
    json: &str,
) -> StdResult<OwnerPolicyStatus, CoreError> {
    let review = f.owner.review_owner_policy(scope, json)?;
    f.owner.submit_owner_policy(
        scope,
        &review.canonical_policy_json,
        &review.confirmation_digest,
    )
}
fn deliver_owner(f: &mut Fixture) -> TestResult {
    f.owner.mls_seal_outbox()?;
    for envelope in f.owner.outbox_envelopes() {
        assert!(
            envelope.is_sealed(),
            "policy must never travel in clear JSON"
        );
        let frame = ordered(&f.owner, envelope);
        assert_eq!(f.owner.ingest(frame.clone()), Ingest::Confirmed);
        f.core.ingest(vec![frame])?;
    }
    Ok(())
}
fn deliver_agent(f: &mut Fixture) -> TestResult {
    f.core.engine.mls_seal_outbox()?;
    for envelope in f.core.engine.outbox_envelopes() {
        assert!(envelope.is_sealed());
        let frame = ordered(&f.owner, envelope);
        f.core.ingest(vec![frame.clone()])?;
        assert_eq!(f.owner.ingest(frame), Ingest::Applied);
    }
    Ok(())
}

#[test]
fn owner_policy_stays_pending_until_exact_confirmed_host_receipt_and_replays_one_original(
) -> TestResult {
    let (mut f, scope, json) = setup()?;
    let review = f.owner.review_owner_policy(&scope, &json)?;
    assert_eq!(review.limit_units, "12340000");
    assert_eq!(review.max_attempt_units, "200000");
    let pending = submit(&mut f, &scope, &json)?;
    assert_eq!(pending.state, "pending");
    assert_eq!(submit(&mut f, &scope, &json)?, pending);
    assert_eq!(f.owner.outbox_len(), 1);
    assert!(f
        .core
        .engine
        .owner_policy_submission(&scope.space_id, &pending.source)
        .is_err());
    deliver_owner(&mut f)?;
    let control = f
        .core
        .engine
        .owner_policy_submission(&scope.space_id, &pending.source)?;
    assert_eq!(control.agent(), scope.agent_id);
    assert_eq!(control.digest(), pending.policy_digest);
    assert_eq!(control.signed().policy.limit_units, 12_340_000);
    assert!(event_log::owner_budget::verify(control.signed()));
    assert_eq!(
        f.owner.owner_policy_status(&scope, &pending.source)?.state,
        "pending"
    );
    assert!(f
        .core
        .engine
        .report_owner_policy(&control, Some(&"ff".repeat(32)))
        .is_err());
    f.core
        .engine
        .report_owner_policy(&control, Some(control.digest()))?;
    f.core
        .engine
        .report_owner_policy(&control, Some(control.digest()))?;
    f.core.engine.report_owner_policy(&control, None)?;
    assert_eq!(
        f.core.engine.outbox_len(),
        1,
        "same result is one original receipt"
    );
    assert_eq!(
        f.owner.owner_policy_status(&scope, &pending.source)?.state,
        "pending"
    );
    deliver_agent(&mut f)?;
    assert_eq!(
        f.owner.owner_policy_status(&scope, &pending.source)?.state,
        "installed"
    );
    assert_eq!(submit(&mut f, &scope, &json)?.source, pending.source);
    assert_eq!(f.owner.outbox_len(), 0);
    Ok(())
}

#[test]
fn owner_policy_confirmation_binds_financial_fields_account_device_and_exact_dm() -> TestResult {
    let (mut f, scope, json) = setup()?;
    let review = f.owner.review_owner_policy(&scope, &json)?;
    let changed = json.replace("12340000", "12340001");
    assert!(f
        .owner
        .submit_owner_policy(&scope, &changed, &review.confirmation_digest)
        .is_err());
    let mut other = scope.clone();
    other.expected_device_id = "22".repeat(32);
    assert!(f.owner.review_owner_policy(&other, &json).is_err());
    other = scope.clone();
    other.agent_id = f.owner_profile.id.clone();
    assert!(f.owner.review_owner_policy(&other, &json).is_err());
    let mut policy: OwnerPeriodPolicy = serde_json::from_str(&json)?;
    policy.owner = f.agent_profile.id.clone();
    assert!(f
        .owner
        .review_owner_policy(&scope, &serde_json::to_string(&policy)?)
        .is_err());
    policy = serde_json::from_str(&json)?;
    policy.expires_at_ms = 1;
    assert!(f
        .owner
        .review_owner_policy(&scope, &serde_json::to_string(&policy)?)
        .is_err());
    let mut extra: serde_json::Value = serde_json::from_str(&json)?;
    extra["implicit_budget"] = true.into();
    assert!(f
        .owner
        .review_owner_policy(&scope, &extra.to_string())
        .is_err());
    let saved = f.owner.net.author.take();
    assert!(f.owner.review_owner_policy(&scope, &json).is_err());
    f.owner.net.author = saved;
    f.owner.store.set_meta(
        "devices.revoked",
        &serde_json::to_string(&vec![scope.expected_device_id.clone()])?,
    )?;
    assert!(f.owner.review_owner_policy(&scope, &json).is_err());
    assert_eq!(f.owner.outbox_len(), 0);
    Ok(())
}

#[test]
fn ordinary_prompt_json_is_not_a_control_and_explicit_refusal_is_not_installation() -> TestResult {
    let (mut f, scope, json) = setup()?;
    let ordinary = f.message(&json);
    assert!(f
        .core
        .engine
        .owner_policy_submission(&scope.space_id, &ordinary)
        .is_err());
    let status = submit(&mut f, &scope, &json)?;
    deliver_owner(&mut f)?;
    let control = f
        .core
        .engine
        .owner_policy_submission(&scope.space_id, &status.source)?;
    f.core.engine.report_owner_policy(&control, None)?;
    deliver_agent(&mut f)?;
    assert_eq!(
        f.owner.owner_policy_status(&scope, &status.source)?.state,
        "refused"
    );
    // Losing membership invalidates the retained control before the IPC effect.
    f.core
        .engine
        .state
        .spaces
        .get_mut(&scope.space_id)
        .ok_or("missing DM")?
        .members
        .retain(|(identity, _)| identity != &scope.expected_identity_id);
    assert!(f
        .core
        .engine
        .owner_policy_submission(&scope.space_id, &status.source)
        .is_err());
    assert!(f
        .core
        .engine
        .report_owner_policy(&control, Some(control.digest()))
        .is_err());
    Ok(())
}

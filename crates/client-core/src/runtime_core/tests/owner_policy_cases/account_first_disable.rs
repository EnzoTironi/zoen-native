//! First explicit disable binds the authenticated service without spending authority.
use super::*;
use chrono::Datelike;
use types::owner_policy_context::{OwnerPolicyImageSpec, OwnerPolicyOptions};

#[test]
fn first_account_disable_reviews_zero_only_and_retains_exact_receipt_after_cold() -> TestResult {
    let (mut f, scope, _) = setup()?;
    let pending = f.owner.request_account_policy_options(&scope)?;
    deliver_owner(&mut f)?;
    let query = f
        .core
        .engine
        .owner_policy_inquiry(&scope.space_id, &pending.source)?;
    let now = crate::engine::now_ms();
    let date = chrono::DateTime::from_timestamp_millis(now).ok_or("invalid clock")?;
    let profile = "22".repeat(32);
    let mut options = OwnerPolicyOptions {
        inquiry_digest: query.digest().into(),
        currency: "USD".into(),
        scale: 6,
        reservation_units: 100,
        image_profile_digest: profile.clone(),
        observed_at_ms: now,
        year: date.year(),
        month: u8::try_from(date.month())?,
        current_policy: None,
        current_policy_digest: None,
        image_spec: Some(OwnerPolicyImageSpec {
            size: "1024x1024".into(),
            quality: "low".into(),
        }),
    };
    f.core
        .engine
        .report_owner_policy_options(&query, options.clone())?;
    deliver_agent(&mut f)?;
    let context = f
        .owner
        .owner_policy_options(&scope, &pending.source)?
        .context
        .ok_or("missing account context")?;
    assert!(context.current_policy_digest.is_none());
    assert_eq!(context.existing_profile_count, 0);
    let before = f.core.image()?;
    for (limit, attempt, enabled) in [
        ("", "0", false),
        ("0", "1", false),
        ("1", "0", false),
        ("100", "100", false),
        ("0", "0", true),
    ] {
        assert!(f
            .owner
            .prepare_owner_policy(
                &scope,
                &pending.source,
                &context.options_digest,
                limit,
                attempt,
                enabled,
            )
            .is_err());
    }
    assert!(f
        .owner
        .prepare_owner_policy(&scope, &pending.source, &"ff".repeat(32), "0", "0", false)
        .is_err());
    assert!(f
        .owner
        .prepare_owner_policy(
            &scope,
            "other-source",
            &context.options_digest,
            "0",
            "0",
            false
        )
        .is_err());
    let review = f.owner.prepare_owner_policy(
        &scope,
        &pending.source,
        &context.options_digest,
        "0",
        "0",
        false,
    )?;
    let policy: OwnerPeriodPolicy = serde_json::from_str(&review.canonical_policy_json)?;
    assert!(policy.valid_shape());
    assert!(!policy.enabled);
    assert_eq!((policy.limit_units, policy.max_attempt_units), (0, 0));
    assert_eq!(policy.allowed_profiles, vec![profile.clone()]);
    assert_eq!(policy.version, 1);
    assert!(policy.previous_digest.is_none());
    assert_eq!(f.owner.outbox_len(), 0, "review never submits");
    assert_eq!(f.core.image()?.as_bytes(), before.as_bytes());
    assert!(f.owner.state.grants.is_empty());
    assert!(f.owner.state.requests.is_empty());
    let submitted = f.owner.submit_owner_policy(
        &scope,
        &review.canonical_policy_json,
        &review.confirmation_digest,
    )?;
    assert_eq!(submitted.state, "pending");
    deliver_owner(&mut f)?;
    f.core = DeviceCore::restore(f.core.image()?, f.credential())?;
    let control = f
        .core
        .engine
        .owner_policy_submission(&scope.space_id, &submitted.source)?;
    assert_eq!(control.signed().policy, policy);
    let signed = control.signed().clone();
    assert!(f
        .core
        .engine
        .report_owner_policy(&control, Some(&"ff".repeat(32)))
        .is_err());
    assert_eq!(
        f.owner
            .owner_policy_status(&scope, &submitted.source)?
            .state,
        "pending"
    );
    f.core
        .engine
        .report_owner_policy(&control, Some(control.digest()))?;
    deliver_agent(&mut f)?;
    let receipt = f.owner.owner_policy_status(&scope, &submitted.source)?;
    assert_eq!(receipt.state, "installed");
    assert_eq!(receipt.policy_digest, submitted.policy_digest);
    f.core = DeviceCore::restore(f.core.image()?, f.credential())?;
    assert_eq!(
        f.owner
            .find_owner_policy_submission(&scope, &submitted.policy_digest)?,
        Some(receipt)
    );

    let next = f.owner.request_account_policy_options(&scope)?;
    deliver_owner(&mut f)?;
    let next_query = f
        .core
        .engine
        .owner_policy_inquiry(&scope.space_id, &next.source)?;
    options.inquiry_digest = next_query.digest().into();
    options.current_policy = Some(signed);
    options.current_policy_digest = Some(submitted.policy_digest.clone());
    options.image_profile_digest = "33".repeat(32);
    f.core
        .engine
        .report_owner_policy_options(&next_query, options)?;
    deliver_agent(&mut f)?;
    let current = f
        .owner
        .owner_policy_options(&scope, &next.source)?
        .context
        .ok_or("cold current context absent")?;
    assert_eq!(current.current_enabled, Some(false));
    assert_eq!(current.current_limit_units.as_deref(), Some("0"));
    assert_eq!(current.current_max_attempt_units.as_deref(), Some("0"));
    assert_eq!(current.current_policy_digest, Some(submitted.policy_digest));
    let successor = f.owner.prepare_owner_policy(
        &scope,
        &next.source,
        &current.options_digest,
        "0",
        "0",
        false,
    )?;
    assert_eq!(successor.allowed_profiles, vec![profile]);
    assert_eq!(successor.version, 2);
    assert_eq!(f.owner.outbox_len(), 0);
    assert!(f.core.engine.state.grants.is_empty());
    assert!(f.core.engine.state.requests.is_empty());
    Ok(())
}

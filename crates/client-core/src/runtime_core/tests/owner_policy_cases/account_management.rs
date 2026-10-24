//! No pending Request, implicit budget or model approval is needed for this read.
use super::*;
use chrono::Datelike;
use types::owner_policy_context::{OwnerPolicyImageSpec, OwnerPolicyOptions};

#[test]
fn account_inquiry_without_image_preserves_explicit_disable_version_and_pending_receipt(
) -> TestResult {
    let (mut f, scope, _) = setup()?;
    assert!(f.owner.state.requests.is_empty());
    assert!(f.core.engine.state.requests.is_empty());
    let status = f.owner.request_account_policy_options(&scope)?;
    assert_eq!(status.state, "pending");
    assert_eq!(
        f.owner
            .find_account_policy_options(&scope)?
            .map(|v| v.source),
        Some(status.source.clone())
    );
    assert!(
        f.owner.find_owner_policy_options(&scope, "", "")?.is_none(),
        "image lookup must not adopt account control"
    );
    deliver_owner(&mut f)?;
    let query = f
        .core
        .engine
        .owner_policy_inquiry(&scope.space_id, &status.source)?;
    let now = crate::engine::now_ms();
    let before = f.core.image()?;
    f.core.owner_policy_account_context(query.signed(), now)?;
    assert!(f
        .core
        .owner_policy_image_context(query.signed(), now)
        .is_err());
    assert_eq!(
        f.core.image()?.as_bytes(),
        before.as_bytes(),
        "read creates no approval/effect"
    );
    let date = chrono::DateTime::from_timestamp_millis(now).ok_or("invalid clock")?;
    let author = f.owner.net.author.clone().ok_or("missing owner")?;
    let prior = OwnerPeriodPolicy {
        owner: f.owner_profile.id.clone(),
        year: date.year(),
        month: u8::try_from(date.month())?,
        version: 7,
        previous_digest: Some("aa".repeat(32)),
        currency: "USD".into(),
        scale: 6,
        limit_units: 9_007_199_254_740_995,
        max_attempt_units: 9_007_199_254_740_993,
        expires_at_ms: now + 60_000,
        enabled: true,
        allowed_profiles: (0..16)
            .map(|value| format!("{value:02x}").repeat(32))
            .collect(),
    };
    let signed = event_log::owner_budget::sign(&author, prior).ok_or("prior signing refused")?;
    let previous = event_log::owner_budget::digest(&signed.policy).ok_or("prior digest")?;
    let options = OwnerPolicyOptions {
        inquiry_digest: query.digest().into(),
        currency: "USD".into(),
        scale: 6,
        reservation_units: 100,
        image_profile_digest: "22".repeat(32),
        observed_at_ms: now,
        year: date.year(),
        month: u8::try_from(date.month())?,
        current_policy: Some(signed),
        current_policy_digest: Some(previous.clone()),
        image_spec: Some(OwnerPolicyImageSpec {
            size: "1024x1024".into(),
            quality: "low".into(),
        }),
    };
    let mut missing = options.clone();
    missing.image_spec = None;
    assert!(f
        .core
        .engine
        .report_owner_policy_options(&query, missing)
        .is_err());
    f.core.engine.report_owner_policy_options(&query, options)?;
    deliver_agent(&mut f)?;
    let context = f
        .owner
        .owner_policy_options(&scope, &status.source)?
        .context
        .ok_or("missing account context")?;
    assert_eq!(
        context.current_limit_units.as_deref(),
        Some("9007199254740995")
    );
    assert_eq!(context.existing_profile_count, 16);
    assert!(
        f.owner
            .prepare_owner_policy(
                &scope,
                &status.source,
                &context.options_digest,
                "9007199254740995",
                "9007199254740993",
                true,
            )
            .is_err(),
        "enabling the new seventeenth profile remains invalid"
    );
    let review = f.owner.prepare_owner_policy(
        &scope,
        &status.source,
        &context.options_digest,
        "9007199254740995",
        "9007199254740993",
        false,
    )?;
    let policy: OwnerPeriodPolicy = serde_json::from_str(&review.canonical_policy_json)?;
    assert_eq!(policy.version, 8);
    assert_eq!(policy.previous_digest, Some(previous));
    assert!(!policy.enabled);
    assert_eq!(
        policy.allowed_profiles,
        (0..16)
            .map(|value| format!("{value:02x}").repeat(32))
            .collect::<Vec<_>>()
    );
    let submitted = f.owner.submit_owner_policy(
        &scope,
        &review.canonical_policy_json,
        &review.confirmation_digest,
    )?;
    assert_eq!(submitted.state, "pending");
    deliver_owner(&mut f)?;
    let control = f
        .core
        .engine
        .owner_policy_submission(&scope.space_id, &submitted.source)?;
    assert!(!control.signed().policy.enabled);
    assert_eq!(
        f.owner
            .owner_policy_status(&scope, &submitted.source)?
            .state,
        "pending",
        "sending disable does not infer Runtime installation"
    );
    f.core
        .engine
        .report_owner_policy(&control, Some(control.digest()))?;
    deliver_agent(&mut f)?;
    assert_eq!(
        f.owner
            .owner_policy_status(&scope, &submitted.source)?
            .state,
        "installed"
    );
    let cold = DeviceCore::restore(f.core.image()?, f.credential())?;
    assert!(cold.engine.state.requests.is_empty());
    Ok(())
}

#[test]
fn account_management_never_substitutes_group_foreign_device_or_stale_context() -> TestResult {
    let (mut f, scope, _) = setup()?;
    let mut other = scope.clone();
    other.expected_device_id = "11".repeat(32);
    assert!(f.owner.request_account_policy_options(&other).is_err());
    assert!(f
        .owner
        .start_owner_policy_direct(
            &scope.expected_identity_id,
            &other.expected_device_id,
            &scope.agent_id
        )
        .is_err());
    let private = f.owner.start_owner_policy_direct(
        &scope.expected_identity_id,
        &scope.expected_device_id,
        &scope.agent_id,
    )?;
    assert_eq!(private, scope.space_id);
    let status = f.owner.request_account_policy_options(&scope)?;
    deliver_owner(&mut f)?;
    let query = f
        .core
        .engine
        .owner_policy_inquiry(&scope.space_id, &status.source)?;
    assert!(f
        .core
        .owner_policy_account_context(
            query.signed(),
            crate::engine::now_ms() + types::owner_policy_context::CONTEXT_TTL_MS + 1
        )
        .is_err());
    f.owner
        .state
        .spaces
        .get_mut(&scope.space_id)
        .ok_or("missing DM")?
        .kind = SpaceKind::Group;
    assert!(f.owner.request_account_policy_options(&scope).is_err());
    assert!(f
        .owner
        .owner_policy_options(&scope, &status.source)
        .is_err());
    f.core
        .engine
        .state
        .spaces
        .get_mut(&scope.space_id)
        .ok_or("missing agent DM")?
        .members
        .retain(|(id, _)| id != &scope.expected_identity_id);
    assert!(f
        .core
        .owner_policy_account_context(query.signed(), crate::engine::now_ms())
        .is_err());
    Ok(())
}

#[test]
fn explicit_account_inquiry_queues_without_clear_fallback_while_private_mls_is_not_ready(
) -> TestResult {
    let (mut f, scope, json) = setup()?;
    f.owner.device()?.forget(&scope.space_id)?;
    assert!(f.owner.ready_epoch(&scope.space_id).is_none());
    assert!(f.owner.review_owner_policy(&scope, &json).is_err());
    let pending = f.owner.request_account_policy_options(&scope)?;
    assert_eq!(pending.state, "pending");
    assert_eq!(f.owner.outbox_len(), 1);
    assert!(
        f.owner.outbox_envelopes().is_empty(),
        "no clear control is ever offered without MLS"
    );
    assert!(
        f.owner
            .owner_policy_options(&scope, &pending.source)
            .is_err(),
        "queued intent does not become readable/effective scope"
    );
    Ok(())
}

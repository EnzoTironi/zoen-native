//! Actual certified accounts + MLS transport; no synthetic install authority.
use super::*;
use crate::{
    owner_policy::{OwnerPolicyScope, OwnerPolicyStatus},
    CoreError,
};
use std::result::Result as StdResult;
use types::owner_budget::OwnerPeriodPolicy;
type TestResult = StdResult<(), Box<dyn std::error::Error>>;
mod account_management;
mod admission;
mod inquiry_observation;
mod inquiry_recovery;
mod publication_cases;
mod retained_membership;

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
    assert_eq!(
        f.owner
            .find_owner_policy_submission(&scope, &pending.policy_digest)?,
        Some(pending.clone())
    );
    assert!(f
        .owner
        .find_owner_policy_submission(&scope, &"cc".repeat(32))?
        .is_none());
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
    // A later authentic negative report cannot erase an observed installation.
    let actor = f
        .core
        .engine
        .net
        .author
        .clone()
        .ok_or("missing agent author")?;
    f.core.engine.append_synced(
        &scope.space_id,
        &actor,
        crate::engine::now_ms(),
        EventBody::OwnerPolicyReported {
            source: pending.source.clone(),
            policy_digest: pending.policy_digest.clone(),
            outcome: types::OwnerPolicyOutcome::Refused,
        },
    )?;
    // This deliberately bypasses the typed producer to model an authentic
    // contradictory report. The owner must retain installation evidence, while
    // the native publisher must refuse its unprepared own echo.
    f.core.engine.mls_seal_outbox()?;
    let envelope = f.core.engine.outbox_envelopes().remove(0);
    let frame = ordered(&f.owner, envelope);
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Applied);
    assert!(matches!(
        f.core.ingest(vec![frame]),
        Err(NativeError::History)
    ));
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

#[test]
fn policy_quote_reads_only_confirmed_image_review_without_act_or_approval() -> TestResult {
    use types::owner_policy_context::OwnerPolicyInquiry;
    let (mut f, request, hash, envelope) = super::image_review_cases::pending();
    let now = crate::engine::now_ms();
    let author = f.owner.net.author.clone().ok_or("missing owner author")?;
    let inquiry = OwnerPolicyInquiry {
        id: "policy_query_nonce".into(),
        owner: f.owner_profile.id.clone(),
        agent: f.agent_profile.id.clone(),
        space: f.space.clone(),
        request: request.clone(),
        content_hash: hash,
        price_digest: image_task_cases::input().definition.price_digest,
        purpose: None,
        at_ms: now,
    };
    let signed =
        event_log::owner_policy_inquiry::sign(&author, inquiry.clone()).ok_or("inquiry refused")?;
    assert!(
        f.core.owner_policy_image_context(&signed, now).is_err(),
        "unconfirmed request is not context"
    );
    let frame = ordered(&f.owner, envelope);
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Applied);
    f.core.ingest(vec![frame])?;
    let before = f.core.image()?;
    assert_eq!(
        f.core.owner_policy_image_context(&signed, now)?,
        image_task_cases::input()
    );
    assert_eq!(
        f.core.image()?.as_bytes(),
        before.as_bytes(),
        "read has no side effects"
    );
    assert!(
        f.core.image_task(&request, now).is_err(),
        "quote read is not ApproveOnce"
    );
    f.encrypted(
        EventBody::GrantRevoked {
            grant: "image-review-act".into(),
        },
        now,
    );
    assert_eq!(
        f.core.owner_policy_image_context(&signed, now)?,
        image_task_cases::input(),
        "read does not require Act"
    );
    for field in ["request", "content_hash", "price_digest"] {
        let mut value = serde_json::to_value(&inquiry)?;
        value[field] = if field == "request" {
            "different_request".into()
        } else {
            "bb".repeat(32).into()
        };
        let changed = serde_json::from_value(value)?;
        assert!(event_log::owner_policy_inquiry::sign(&author, changed)
            .is_none_or(|changed| f.core.owner_policy_image_context(&changed, now).is_err()));
    }
    assert!(f
        .core
        .owner_policy_image_context(
            &signed,
            now + types::owner_policy_context::CONTEXT_TTL_MS + 1
        )
        .is_err());
    Ok(())
}

#[test]
fn authenticated_options_prepare_exact_money_and_successor_of_expired_disabled_policy() -> TestResult
{
    use chrono::Datelike;
    use types::owner_policy_context::OwnerPolicyOptions;
    let (mut f, request, hash, envelope) = super::image_review_cases::pending();
    let frame = ordered(&f.owner, envelope);
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Applied);
    f.core.ingest(vec![frame])?;
    f.owner.set_registered(true)?;
    f.core.engine.set_registered(true)?;
    let account = f.owner.account().ok_or("missing owner")?;
    let scope = OwnerPolicyScope {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: f.space.clone(),
        agent_id: f.agent_profile.id.clone(),
    };
    let query_status = f
        .owner
        .request_owner_policy_options(&scope, &request, &hash)?;
    assert_eq!(query_status.state, "pending");
    assert_eq!(
        f.owner
            .find_owner_policy_options(&scope, &request, &hash)?
            .map(|status| status.source),
        Some(query_status.source.clone())
    );
    assert!(f
        .owner
        .find_owner_policy_options(&scope, &request, &"cc".repeat(32))?
        .is_none());
    assert!(f
        .owner
        .prepare_owner_policy(
            &scope,
            &query_status.source,
            "not-ready",
            "100",
            "100",
            true
        )
        .is_err());
    deliver_owner(&mut f)?;
    let query = f
        .core
        .engine
        .owner_policy_inquiry(&scope.space_id, &query_status.source)?;
    let now = crate::engine::now_ms();
    let date = chrono::DateTime::from_timestamp_millis(now).ok_or("invalid clock")?;
    let options = OwnerPolicyOptions {
        inquiry_digest: query.digest().into(),
        currency: "USD".into(),
        scale: 6,
        reservation_units: 9_007_199_254_740_993,
        image_profile_digest: image_task_cases::input().definition.price_digest,
        observed_at_ms: now,
        year: date.year(),
        month: u8::try_from(date.month())?,
        current_policy: None,
        current_policy_digest: None,
        image_spec: None,
    };
    let mut wrong = options.clone();
    wrong.inquiry_digest = "bb".repeat(32);
    assert!(f
        .core
        .engine
        .report_owner_policy_options(&query, wrong)
        .is_err());
    f.core
        .engine
        .report_owner_policy_options(&query, options.clone())?;
    f.core
        .engine
        .report_owner_policy_options(&query, options.clone())?;
    assert_eq!(f.core.engine.outbox_len(), 1);
    assert!(f
        .owner
        .owner_policy_options(&scope, &query_status.source)?
        .context
        .is_none());
    deliver_agent(&mut f)?;
    let context = f
        .owner
        .owner_policy_options(&scope, &query_status.source)?
        .context
        .ok_or("no options")?;
    assert_eq!(
        context.reservation_units, "9007199254740993",
        "bridge must not round money beyond JS safe integers"
    );
    assert!(context.current_limit_units.is_none(), "no implicit budget");
    assert!(f
        .owner
        .prepare_owner_policy(
            &scope,
            &query_status.source,
            &context.options_digest,
            "9007199254740992",
            "1",
            true
        )
        .is_err());
    let review = f.owner.prepare_owner_policy(
        &scope,
        &query_status.source,
        &context.options_digest,
        "9007199254740995",
        "9007199254740993",
        true,
    )?;
    assert_eq!(review.limit_units, "9007199254740995");
    let first: OwnerPeriodPolicy = serde_json::from_str(&review.canonical_policy_json)?;
    assert_eq!(first.version, 1);
    assert!(first.previous_digest.is_none());
    assert_eq!(
        first.allowed_profiles,
        vec![options.image_profile_digest.clone()]
    );
    assert!(f
        .owner
        .prepare_owner_policy(
            &scope,
            &query_status.source,
            "bb",
            "9007199254740995",
            "9007199254740993",
            true
        )
        .is_err());

    let second_status = f
        .owner
        .request_owner_policy_options(&scope, &request, &hash)?;
    deliver_owner(&mut f)?;
    let second = f
        .core
        .engine
        .owner_policy_inquiry(&scope.space_id, &second_status.source)?;
    let author = f.owner.net.author.clone().ok_or("missing owner author")?;
    let prior = OwnerPeriodPolicy {
        version: 7,
        previous_digest: Some("ab".repeat(32)),
        expires_at_ms: now - 1,
        enabled: false,
        ..first
    };
    let signed = event_log::owner_budget::sign(&author, prior).ok_or("policy signature refused")?;
    let latest_digest =
        event_log::owner_budget::digest(&signed.policy).ok_or("policy digest refused")?;
    let options = OwnerPolicyOptions {
        inquiry_digest: second.digest().into(),
        current_policy: Some(signed),
        current_policy_digest: Some(latest_digest.clone()),
        ..options
    };
    f.core
        .engine
        .report_owner_policy_options(&second, options)?;
    deliver_agent(&mut f)?;
    let context = f
        .owner
        .owner_policy_options(&scope, &second_status.source)?
        .context
        .ok_or("no latest policy")?;
    assert_eq!(context.current_enabled, Some(false));
    assert_eq!(context.current_expired, Some(true));
    let review = f.owner.prepare_owner_policy(
        &scope,
        &second_status.source,
        &context.options_digest,
        "9007199254740995",
        "9007199254740993",
        true,
    )?;
    let successor: OwnerPeriodPolicy = serde_json::from_str(&review.canonical_policy_json)?;
    assert_eq!(
        successor.version, 8,
        "expired/disabled existing rows must not reset to v1"
    );
    assert_eq!(successor.previous_digest, Some(latest_digest));
    Ok(())
}

//! Actual signed private control originals; clock and storage faults grant no authority.
use super::publication_cases::provider_rows;
use super::*;
use types::owner_policy_context::{OwnerPolicyInquiry, OwnerPolicyInquiryPurpose, CONTEXT_TTL_MS};

fn queue_at(
    f: &mut Fixture,
    scope: &OwnerPolicyScope,
    at_ms: i64,
) -> std::result::Result<
    StdResult<crate::owner_policy::OwnerPolicyOptionsStatus, CoreError>,
    Box<dyn std::error::Error>,
> {
    Ok(f.owner.queue_owner_policy_inquiry(
        scope,
        OwnerPolicyInquiry {
            id: types::new_id("account_inquiry")?,
            owner: scope.expected_identity_id.clone(),
            agent: scope.agent_id.clone(),
            space: scope.space_id.clone(),
            request: String::new(),
            content_hash: String::new(),
            price_digest: String::new(),
            purpose: Some(OwnerPolicyInquiryPurpose::AccountBudget),
            at_ms,
        },
    ))
}
fn cold_owner(f: &mut Fixture) -> TestResult {
    let actor = f.owner.net.author.clone().ok_or("missing owner")?;
    let bytes = f.owner.store.memory_image()?;
    f.owner = Engine::from_store(store::Store::from_memory_image(&bytes)?, ":memory:")?;
    f.owner.net.author = Some(actor);
    Ok(())
}
fn head(f: &Fixture) -> StdResult<(String, String), Box<dyn std::error::Error>> {
    Ok(f.owner.store.conn().query_row(
        "SELECT key,value FROM meta WHERE key GLOB 'runtime.owner-policy.inquiry/1/*' ORDER BY length(key) LIMIT 1",
        [], |row| Ok((row.get(0)?, row.get(1)?)),
    )?)
}

#[test]
fn known_account_inquiry_expiry_is_terminal_only_after_the_exact_boundary_and_current_scope(
) -> TestResult {
    let (mut f, scope, _) = setup()?;
    let at = crate::engine::now_ms();
    let status = queue_at(&mut f, &scope, at)?;
    let before = f.owner.outbox_len();
    assert_eq!(
        f.owner
            .owner_policy_options_at(&scope, &status.source, at + CONTEXT_TTL_MS)?
            .state,
        "pending"
    );
    let expired =
        f.owner
            .owner_policy_options_at(&scope, &status.source, at + CONTEXT_TTL_MS + 1)?;
    assert_eq!(expired.state, "expired");
    assert!(expired.context.is_none());
    assert_eq!(
        f.owner.outbox_len(),
        before,
        "expiry neither signs nor retries"
    );
    let mut foreign = scope.clone();
    foreign.expected_device_id = "aa".repeat(32);
    assert!(f
        .owner
        .owner_policy_options_at(&foreign, &status.source, at + CONTEXT_TTL_MS + 1)
        .is_err());
    cold_owner(&mut f)?;
    assert_eq!(
        f.owner
            .owner_policy_options_at(&scope, &status.source, at + CONTEXT_TTL_MS + 1)?
            .state,
        "expired"
    );
    deliver_owner(&mut f)?;
    let original = f
        .core
        .engine
        .owner_policy_inquiry(&scope.space_id, &status.source)?;
    assert!(
        f.core
            .owner_policy_account_context(original.signed(), at + CONTEXT_TTL_MS + 1)
            .is_err(),
        "expired inquiry never admits Runtime context"
    );
    assert!(f
        .owner
        .prepare_owner_policy(&scope, &status.source, &"00".repeat(32), "100", "10", true)
        .is_err());
    assert!(f
        .owner
        .owner_policy_options_at(&scope, "unknown_source", at + CONTEXT_TTL_MS + 1)
        .is_err());
    Ok(())
}

#[test]
fn inquiry_ack_recovery_uses_atomic_producer_order_after_clock_rollback_ties_and_cold_reopen(
) -> TestResult {
    for delta in [-1, 0] {
        let (mut f, scope, _) = setup()?;
        let at = crate::engine::now_ms();
        let first = queue_at(&mut f, &scope, at)?;
        deliver_owner(&mut f)?;
        let second = queue_at(&mut f, &scope, at + delta)?;
        assert_ne!(first.source, second.source);
        assert_eq!(f.owner.outbox_len(), 1);
        assert_eq!(
            f.owner.store.conn().query_row(
                "SELECT count(*) FROM meta WHERE key GLOB 'runtime.owner-policy.inquiry/1/*'",
                [],
                |row| row.get::<_, i64>(0),
            )?,
            3,
            "only marker, head and current producer are retained"
        );
        f.owner.mls_seal_outbox()?;
        let exact = f.owner.outbox_envelopes();
        for cold in [false, true] {
            if cold {
                cold_owner(&mut f)?;
            }
            let recovered = f
                .owner
                .find_account_policy_options(&scope)?
                .ok_or("missing original")?;
            assert_eq!(
                recovered, second,
                "ACK loss retains B, not older accepted A"
            );
            assert_eq!(
                f.owner.outbox_envelopes(),
                exact,
                "lookup cannot re-sign/reseal"
            );
            assert_eq!(f.owner.outbox_len(), 1);
        }
        deliver_owner(&mut f)?;
        cold_owner(&mut f)?;
        assert_eq!(
            f.owner
                .find_account_policy_options(&scope)?
                .map(|v| v.source),
            Some(second.source)
        );
        assert_eq!(f.owner.outbox_len(), 0);
    }
    Ok(())
}

#[test]
fn missing_corrupt_or_rolled_back_inquiry_producer_records_never_select_another_original(
) -> TestResult {
    for fault in [
        "head_missing",
        "marker_missing",
        "extra_record",
        "oversized",
        "record_missing",
        "head_rollback",
        "raw",
        "order",
        "source",
        "sealed",
    ] {
        let (mut f, scope, _) = setup()?;
        let first = f.owner.request_account_policy_options(&scope)?;
        let (_, older) = head(&f)?;
        deliver_owner(&mut f)?;
        let second = f.owner.request_account_policy_options(&scope)?;
        f.owner.mls_seal_outbox()?;
        let (key, raw) = head(&f)?;
        match fault {
            "extra_record" => f.owner.store.set_meta(&format!("{key}/extra"), &raw)?,
            "oversized" => f.owner.store.set_meta(&key, &"x".repeat(16_385))?,
            "marker_missing" => f.owner.store.meta_delete(&format!("{key}:initialized"))?,
            "head_missing" => {
                f.owner
                    .store
                    .conn()
                    .execute("DELETE FROM meta WHERE key=?1", [&key])?;
            }
            "record_missing" => {
                f.owner.store.conn().execute(
                    "DELETE FROM meta WHERE key=?1",
                    [format!("{key}/{}", second.source)],
                )?;
            }
            "head_rollback" => f.owner.store.set_meta(&key, &older)?,
            "raw" => f.owner.store.set_meta(&key, "{}")?,
            "sealed" => f
                .owner
                .store
                .set_meta(&format!("mls.sealed:{}", second.source), "{}")?,
            "order" | "source" => {
                let mut value: serde_json::Value = serde_json::from_str(&raw)?;
                if fault == "order" {
                    value["order"] = 1.into();
                } else {
                    value["event"]["client_id"] = first.source.clone().into();
                }
                let altered = serde_json::to_string(&value)?;
                f.owner.store.set_meta(&key, &altered)?;
            }
            _ => return Err("unknown fault".into()),
        }
        cold_owner(&mut f)?;
        assert!(
            f.owner.find_account_policy_options(&scope).is_err(),
            "{fault}"
        );
        assert_eq!(f.owner.outbox_len(), 1, "{fault} never mints a new inquiry");
    }
    Ok(())
}

#[test]
fn inquiry_head_write_failure_rolls_back_the_new_outbox_and_producer_association_together(
) -> TestResult {
    let (mut f, scope, _) = setup()?;
    let first = f.owner.request_account_policy_options(&scope)?;
    deliver_owner(&mut f)?;
    let (key, original) = head(&f)?;
    let before = provider_rows(&f.owner)?;
    f.owner.store.conn().execute_batch(&format!("CREATE TEMP TRIGGER reject_inquiry_head BEFORE UPDATE ON meta WHEN OLD.key='{}' BEGIN SELECT RAISE(ABORT,'injected inquiry head failure'); END;", key.replace('\'', "''")))?;
    assert!(f.owner.request_account_policy_options(&scope).is_err());
    assert_eq!(f.owner.outbox_len(), 0);
    assert!(f.owner.net.pending.is_empty());
    assert_eq!(head(&f)?, (key, original));
    assert_eq!(provider_rows(&f.owner)?, before);
    assert!(f.owner.store.conn().is_autocommit());
    f.owner
        .store
        .conn()
        .execute_batch("DROP TRIGGER reject_inquiry_head")?;
    assert_eq!(
        f.owner
            .find_account_policy_options(&scope)?
            .map(|v| v.source),
        Some(first.source)
    );
    let next = f.owner.request_account_policy_options(&scope)?;
    assert_eq!(
        f.owner
            .find_account_policy_options(&scope)?
            .map(|v| v.source),
        Some(next.source)
    );
    Ok(())
}

#[test]
fn inquiry_counter_overflow_fails_atomically_without_another_original() -> TestResult {
    let (mut f, scope, _) = setup()?;
    let first = f.owner.request_account_policy_options(&scope)?;
    let (key, raw) = head(&f)?;
    // Canonical serialization must preserve the actual Original field order.
    let raw = raw.replace("\"order\":1", &format!("\"order\":{}", u64::MAX));
    f.owner.store.set_meta(&key, &raw)?;
    f.owner
        .store
        .set_meta(&format!("{key}/{}", first.source), &raw)?;
    let before = f.owner.outbox_len();
    assert!(f.owner.request_account_policy_options(&scope).is_err());
    assert_eq!(f.owner.outbox_len(), before);
    assert_eq!(head(&f)?, (key, raw));
    Ok(())
}

#[test]
fn image_inquiry_ack_recovery_also_retains_latest_producer_after_clock_rollback() -> TestResult {
    let (mut f, request, hash, envelope) = super::super::image_review_cases::pending();
    let frame = ordered(&f.owner, envelope);
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Applied);
    f.core.ingest(vec![frame])?;
    f.owner.set_registered(true)?;
    f.core.engine.set_registered(true)?;
    let account = f.owner.account().ok_or("missing account")?;
    let scope = OwnerPolicyScope {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        agent_id: f.agent_profile.id.clone(),
        space_id: f.space.clone(),
    };
    let at = crate::engine::now_ms();
    let inquiry = |at_ms| OwnerPolicyInquiry {
        id: types::new_id("image_inquiry").unwrap(),
        owner: scope.expected_identity_id.clone(),
        agent: scope.agent_id.clone(),
        space: scope.space_id.clone(),
        request: request.clone(),
        content_hash: hash.clone(),
        price_digest: image_task_cases::input().definition.price_digest,
        purpose: None,
        at_ms,
    };
    let first = f.owner.queue_owner_policy_inquiry(&scope, inquiry(at))?;
    deliver_owner(&mut f)?;
    let second = f
        .owner
        .queue_owner_policy_inquiry(&scope, inquiry(at - 1))?;
    assert_ne!(first.source, second.source);
    cold_owner(&mut f)?;
    assert_eq!(
        f.owner
            .find_owner_policy_options(&scope, &request, &hash)?
            .map(|value| value.source),
        Some(second.source)
    );
    assert!(f.owner.find_account_policy_options(&scope)?.is_none());
    assert_eq!(f.owner.outbox_len(), 1);
    Ok(())
}

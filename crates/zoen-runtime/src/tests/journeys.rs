use super::*;
pub(super) async fn run() {
    stores::native_device_custody().await;
    println!("native custody journey: test-provisioned real directory, retained OpenMLS packages, cold restore and device revocation PASS");
    failure_cuts::other_namespace_cannot_refund_dispatched_hold().await;
    println!("authority journey: another FDB namespace cannot refund an in-flight paid hold PASS");
    deployment::matching_workers_and_immutable_pairing().await;
    println!(
        "authority journey: matching workers reopen, immutable pairing refuses changed keys PASS"
    );
    deployment::changed_marker_closes_open_and_execution().await;
    println!(
        "authority journey: missing or changed FDB pairing refuses open, dispatch and closure PASS"
    );
    deployment::orphaned_marker_is_not_adopted().await;
    println!("authority journey: orphaned FDB pairing is not silently adopted by SQL PASS");
    deployment::unbound_attempts_cannot_be_reassigned().await;
    println!(
        "authority journey: unbound retained SQL attempts cannot be assigned a new FDB store PASS"
    );
    deployment::failed_initial_sql_commit_keeps_orphan_closed().await;
    println!("authority journey: failed initial SQL commit leaves FDB pairing orphan closed PASS");
    let w = World::new().await;
    let step = w.step(0).await;
    assert_eq!(
        w.runtime
            .run_model(&step.request.context.run_id)
            .await
            .unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    let (_, state) = w.runtime.complete_verified(&step).await.unwrap();
    assert_eq!(state, FinancialState::Settled { units: 6 });
    assert!(w.runtime.complete_verified(&step).await.is_err());
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        w.balance().await,
        PeriodBalance {
            held_units: 0,
            spent_units: 6
        }
    );
    w.runtime.reconcile(64).await.unwrap();
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_dispositions").await,
        1
    );
    w.finish().await;
    println!("authority journey: real HTTP settlement and durable replay PASS");
    financial::policies_and_historical_owner().await;
    println!("authority journey: signed policy and historical owner sponsorship PASS");
    financial::configured_gateway_must_match_price_profile().await;
    println!("authority journey: gateway model, credential and endpoint match approved price PASS");
    financial::owner_wide_concurrency().await;
    println!("authority journey: 100 concurrent reservations share owner ceiling PASS");
    financial::bounded_reservation_contention_recovers_once().await;
    println!("authority journey: bounded pool and row-lock contention recover one exact reservation PASS");
    financial::reservation_retry_rechecks_revoked_device().await;
    println!(
        "authority journey: reservation retry rechecks revoked device, zero hold and send PASS"
    );
    financial::immutable_balanced_postings().await;
    println!("authority journey: immutable exact balanced postings and aggregate audit PASS");
    financial::changed_and_lowered_policy().await;
    println!("authority journey: lower policy retains obligations, no price or FX fallback PASS");
    financial::overrun_and_missing_usage().await;
    println!("authority journey: direct zero versus missing usage and full supported overrun PASS");
    financial::malformed_output_still_settles().await;
    println!("authority journey: invalid output settles valid receipt, bounded capture PASS");
    failure_cuts::reservation_ack_lost_reuses_one_hold().await;
    println!("authority journey: suppressed reservation ACK recovers one hold and one send PASS");
    failure_cuts::claim_ack_lost_never_recreates_claim().await;
    println!("authority journey: suppressed claim ACK cannot resend, permanent closure releases once PASS");
    failure_cuts::admission_and_guard_reply_losses_keep_hold().await;
    println!("authority journey: suppressed admission or guard ACK sends zero, keeps hold PASS");
    failure_cuts::expired_policy_cannot_finish_admission().await;
    println!("authority journey: signed policy expires during admission, zero send and hold retained PASS");
    failure_cuts::delayed_permit_use_cannot_send().await;
    println!(
        "authority journey: delayed permit consumption sends zero and retains admitted hold PASS"
    );
    failure_cuts::admission_tombstone_race_and_old_worker().await;
    println!("authority journey: 12 real admission/tombstone races exclude unsafe release PASS");
    failure_cuts::both_admission_closure_orderings().await;
    println!("authority journey: both forced admission/tombstone orderings protect the hold PASS");
    failure_cuts::revocation_does_not_hold_http_lock().await;
    println!("authority journey: revocation commits while HTTP paused, late evidence settles PASS");
    failure_cuts::takeover_retains_late_bill_but_denies_progress().await;
    println!("authority journey: device takeover denies stale writes, late evidence settles PASS");
    failure_cuts::cancellation_and_lost_http_never_repeat().await;
    println!("authority journey: dropped future or lost reply never repeats paid send PASS");
    stores::closed_sql_pool_retains_fdb_evidence().await;
    println!(
        "authority journey: unavailable SQL pool retains FDB evidence and reconciles once PASS"
    );
    stores::settlement_ack_loss_and_outbox_ack_selection().await;
    println!(
        "authority journey: suppressed settlement ACK and missing outbox ACKs recover once PASS"
    );
    stores::declared_restore_closes_dispatch_and_release().await;
    println!("authority journey: declared restore forbids dispatch and absence-based release PASS");
    stores::sealed_store_canaries().await;
    println!(
        "authority journey: sealed SQL/FDB evidence contains no prompt, key or reply canaries PASS"
    );
}

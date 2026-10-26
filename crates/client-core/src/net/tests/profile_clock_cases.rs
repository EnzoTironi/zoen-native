use super::*;

pub(super) fn an_unrefreshed_receipt_clock_expires_without_advancing_its_timestamp() {
    let clock = protocol::KeyPackageClaimClock::from_server_ms(1_800_000_000_000).unwrap();
    let mut sample = super::ClaimClockSample::new(clock);
    sample.sampled -=
        std::time::Duration::from_millis(protocol::KEY_PACKAGE_CLAIM_TTL_MS as u64 / 2);
    assert_eq!(sample.fresh(), Some(clock));
    sample.sampled -=
        std::time::Duration::from_millis(protocol::KEY_PACKAGE_CLAIM_TTL_MS as u64 / 2 + 1_000);
    assert_eq!(sample.fresh(), None);
}

pub(super) fn agent_definition_configured_http_name_keeps_case_and_default_port_binding() {
    for raw in [
        "https://RELAY.example:443/",
        "wss://RELAY.example:443/",
        "ws://LOCALHOST:80/",
        "relay.example:443/",
    ] {
        let (cmd, _commands) = mpsc::unbounded_channel();
        let (status_tx, status) = watch::channel(NetStatus {
            state: ConnState::Online,
            synced: true,
            registered: true,
            error: None,
            authenticated_relay: Some(AuthenticatedRelay {
                base: http_base(raw),
                name: "synthetic-qualified-relay".into(),
                identity: "owner".into(),
                device: "device".into(),
            }),
        });
        let net = Net {
            rt: None,
            cmd,
            status,
            engine: EngineLease::new(Arc::new(Mutex::new(Engine::open(":memory:").unwrap()))),
        };
        assert_eq!(
            net.authenticated_relay_name(raw, "owner", "device")
                .as_deref(),
            Some("synthetic-qualified-relay")
        );
        assert!(net
            .authenticated_relay_name(raw, "stranger", "device")
            .is_none());
        assert!(net
            .authenticated_relay_name(raw, "owner", "foreign-device")
            .is_none());
        assert!(net
            .authenticated_relay_name("https://different.example", "owner", "device")
            .is_none());
        assert!(net
            .authenticated_synced_relay_name(raw, "owner", "device")
            .is_some());
        status_tx.send_modify(|status| status.synced = false);
        assert!(net
            .authenticated_relay_name(raw, "owner", "device")
            .is_some());
        assert!(
            net.authenticated_synced_relay_name(raw, "owner", "device")
                .is_none(),
            "a newly Ready socket is authenticated but has not caught up"
        );
        status_tx.send_modify(|status| {
            status.synced = true;
            status.authenticated_relay = None;
        });
        assert!(
            net.authenticated_synced_relay_name(raw, "owner", "device")
                .is_none(),
            "catch-up without an authenticated owner is insufficient"
        );
    }
}

pub(super) fn recovery_download_retries_follow_current_references_and_discard_obsolete_failures() {
    let mut retries = RecoveryRetries::default();
    let now = tokio::time::Instant::now();
    let mut reference = crate::mls_recovery::ContextRef {
        space: "changing-context".into(),
        epoch: 0,
        blob: "0".repeat(64),
        device: "device".into(),
    };
    for epoch in 0..10_000 {
        reference.epoch = epoch;
        reference.blob = format!("{epoch:064x}");
        assert_eq!(
            retries.next(vec![reference.clone()], now),
            Some(reference.clone())
        );
        retries.defer(reference.clone(), now);
        assert_eq!(retries.next(vec![reference.clone()], now), None);
        assert_eq!(retries.by_space.len(), 1);
    }
    assert_eq!(
        retries.next(vec![reference.clone()], now + Duration::from_secs(5)),
        Some(reference.clone())
    );
    let other = crate::mls_recovery::ContextRef {
        space: "another-space".into(),
        ..reference.clone()
    };
    retries.defer(other.clone(), now);
    assert_eq!(retries.next(vec![other.clone()], now), None);
    assert_eq!(retries.by_space.len(), 1, "obsolete Space was removed");
    retries.complete(&reference);
    assert_eq!(
        retries.by_space.len(),
        1,
        "a stale completion cannot clear another Space's backoff"
    );
    retries.complete(&other);
    assert!(retries.by_space.is_empty());
    retries.defer(other, now);
    assert!(retries.next(Vec::new(), now).is_none());
    assert!(
        retries.by_space.is_empty(),
        "cached, removed or restored groups need no retry state"
    );
}

pub(super) fn catch_up_retry_keeps_handshake_refusals_terminal_and_rate_hints_bounded() {
    for code in [
        ErrorCode::Unauthorized,
        ErrorCode::UpgradeRequired,
        ErrorCode::Other,
    ] {
        let mut backoff = Duration::from_secs(4);
        assert!(matches!(
            refused(code, "refused".into(), &mut backoff),
            Exit::Blocked(message) if message == "refused"
        ));
        assert_eq!(backoff, Duration::from_secs(4));
    }
    let mut backoff = Duration::from_secs(4);
    assert!(matches!(
        refused(ErrorCode::Unavailable, "unavailable".into(), &mut backoff),
        Exit::Retry(_)
    ));
    assert_eq!(backoff, Duration::from_secs(4));
    assert!(matches!(
        refused(
            ErrorCode::RateLimited,
            protocol::slow_down(Duration::from_secs(9)),
            &mut backoff
        ),
        Exit::Retry(_)
    ));
    assert_eq!(backoff, Duration::from_secs(9));
}

pub(super) fn superseded_profile_query_cannot_consume_current_batch_or_its_backoff() {
    let mut waiting = HashMap::new();
    remember_waiting(
        &mut waiting,
        1,
        Waiting::SealedProfiles(vec!["self".into()]),
    );
    remember_waiting(
        &mut waiting,
        2,
        Waiting::AgreementKeys(vec!["contact".into()]),
    );
    remember_waiting(
        &mut waiting,
        3,
        Waiting::SealedProfiles(vec!["self".into()]),
    );
    assert!(
        waiting.remove(&1).is_none(),
        "late Q1 must never reach sealed_profiles_arrived"
    );
    assert!(
        matches!(waiting.get(&3),Some(Waiting::SealedProfiles(ids)) if ids==&vec!["self".to_owned()])
    );
    assert!(
        waiting.contains_key(&2),
        "independent agreement request remains tracked"
    );
    remember_waiting(&mut waiting, 4, Waiting::ProfileUploaded(2));
    remember_waiting(&mut waiting, 5, Waiting::ProfileUploaded(3));
    assert!(
        waiting.remove(&4).is_none(),
        "old upload ACK must not settle current request deadline"
    );
    assert!(matches!(waiting.get(&5), Some(Waiting::ProfileUploaded(3))));
}

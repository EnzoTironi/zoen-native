use super::*;
#[cfg(unix)]
mod docker;
const SECRET: &str = "0123456789abcdef0123456789abcdef";
fn service() -> Service {
    Service::configured(
        Some(
            "turns:turn.example.test:5349?transport=tcp,turns:alternate.example.test:443?transport=tcp"
                .into(),
        ),
        Some(SECRET.into()),
    )
    .unwrap()
}

#[test]
fn credentials_use_standard_rest_mac_and_hide_account_and_device() {
    let service = service();
    let credentials = service
        .issue("identity", "device", 1_700_000_000_000)
        .unwrap();
    assert!(credentials.valid());
    assert_eq!(credentials.expires_at_ms, 1_700_000_100_000);
    // Independent Python stdlib HMAC/SHA256 + HMAC/SHA1/base64 fixture.
    assert_eq!(
        credentials.username,
        "1700000100:a5cbeda576cf84e65c97a40e150a13bb1e6f4d39a4dccc870b7c820c8a0cf919"
    );
    assert_eq!(credentials.credential, "Efv+R/4mQc78cjicq/JY76iirvQ=");
    let password = STANDARD.decode(&credentials.credential).unwrap();
    hmac::verify(
        &hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, SECRET.as_bytes()),
        credentials.username.as_bytes(),
        &password,
    )
    .unwrap();
    assert!(!credentials.username.contains("identity") && !credentials.username.contains("device"));
    let debug = format!("{credentials:?}");
    assert!(!debug.contains(&credentials.username) && !debug.contains(&credentials.credential));
}

#[test]
fn same_device_window_reuses_allocation_identity_without_extending_ttl() {
    let service = service();
    let a = service
        .issue("identity", "device", 1_700_000_000_000)
        .unwrap();
    let b = service
        .issue("identity", "device", 1_700_000_019_999)
        .unwrap();
    assert_eq!(a, b);
    assert_ne!(
        a.username,
        service
            .issue("identity", "other-device", 1_700_000_000_000)
            .unwrap()
            .username
    );
    assert_ne!(
        a.username,
        service
            .issue("other-identity", "device", 1_700_000_000_000)
            .unwrap()
            .username
    );
    for now in [1_700_000_000_000, 1_700_000_059_999, 1_700_000_060_000] {
        let c = service.issue("identity", "device", now).unwrap();
        assert!(c.expires_at_ms > now && c.expires_at_ms <= now + 120_000);
    }
    assert!(service.issue("identity", "device", -1).is_err());
    assert!(service.issue("identity", "device", i64::MAX).is_err());
}

#[test]
fn absent_partial_invalid_or_cleartext_only_configuration_fails_closed() {
    let disabled = Service::disabled();
    assert!(disabled
        .issue("identity", "device", 1_700_000_000_000)
        .is_err());
    assert!(disabled.check_quota("identity", "device").is_err());
    assert!(Service::configured(None, None).is_ok());
    for (urls, secret) in [
        (None, Some(SECRET)),
        (Some("turns:turn.example.test"), None),
        (Some("turn:turn.example.test"), Some(SECRET)),
        (Some("turns:turn.example.test"), Some("short")),
        (Some("turns:user:password@turn.example.test"), Some(SECRET)),
    ] {
        assert!(Service::configured(urls.map(Into::into), secret.map(Into::into)).is_err());
    }
}

#[test]
fn device_and_account_buckets_survive_requests_and_cannot_be_bypassed_by_new_usernames() {
    let service = service();
    for _ in 0..4 {
        service.check_quota("account", "device").unwrap();
    }
    assert!(service.check_quota("account", "device").is_err());
    for n in 0..4 {
        service
            .check_quota("account", &format!("sibling-{n}"))
            .unwrap();
    }
    assert!(service.check_quota("account", "extra-device").is_err());
    service
        .check_quota("other-account", "other-device")
        .unwrap();
}

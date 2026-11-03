use super::*;
use crate::push::tests::{apns_cfg, Mock};

fn target(platform: &str) -> Target {
    Target {
        registration_id: 0,
        platform: platform.into(),
        token: "ab".repeat(32),
    }
}

fn fresh_wake() -> protocol::push::CallWake {
    let now = now() as i64 * 1000;
    protocol::push::CallWake::new("a0".repeat(16), now + 30_000, now).unwrap()
}

#[test]
fn registration_derives_purpose_and_redacts_transport_tokens() {
    for platform in [APNS, APNS_SANDBOX, FCM] {
        assert_eq!(purpose(platform), Some("notification"));
        assert!(valid_registration(platform, ""));
    }
    for platform in [APNS_VOIP, APNS_VOIP_SANDBOX] {
        assert_eq!(purpose(platform), Some("voip"));
        assert!(valid_registration(platform, ""));
        assert!(valid_registration(platform, &"AB".repeat(32)));
        for token in ["abc", "a:b", "token/../x", &"ab".repeat(257)] {
            assert!(!valid_registration(platform, token));
        }
    }
    assert_eq!(purpose("voip"), None);
    assert!(!valid_registration("voip", "aabb"));
    let target = target(APNS_VOIP);
    assert!(!format!("{target:?}").contains(&target.token));
}

#[tokio::test]
async fn voip_and_message_wakes_use_separate_routes_topics_and_payloads() {
    let mock = Arc::new(Mock::default());
    let pusher = Pusher::new(mock.clone(), Some(apns_cfg()), None).unwrap();
    let wake = fresh_wake();
    for platform in [APNS_VOIP, APNS_VOIP_SANDBOX] {
        assert_eq!(pusher.wake(&target(platform)).await, Outcome::Disabled);
        assert_eq!(
            pusher.call_wake(&target(platform), &wake).await,
            Outcome::Sent
        );
    }
    for platform in [APNS, APNS_SANDBOX, FCM] {
        assert!(matches!(
            pusher.call_wake(&target(platform), &wake).await,
            Outcome::Failed(_)
        ));
    }
    let calls = mock.calls.lock().unwrap();
    assert_eq!(
        calls.len(),
        2,
        "ordinary event wake must never deliver VoIP"
    );
    for (index, (url, headers, body)) in calls.iter().enumerate() {
        let header = |name: &str| {
            headers
                .iter()
                .find(|(key, _)| key == name)
                .unwrap()
                .1
                .as_str()
        };
        assert!(url.starts_with(if index == 0 {
            "https://api.push.apple.com/"
        } else {
            "https://api.sandbox.push.apple.com/"
        }));
        assert_eq!(header("apns-topic"), "xyz.tironi.zoen.voip");
        assert_eq!(header("apns-push-type"), "voip");
        assert_eq!(header("apns-priority"), "10");
        assert_eq!(
            header("apns-expiration"),
            (wake.expires_at_ms() / 1000).to_string()
        );
        assert_eq!(
            serde_json::from_str::<Value>(body).unwrap(),
            json!({
                "aps": { "content-available": 1 },
                "zoen": { "v": 1, "call_wake": "a0".repeat(16), "expires_at_ms": wake.expires_at_ms().to_string() }
            })
        );
        assert!(body.len() < 512);
    }
}

#[tokio::test]
async fn expired_or_invalid_targets_never_reach_provider_and_dead_token_is_explicit() {
    let mock = Arc::new(Mock::default());
    let pusher = Pusher::new(mock.clone(), Some(apns_cfg()), None).unwrap();
    let expired = protocol::push::CallWake::new("a0".repeat(16), 2000, 1000).unwrap();
    assert!(matches!(
        pusher.call_wake(&target(APNS_VOIP), &expired).await,
        Outcome::Failed(_)
    ));
    for token in ["", "abc", "../../private", &"ab".repeat(257)] {
        let invalid = Target {
            registration_id: 0,
            platform: APNS_VOIP.into(),
            token: token.into(),
        };
        assert!(matches!(
            pusher.call_wake(&invalid, &fresh_wake()).await,
            Outcome::Failed(_)
        ));
    }
    assert!(mock.calls.lock().unwrap().is_empty());
    mock.replies
        .lock()
        .unwrap()
        .push((410, "private-provider-text".into()));
    assert_eq!(
        pusher.call_wake(&target(APNS_VOIP), &fresh_wake()).await,
        Outcome::Gone
    );
    mock.replies
        .lock()
        .unwrap()
        .push((400, r#"{"reason":"BadDeviceToken"}"#.into()));
    assert_eq!(
        pusher.call_wake(&target(APNS_VOIP), &fresh_wake()).await,
        Outcome::Failed("apns voip 400".into())
    );
    assert!(pusher.voip_available());
    let disabled = Pusher::new(mock, None, None).unwrap();
    assert!(!disabled.voip_available());
    assert_eq!(
        disabled.call_wake(&target(APNS_VOIP), &fresh_wake()).await,
        Outcome::Disabled
    );
}

#[tokio::test]
async fn wake_requires_an_apns_deadline_in_a_future_second() -> anyhow::Result<()> {
    let http = Arc::new(Mock::default());
    let pusher = Pusher::new(http.clone(), Some(apns_cfg()), None)?;
    let wake = protocol::push::CallWake::new("ab".repeat(16), 1_999, 1_000)?;
    assert_eq!(
        pusher.call_wake_at(&target(APNS_VOIP), &wake, 1_500).await,
        Outcome::Failed("VoIP wake too close to expiry".into())
    );
    assert!(http.calls.lock().unwrap().is_empty());
    let wake = protocol::push::CallWake::new("ab".repeat(16), 2_000, 1_000)?;
    assert_eq!(
        pusher.call_wake_at(&target(APNS_VOIP), &wake, 1_500).await,
        Outcome::Sent
    );
    let calls = http.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert!(calls[0]
        .1
        .iter()
        .any(|(key, value)| key == "apns-expiration" && value == "2"));
    Ok(())
}

#[tokio::test]
async fn saturated_voip_wakes_leave_message_wake_capacity_available() -> anyhow::Result<()> {
    let http = Arc::new(Mock::default());
    let pusher = Pusher::new(http.clone(), Some(apns_cfg()), None)?;
    let permits = pusher
        .voip_slots
        .try_acquire_many(u32::try_from(pusher.voip_slots.available_permits())?)?;
    assert!(matches!(
        pusher.call_wake(&target(APNS_VOIP), &fresh_wake()).await,
        Outcome::Failed(_)
    ));
    assert!(http.calls.lock().unwrap().is_empty());
    assert!(pusher.try_wake_task().is_some());
    drop(permits);
    assert_eq!(
        pusher.call_wake(&target(APNS_VOIP), &fresh_wake()).await,
        Outcome::Sent
    );
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn stalled_provider_request_ends_at_apns_expiration() -> anyhow::Result<()> {
    struct Stalled;

    #[async_trait::async_trait]
    impl Http for Stalled {
        async fn post(
            &self,
            _url: &str,
            _headers: &[(&str, String)],
            _body: String,
        ) -> anyhow::Result<(u16, String)> {
            std::future::pending().await
        }
    }

    let pusher = Pusher::new(Arc::new(Stalled), Some(apns_cfg()), None)?;
    let wake = protocol::push::CallWake::new("ab".repeat(16), 2_900, 1_000)?;
    let start = tokio::time::Instant::now();
    assert_eq!(
        pusher.call_wake_at(&target(APNS_VOIP), &wake, 1_500).await,
        Outcome::Failed("VoIP provider deadline elapsed".into())
    );
    assert_eq!(start.elapsed(), Duration::from_millis(500));
    Ok(())
}

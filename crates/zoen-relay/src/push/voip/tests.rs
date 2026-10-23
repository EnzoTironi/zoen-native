use super::*;
use crate::push::tests::{apns_cfg, Mock};

fn target(platform: &str) -> Target {
    Target {
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
    let disabled = Pusher::new(mock, None, None).unwrap();
    assert_eq!(
        disabled.call_wake(&target(APNS_VOIP), &fresh_wake()).await,
        Outcome::Disabled
    );
}

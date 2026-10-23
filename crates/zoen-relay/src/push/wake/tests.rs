use super::*;
use std::sync::Mutex;

struct Tokens {
    current: Mutex<Vec<Target>>,
    requested: Mutex<Vec<Vec<String>>>,
    removed: Mutex<Vec<String>>,
    fail: bool,
}

#[async_trait::async_trait]
impl DeviceTokens for Tokens {
    async fn targets(&self, identities: &[String]) -> Result<Vec<Target>, ()> {
        self.requested.lock().unwrap().push(identities.to_vec());
        if self.fail {
            Err(())
        } else {
            Ok(self.current.lock().unwrap().clone())
        }
    }
    async fn remove_dead(&self, token: &str) {
        self.removed.lock().unwrap().push(token.into());
    }
}

#[derive(Default)]
struct Transport(Mutex<Vec<String>>);

#[async_trait::async_trait]
impl WakeTransport for Transport {
    async fn wake(&self, target: &Target) -> Outcome {
        self.0.lock().unwrap().push(target.token.clone());
        match target.platform.as_str() {
            "gone" => Outcome::Gone,
            "failed" => Outcome::Failed("PRIVATE_PROVIDER_REASON_TOKEN_CANARY".into()),
            "disabled" => Outcome::Disabled,
            _ => Outcome::Sent,
        }
    }
}

fn tokens(platforms: &[&str]) -> Tokens {
    Tokens {
        current: Mutex::new(
            platforms
                .iter()
                .enumerate()
                .map(|(n, p)| Target {
                    platform: (*p).into(),
                    token: format!("SYNTHETIC_DEVICE_TOKEN_{n}"),
                })
                .collect(),
        ),
        requested: Mutex::default(),
        removed: Mutex::default(),
        fail: false,
    }
}

#[tokio::test]
async fn one_online_sibling_cannot_suppress_any_registered_recipient_device() {
    let store = tokens(&["apns", "fcm"]);
    let transport = Transport::default();
    // Presence intentionally cannot enter this admission port: it is not a
    // receipt that either device processed the encrypted event.
    let count = dispatch(
        &store,
        &transport,
        "author",
        &["author".into(), "recipient".into()],
    )
    .await;
    assert_eq!(
        *store.requested.lock().unwrap(),
        vec![vec!["recipient".to_string()]]
    );
    assert_eq!(count.targets, 2);
    assert_eq!(count.sent, 2);
    assert_eq!(
        *transport.0.lock().unwrap(),
        vec!["SYNTHETIC_DEVICE_TOKEN_0", "SYNTHETIC_DEVICE_TOKEN_1"]
    );
    assert!(store.removed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn every_wake_reloads_current_authorized_devices_after_revocation() {
    let store = tokens(&["apns", "fcm"]);
    let transport = Transport::default();
    let audience = ["author".into(), "recipient".into()];
    assert_eq!(
        dispatch(&store, &transport, "author", &audience).await.sent,
        2
    );
    // Model the existing SQL port's current, unrevoked rows at the next lookup.
    // This does not claim atomic revocation of a provider request already sent.
    store.current.lock().unwrap().remove(0);
    transport.0.lock().unwrap().clear();
    assert_eq!(
        dispatch(&store, &transport, "author", &audience).await.sent,
        1
    );
    assert_eq!(
        *transport.0.lock().unwrap(),
        vec!["SYNTHETIC_DEVICE_TOKEN_1"]
    );
    store.current.lock().unwrap().clear();
    transport.0.lock().unwrap().clear();
    assert_eq!(
        dispatch(&store, &transport, "author", &audience).await,
        WakeAttempts::default()
    );
    assert!(transport.0.lock().unwrap().is_empty());
    assert_eq!(
        *store.requested.lock().unwrap(),
        vec![vec!["recipient".to_string()]; 3]
    );
}

#[tokio::test]
async fn dead_token_cleanup_does_not_turn_failures_into_retry_or_disclose_reasons() {
    let store = tokens(&["gone", "failed", "disabled", "apns"]);
    let transport = Transport::default();
    let count = dispatch(&store, &transport, "author", &["recipient".into()]).await;
    assert_eq!(
        count,
        WakeAttempts {
            targets: 4,
            sent: 1,
            gone: 1,
            failed: 1,
            disabled: 1,
            lookup_failed: false
        }
    );
    assert_eq!(transport.0.lock().unwrap().len(), 4);
    assert_eq!(
        *store.removed.lock().unwrap(),
        vec!["SYNTHETIC_DEVICE_TOKEN_0"]
    );
    let public = format!("{count:?}");
    assert!(!public.contains("PRIVATE"));
    assert!(!public.contains("SYNTHETIC"));
}

#[tokio::test]
async fn empty_author_only_or_failed_lookup_never_falls_back_to_unscoped_tokens() {
    for audience in [vec![], vec!["author".into()]] {
        let store = tokens(&["apns"]);
        let transport = Transport::default();
        assert_eq!(
            dispatch(&store, &transport, "author", &audience).await,
            WakeAttempts::default()
        );
        assert!(store.requested.lock().unwrap().is_empty());
        assert!(transport.0.lock().unwrap().is_empty());
    }
    let mut store = tokens(&["apns"]);
    store.fail = true;
    let transport = Transport::default();
    let count = dispatch(&store, &transport, "author", &["recipient".into()]).await;
    assert!(count.lookup_failed);
    assert_eq!(count.targets, 0);
    assert!(transport.0.lock().unwrap().is_empty());
}

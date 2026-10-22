use super::*;

/// URL, headers, body.
type Call = (String, Vec<(String, String)>, String);

#[derive(Default)]
struct Mock {
    calls: Mutex<Vec<Call>>,
    replies: Mutex<Vec<(u16, String)>>,
}

#[async_trait::async_trait]
impl Http for Mock {
    async fn post(
        &self,
        url: &str,
        headers: &[(&str, String)],
        body: String,
    ) -> anyhow::Result<(u16, String)> {
        self.calls.lock().unwrap().push((
            url.into(),
            headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            body,
        ));
        let mut r = self.replies.lock().unwrap();
        Ok(if r.is_empty() {
            (200, String::new())
        } else {
            r.remove(0)
        })
    }
}

fn p256_pem() -> String {
    let doc = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
        .unwrap();
    let b = base64::engine::general_purpose::STANDARD.encode(doc.as_ref());
    format!("-----BEGIN PRIVATE KEY-----\n{b}\n-----END PRIVATE KEY-----\n")
}

fn apns_cfg() -> ApnsConfig {
    ApnsConfig {
        key_p8: p256_pem(),
        key_id: "KEY1234567".into(),
        team_id: "TEAM123456".into(),
        topic: "xyz.tironi.zoen".into(),
    }
}

fn target(p: &str) -> Target {
    Target {
        platform: p.into(),
        token: "abc".into(),
    }
}

/// Deterministic resource benchmark: hold every admitted job until the entire
/// burst is offered, so scheduler speed cannot hide an unbounded task backlog.
#[tokio::test]
async fn wake_admission_resource_benchmark() {
    const BURST: usize = 4096;
    const AUDIENCE_BYTES: usize = 1024;
    let mut baseline = Vec::new();
    let mut baseline_release = Vec::new();
    for _ in 0..BURST {
        let audience = vec![0u8; AUDIENCE_BYTES];
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        baseline_release.push(tx);
        baseline.push(tokio::spawn(async move {
            let _audience = audience;
            let _ = rx.await;
        }));
    }
    assert_eq!(baseline.len(), BURST);
    drop(baseline_release);
    for job in baseline {
        job.await.unwrap();
    }
    let p = Pusher::new(Arc::new(Mock::default()), None, None).unwrap();
    let other = Pusher::new(Arc::new(Mock::default()), None, None).unwrap();
    let mut jobs = Vec::new();
    let mut release = Vec::new();
    let mut cloned_bytes = 0;
    for _ in 0..BURST {
        let Some(permit) = p.try_wake_task() else {
            continue;
        };
        let audience = vec![0u8; AUDIENCE_BYTES];
        cloned_bytes += audience.len();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        release.push(tx);
        jobs.push(tokio::spawn(async move {
            let _permit = permit;
            let _audience = audience;
            let _ = rx.await;
        }));
    }
    assert_eq!(jobs.len(), MAX_WAKE_TASKS);
    assert_eq!(cloned_bytes, MAX_WAKE_TASKS * AUDIENCE_BYTES);
    assert!(p.try_wake_task().is_none());
    assert!(other.try_wake_task().is_some(), "limits are per Pusher");
    eprintln!(
        "wake burst={BURST}: before tasks={BURST} audience_bytes={}; after tasks={} audience_bytes={cloned_bytes}",
        BURST * AUDIENCE_BYTES,
        jobs.len()
    );
    drop(release);
    for job in jobs {
        job.await.unwrap();
    }
    assert_eq!(p.wake_slots.available_permits(), MAX_WAKE_TASKS);
    let permit = p.try_wake_task().unwrap();
    let job = tokio::spawn(async move {
        let _permit = permit;
        std::future::pending::<()>().await;
    });
    job.abort();
    assert!(job.await.unwrap_err().is_cancelled());
    assert_eq!(p.wake_slots.available_permits(), MAX_WAKE_TASKS);
}

#[tokio::test]
async fn off_without_config() {
    let p = Pusher::new(Arc::new(Mock::default()), None, None).unwrap();
    assert!(!p.enabled());
    assert_eq!(p.wake(&target(APNS)).await, Outcome::Disabled);
    assert_eq!(p.wake(&target(FCM)).await, Outcome::Disabled);
}

#[tokio::test]
async fn apns_wake_is_content_free_background() {
    let m = Arc::new(Mock::default());
    let p = Pusher::new(m.clone(), Some(apns_cfg()), None).unwrap();
    assert_eq!(p.wake(&target(APNS_SANDBOX)).await, Outcome::Sent);
    let calls = m.calls.lock().unwrap();
    let (url, headers, body) = &calls[0];
    assert_eq!(url, "https://api.sandbox.push.apple.com/3/device/abc");
    let h = |k: &str| headers.iter().find(|(n, _)| n == k).unwrap().1.clone();
    assert_eq!(h("apns-push-type"), "background");
    assert_eq!(h("apns-topic"), "xyz.tironi.zoen");
    let jwt = h("authorization");
    let header: Value = serde_json::from_slice(
        &B64.decode(jwt.trim_start_matches("bearer ").split('.').next().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(header["kid"], "KEY1234567");
    assert_eq!(body, r#"{"aps":{"content-available":1}}"#);
}

#[tokio::test]
async fn apns_dead_token_is_gone() {
    let m = Arc::new(Mock::default());
    m.replies
        .lock()
        .unwrap()
        .push((410, "{\"reason\":\"Unregistered\"}".into()));
    let p = Pusher::new(m, Some(apns_cfg()), None).unwrap();
    assert_eq!(p.wake(&target(APNS)).await, Outcome::Gone);
}

#[test]
fn bad_key_is_an_error() {
    let mut c = apns_cfg();
    c.key_p8 = "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----".into();
    assert!(Pusher::new(Arc::new(Mock::default()), Some(c), None).is_err());
}

#[test]
fn service_account_parses() {
    let c = FcmConfig::from_json(r#"{"project_id":"zoen","client_email":"a@b","private_key":"k"}"#)
        .unwrap();
    assert_eq!(c.project_id, "zoen");
    assert_eq!(c.token_uri, "https://oauth2.googleapis.com/token");
    assert!(FcmConfig::from_json(r#"{"project_id":"zoen"}"#).is_err());
}

#[test]
fn platforms() {
    assert!(valid_platform("fcm") && valid_platform("apns") && valid_platform("apns-sandbox"));
    assert!(!valid_platform("web"));
    assert!(valid_token("a1:B_-") && valid_token(""));
    assert!(!valid_token("abc/../x") && !valid_token("a?b"));
}

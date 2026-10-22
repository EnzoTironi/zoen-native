use super::*;

/// A throwaway RSA key from openssl; skipped where openssl is missing.
fn rsa_pem() -> Option<String> {
    let out = std::process::Command::new("openssl")
        .args([
            "genpkey",
            "-algorithm",
            "RSA",
            "-pkeyopt",
            "rsa_keygen_bits:2048",
        ])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8(out.stdout).ok())
        .flatten()
}

#[derive(Default)]
struct Mock(Mutex<Vec<(String, String)>>);

#[async_trait::async_trait]
impl Http for Mock {
    async fn post(
        &self,
        url: &str,
        _: &[(&str, String)],
        body: String,
    ) -> anyhow::Result<(u16, String)> {
        self.0.lock().unwrap().push((url.into(), body));
        Ok(if url.contains("oauth2") {
            (200, r#"{"access_token":"ya29","expires_in":3600}"#.into())
        } else if url.contains("messages:send") && self.0.lock().unwrap().len() > 3 {
            (404, r#"{"error":{"status":"NOT_FOUND"}}"#.into())
        } else {
            (200, "{}".into())
        })
    }
}

#[tokio::test]
async fn fcm_exchanges_once_and_sends_data_only() {
    let Some(private_key) = rsa_pem() else { return };
    let m = Arc::new(Mock::default());
    let cfg = FcmConfig {
        project_id: "zoen".into(),
        client_email: "push@zoen.iam.gserviceaccount.com".into(),
        private_key,
        token_uri: "https://oauth2.googleapis.com/token".into(),
    };
    let p = Pusher::new(m.clone(), None, Some(cfg)).unwrap();
    let t = Target {
        platform: FCM.into(),
        token: "tok".into(),
    };
    assert_eq!(p.wake(&t).await, Outcome::Sent);
    assert_eq!(p.wake(&t).await, Outcome::Sent);
    {
        let calls = m.0.lock().unwrap();
        assert_eq!(
            calls.iter().filter(|(u, _)| u.contains("oauth2")).count(),
            1
        );
        let send: Value = serde_json::from_str(&calls[1].1).unwrap();
        assert_eq!(send["message"]["data"]["wake"], "1");
        assert!(send["message"].get("notification").is_none());
    }
    assert_eq!(p.wake(&t).await, Outcome::Gone);
}

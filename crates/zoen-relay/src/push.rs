//! Wake pushes (ADR 0047). Messages are end-to-end encrypted, so a push never carries
//! content: it only wakes the app, which syncs, decrypts on the device and decides itself
//! whether to show a notification (mute lives on the device). APNs uses a token-based
//! `.p8` key; Android uses FCM HTTP v1 with a service account. Either is off when its
//! env is absent, and with both off pushes are a no-op.
//!
//! Env:
//!   ZOEN_APNS_KEY_P8     contents of the AuthKey_XXXX.p8 file (PEM)
//!   ZOEN_APNS_KEY_ID     the key's 10-character id
//!   ZOEN_APNS_TEAM_ID    the Apple Developer team id
//!   ZOEN_APNS_TOPIC      the app bundle id (e.g. xyz.tironi.zoen)
//!   ZOEN_FCM_SERVICE_ACCOUNT  the Firebase service-account JSON (project_id, client_email, private_key)

use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use ring::{
    rand::SystemRandom,
    signature::{EcdsaKeyPair, RsaKeyPair, ECDSA_P256_SHA256_FIXED_SIGNING, RSA_PKCS1_SHA256},
};
use serde_json::{json, Value};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

mod wake;
pub(crate) use wake::wake_registered_devices;

/// Bound whole wake jobs, including their audience clones and database/provider work.
const MAX_WAKE_TASKS: usize = 32;

pub const APNS: &str = "apns";
pub const APNS_SANDBOX: &str = "apns-sandbox";
pub const FCM: &str = "fcm";

pub fn valid_platform(p: &str) -> bool {
    matches!(p, APNS | APNS_SANDBOX | FCM)
}

/// Tokens go into provider URLs: only the characters APNs (hex) and FCM tokens use.
pub fn valid_token(t: &str) -> bool {
    t.len() <= 4096
        && t.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-'))
}

/// One device token to wake.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub platform: String,
    pub token: String,
}

/// What came of one push.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Sent,
    /// The provider says this token is dead: drop it.
    Gone,
    Failed(String),
    /// That platform isn't configured on this relay.
    Disabled,
}

/// Sends one HTTP POST. A trait so tests mock the network.
#[async_trait::async_trait]
pub trait Http: Send + Sync {
    async fn post(
        &self,
        url: &str,
        headers: &[(&str, String)],
        body: String,
    ) -> anyhow::Result<(u16, String)>;
}

pub struct Reqwest(reqwest::Client);

#[async_trait::async_trait]
impl Http for Reqwest {
    async fn post(
        &self,
        url: &str,
        headers: &[(&str, String)],
        body: String,
    ) -> anyhow::Result<(u16, String)> {
        let mut r = self.0.post(url).body(body);
        for (k, v) in headers {
            r = r.header(*k, v);
        }
        let res = r.send().await?;
        let status = res.status().as_u16();
        Ok((status, res.text().await.unwrap_or_default()))
    }
}

pub struct ApnsConfig {
    pub key_p8: String,
    pub key_id: String,
    pub team_id: String,
    pub topic: String,
}

pub struct FcmConfig {
    pub project_id: String,
    pub client_email: String,
    pub private_key: String,
    pub token_uri: String,
}

impl FcmConfig {
    pub fn from_json(s: &str) -> anyhow::Result<Self> {
        let v: Value = serde_json::from_str(s)?;
        let f = |k: &str| {
            v[k].as_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("service account is missing {k}"))
        };
        Ok(Self {
            project_id: f("project_id")?,
            client_email: f("client_email")?,
            private_key: f("private_key")?,
            token_uri: f("token_uri")
                .unwrap_or_else(|_| "https://oauth2.googleapis.com/token".into()),
        })
    }
}

/// A cached bearer and when it stops being good.
type Cached = Mutex<Option<(String, u64)>>;

struct Apns {
    key: EcdsaKeyPair,
    cfg: ApnsConfig,
    jwt: Cached,
}

struct Fcm {
    key: RsaKeyPair,
    cfg: FcmConfig,
    access: Cached,
}

pub struct Pusher {
    wake_slots: Arc<Semaphore>,
    http: Arc<dyn Http>,
    apns: Option<Apns>,
    fcm: Option<Fcm>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// PEM to DER (PKCS#8), without another dependency.
fn pem_der(pem: &str) -> anyhow::Result<Vec<u8>> {
    let b64: String = pem
        .replace("\\n", "\n")
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect::<Vec<_>>()
        .concat();
    Ok(base64::engine::general_purpose::STANDARD.decode(b64.trim())?)
}

fn jwt(
    header: Value,
    claims: Value,
    sign: impl FnOnce(&[u8]) -> anyhow::Result<Vec<u8>>,
) -> anyhow::Result<String> {
    let input = format!(
        "{}.{}",
        B64.encode(header.to_string()),
        B64.encode(claims.to_string())
    );
    let sig = sign(input.as_bytes())?;
    Ok(format!("{input}.{}", B64.encode(sig)))
}

impl Pusher {
    pub fn new(
        http: Arc<dyn Http>,
        apns: Option<ApnsConfig>,
        fcm: Option<FcmConfig>,
    ) -> anyhow::Result<Self> {
        let rng = SystemRandom::new();
        let apns = match apns {
            Some(cfg) => Some(Apns {
                key: EcdsaKeyPair::from_pkcs8(
                    &ECDSA_P256_SHA256_FIXED_SIGNING,
                    &pem_der(&cfg.key_p8)?,
                    &rng,
                )
                .map_err(|e| anyhow::anyhow!("APNs key: {e}"))?,
                cfg,
                jwt: Mutex::new(None),
            }),
            None => None,
        };
        let fcm = match fcm {
            Some(cfg) => Some(Fcm {
                key: RsaKeyPair::from_pkcs8(&pem_der(&cfg.private_key)?)
                    .map_err(|e| anyhow::anyhow!("FCM key: {e}"))?,
                cfg,
                access: Mutex::new(None),
            }),
            None => None,
        };
        Ok(Self {
            wake_slots: Arc::new(Semaphore::new(MAX_WAKE_TASKS)),
            http,
            apns,
            fcm,
        })
    }

    /// From env; a platform whose env is incomplete or broken is off (and says so once).
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let apns = match (
            var("ZOEN_APNS_KEY_P8"),
            var("ZOEN_APNS_KEY_ID"),
            var("ZOEN_APNS_TEAM_ID"),
            var("ZOEN_APNS_TOPIC"),
        ) {
            (Some(key_p8), Some(key_id), Some(team_id), Some(topic)) => Some(ApnsConfig {
                key_p8,
                key_id,
                team_id,
                topic,
            }),
            _ => None,
        };
        let fcm = var("ZOEN_FCM_SERVICE_ACCOUNT").and_then(|s| match FcmConfig::from_json(&s) {
            Ok(c) => Some(c),
            Err(error) => {
                tracing::warn!(%error, "FCM disabled: bad service account");
                None
            }
        });
        let http = Arc::new(Reqwest(
            reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        ));
        let (a, f) = (apns.is_some(), fcm.is_some());
        let p = Self::new(http.clone(), apns, None)
            .and_then(|mut p| {
                p.fcm = Self::new(http.clone(), None, fcm)?.fcm;
                Ok(p)
            })
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "push disabled: bad key");
                Self::disabled_with(http)
            });
        tracing::info!(
            apns = a && p.apns.is_some(),
            fcm = f && p.fcm.is_some(),
            "push"
        );
        p
    }

    fn disabled_with(http: Arc<dyn Http>) -> Self {
        Self {
            wake_slots: Arc::new(Semaphore::new(MAX_WAKE_TASKS)),
            http,
            apns: None,
            fcm: None,
        }
    }

    pub fn disabled() -> Self {
        Self::disabled_with(Arc::new(Reqwest(reqwest::Client::new())))
    }

    pub fn enabled(&self) -> bool {
        self.apns.is_some() || self.fcm.is_some()
    }

    /// Admit before cloning an audience or spawning. Wakes are best-effort: overload
    /// drops this wake rather than queuing tasks; durable events still sync normally.
    /// Keep the permit in the task until all database/provider work has finished.
    pub(crate) fn try_wake_task(&self) -> Option<OwnedSemaphorePermit> {
        self.wake_slots.clone().try_acquire_owned().ok()
    }

    pub async fn wake(&self, t: &Target) -> Outcome {
        let r = match t.platform.as_str() {
            APNS | APNS_SANDBOX => match &self.apns {
                Some(a) => self.apns(a, t).await,
                None => return Outcome::Disabled,
            },
            FCM => match &self.fcm {
                Some(f) => self.fcm(f, t).await,
                None => return Outcome::Disabled,
            },
            _ => return Outcome::Disabled,
        };
        r.unwrap_or_else(|e| Outcome::Failed(e.to_string()))
    }

    fn apns_jwt(a: &Apns) -> anyhow::Result<String> {
        let mut c = a.jwt.lock().unwrap();
        let t = now();
        if let Some((j, until)) = c.as_ref() {
            if *until > t {
                return Ok(j.clone());
            }
        }
        let j = jwt(
            json!({"alg": "ES256", "kid": a.cfg.key_id}),
            json!({"iss": a.cfg.team_id, "iat": t}),
            |m| {
                a.key
                    .sign(&SystemRandom::new(), m)
                    .map(|s| s.as_ref().to_vec())
                    .map_err(|_| anyhow::anyhow!("sign"))
            },
        )?;
        // Apple wants a new one between 20 and 60 minutes.
        *c = Some((j.clone(), t + 40 * 60));
        Ok(j)
    }

    async fn apns(&self, a: &Apns, t: &Target) -> anyhow::Result<Outcome> {
        let host = if t.platform == APNS_SANDBOX {
            "api.sandbox.push.apple.com"
        } else {
            "api.push.apple.com"
        };
        let (status, body) = self
            .http
            .post(
                &format!("https://{host}/3/device/{}", t.token),
                &[
                    ("authorization", format!("bearer {}", Self::apns_jwt(a)?)),
                    ("apns-topic", a.cfg.topic.clone()),
                    ("apns-push-type", "background".into()),
                    ("apns-priority", "5".into()),
                    ("content-type", "application/json".into()),
                ],
                json!({"aps": {"content-available": 1}}).to_string(),
            )
            .await?;
        Ok(match status {
            200 => Outcome::Sent,
            410 => Outcome::Gone,
            400 if body.contains("BadDeviceToken") => Outcome::Gone,
            s => Outcome::Failed(format!("apns {s}")),
        })
    }

    async fn fcm_access(&self, f: &Fcm) -> anyhow::Result<String> {
        let t = now();
        if let Some((tok, until)) = f.access.lock().unwrap().as_ref() {
            if *until > t {
                return Ok(tok.clone());
            }
        }
        let assertion = jwt(
            json!({"alg": "RS256", "typ": "JWT"}),
            json!({
                "iss": f.cfg.client_email,
                "scope": "https://www.googleapis.com/auth/firebase.messaging",
                "aud": f.cfg.token_uri,
                "iat": t,
                "exp": t + 3600,
            }),
            |m| {
                let mut sig = vec![0; f.key.public().modulus_len()];
                f.key
                    .sign(&RSA_PKCS1_SHA256, &SystemRandom::new(), m, &mut sig)
                    .map_err(|_| anyhow::anyhow!("sign"))?;
                Ok(sig)
            },
        )?;
        let (status, body) = self
            .http
            .post(
                &f.cfg.token_uri,
                &[("content-type", "application/x-www-form-urlencoded".into())],
                format!(
                    "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={assertion}"
                ),
            )
            .await?;
        anyhow::ensure!(status == 200, "fcm token exchange {status}");
        let v: Value = serde_json::from_str(&body)?;
        let tok = v["access_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("no access_token"))?
            .to_string();
        let ttl = v["expires_in"].as_u64().unwrap_or(3600);
        *f.access.lock().unwrap() = Some((tok.clone(), t + ttl.saturating_sub(300)));
        Ok(tok)
    }

    async fn fcm(&self, f: &Fcm, t: &Target) -> anyhow::Result<Outcome> {
        let access = self.fcm_access(f).await?;
        let (status, body) = self
            .http
            .post(
                &format!(
                    "https://fcm.googleapis.com/v1/projects/{}/messages:send",
                    f.cfg.project_id
                ),
                &[
                    ("authorization", format!("Bearer {access}")),
                    ("content-type", "application/json".into()),
                ],
                json!({"message": {
                    "token": t.token,
                    "data": {"wake": "1"},
                    "android": {"priority": "high"},
                }})
                .to_string(),
            )
            .await?;
        Ok(match status {
            200 => Outcome::Sent,
            404 => Outcome::Gone,
            // Only UNREGISTERED means a dead token; INVALID_ARGUMENT can be our own bad
            // payload, and treating it as Gone would wipe every Android token.
            400 | 403 if body.contains("UNREGISTERED") => Outcome::Gone,
            s => Outcome::Failed(format!("fcm {s}")),
        })
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fcm_tests;

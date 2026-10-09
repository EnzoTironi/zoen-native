//! Growth on the device (ADR 0044): where this install came from, the cached remote config,
//! the onboarding plan for that source and arm, flags and remote copy, and the signed
//! report that tells the relay only the source kind, the arms shown and (opt-in) app health.

use roda_proto::experiments::{
    attribution_from_link, report_message, Attribution, ClientReport, Exposure, Health,
    RemoteConfig, SourceKind,
};
use sha2::{Digest, Sha256};

use crate::{CoreError, RodaEngine};

fn invalid(reason: String) -> CoreError {
    CoreError::Invalid { reason }
}

const META_UNIT: &str = "growth:unit";
const META_SOURCE: &str = "growth:source";
const META_SOURCE_SENT: &str = "growth:source_sent";
const META_CONFIG: &str = "growth:config";
const META_ETAG: &str = "growth:etag";
const META_EXPOSURES: &str = "growth:exposures";

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AcquisitionDto {
    /// `friend`, `space`, `campaign` or `organic`.
    pub kind: String,
    pub campaign: Option<String>,
    /// The friend's handle or the Space invite code; stays on this device.
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OnboardingPlanDto {
    pub flow: String,
    /// Screen ids in order (`hello`, `profile`, `areas`, `plan`, `agents`, `notifications`,
    /// `location`, `done`); the app skips ids it doesn't know.
    pub steps: Vec<String>,
    /// `home`, `chat_with_inviter` or `space`.
    pub landing: String,
    pub landing_target: Option<String>,
    /// Copy overrides for this flow and arm, `{target}` already filled in.
    pub copy: std::collections::HashMap<String, String>,
    /// `flag:variant` of the experiment that chose the flow, if any.
    pub experiment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct GrowthSyncDto {
    pub config_version: u64,
    pub reported: bool,
}

impl RodaEngine {
    fn meta(&self, key: &str) -> Option<String> {
        self.lock().store.meta(key).ok().flatten()
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<(), CoreError> {
        Ok(self.lock().store.set_meta(key, value)?)
    }

    /// This install's experiment unit, made on first use.
    fn unit(&self) -> String {
        if let Some(u) = self.meta(META_UNIT) {
            return u;
        }
        let mut b = [0u8; 16];
        getrandom::getrandom(&mut b).expect("os randomness");
        let u = hex::encode(b);
        let _ = self.set_meta(META_UNIT, &u);
        u
    }

    fn config(&self) -> RemoteConfig {
        self.meta(META_CONFIG)
            .and_then(|j| RemoteConfig::parse(&j).ok())
            .unwrap_or_else(RemoteConfig::builtin)
    }

    fn source(&self) -> Attribution {
        self.meta(META_SOURCE)
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_else(Attribution::organic)
    }

    fn mark_exposed(&self, flag: &str, variant: &str) {
        let mut pending: Vec<Exposure> = self
            .meta(META_EXPOSURES)
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();
        if !pending.iter().any(|e| e.flag == flag) {
            pending.push(Exposure {
                flag: flag.into(),
                variant: variant.into(),
            });
            if let Ok(j) = serde_json::to_string(&pending) {
                let _ = self.set_meta(META_EXPOSURES, &j);
            }
        }
    }
}

fn dto(a: &Attribution) -> AcquisitionDto {
    AcquisitionDto {
        kind: a.kind.as_str().into(),
        campaign: a.campaign.clone(),
        target: a.target.clone(),
    }
}

#[uniffi::export]
impl RodaEngine {
    /// Remembers the link that opened the app, if it's the first one with a source (first
    /// touch: later links don't overwrite it). Returns the source on record.
    pub fn growth_capture_link(&self, link: String) -> AcquisitionDto {
        let current = self.meta(META_SOURCE);
        if current.is_none() {
            if let Some(a) = attribution_from_link(&link) {
                if a.kind != SourceKind::Organic || a.campaign.is_some() {
                    if let Ok(j) = serde_json::to_string(&a) {
                        let _ = self.set_meta(META_SOURCE, &j);
                    }
                }
            }
        }
        dto(&self.source())
    }

    pub fn growth_acquisition(&self) -> AcquisitionDto {
        dto(&self.source())
    }

    /// The onboarding to show now. Showing it counts as an exposure of the arm that chose it.
    pub fn growth_onboarding_plan(&self) -> OnboardingPlanDto {
        let unit = self.unit();
        let source = self.source();
        let plan = self.config().onboarding_for(&unit, &source);
        if let Some(a) = plan.exposure.as_ref().filter(|a| a.enrolled) {
            self.mark_exposed(&a.flag, &a.variant);
        }
        let target = plan.landing_target.clone().unwrap_or_default();
        let source_target = source.target.clone().unwrap_or_default();
        let fill = |v: &String| {
            v.replace(
                "{target}",
                if target.is_empty() {
                    &source_target
                } else {
                    &target
                },
            )
        };
        OnboardingPlanDto {
            flow: plan.flow,
            steps: plan.steps,
            landing: plan.landing,
            landing_target: plan.landing_target,
            copy: plan
                .copy
                .iter()
                .map(|(k, v)| (k.clone(), fill(v)))
                .collect(),
            experiment: plan.exposure.map(|a| format!("{}:{}", a.flag, a.variant)),
        }
    }

    /// The arm of a flag for this install (`None` for an unknown flag). Reading it counts
    /// as an exposure when the install is enrolled in that experiment.
    pub fn growth_variant(&self, flag: String) -> Option<String> {
        let a = self.config().assign(&flag, &self.unit())?;
        if a.enrolled {
            self.mark_exposed(&a.flag, &a.variant);
        }
        Some(a.variant)
    }

    /// Remote copy for `key`: `key@<lang>` first, then `key`. `None` = use the app's own.
    pub fn growth_copy(&self, key: String, lang: String) -> Option<String> {
        let copy = self.config().copy_for(&self.unit());
        copy.get(&format!("{key}@{lang}"))
            .or_else(|| copy.get(&key))
            .cloned()
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl RodaEngine {
    /// Refreshes the remote config from `relay_url` (or the account's relay) and, once
    /// there's a registered, unlocked account, sends what's pending: the source (once), the
    /// arms shown, and app health when `share_health` is on. Safe to call often.
    pub async fn growth_sync(
        &self,
        relay_url: Option<String>,
        share_health: bool,
        sessions: u32,
        crashes: u32,
    ) -> Result<GrowthSyncDto, CoreError> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let account = self.account();
        let relay = relay_url
            .or_else(|| account.as_ref().map(|a| a.relay_url.clone()))
            .ok_or_else(|| invalid("no relay".into()))?;
        let base = crate::net::http_base(&relay);
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| invalid(e.to_string()))?;

        let mut req = http.get(format!("{base}/v1/config"));
        if let Some(etag) = self.meta(META_ETAG) {
            req = req.header("if-none-match", etag);
        }
        if let Ok(r) = req.send().await {
            if r.status().is_success() {
                let etag = r
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                if let Ok(text) = r.text().await {
                    if RemoteConfig::parse(&text).is_ok() {
                        self.set_meta(META_CONFIG, &text)?;
                        if let Some(e) = etag {
                            self.set_meta(META_ETAG, &e)?;
                        }
                    }
                }
            }
        }
        let version = self.config().version;

        let Some(acct) = account.filter(|a| a.registered && a.unlocked) else {
            return Ok(GrowthSyncDto {
                config_version: version,
                reported: false,
            });
        };
        let key = {
            let e = self.lock();
            e.net.author.as_ref().map(|a| a.key.clone())
        };
        let Some(key) = key else {
            return Ok(GrowthSyncDto {
                config_version: version,
                reported: false,
            });
        };
        let source_pending = self.meta(META_SOURCE_SENT).is_none();
        let exposures: Vec<Exposure> = self
            .meta(META_EXPOSURES)
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();
        let health = (share_health && sessions > 0).then_some(Health { sessions, crashes });
        if !source_pending && exposures.is_empty() && health.is_none() {
            return Ok(GrowthSyncDto {
                config_version: version,
                reported: false,
            });
        }
        let report = ClientReport {
            unit: self.unit(),
            attribution: source_pending.then(|| self.source().reportable()),
            exposures: exposures.clone(),
            health,
        };
        let body = serde_json::to_vec(&report).map_err(|e| invalid(e.to_string()))?;
        let ts = crate::engine::now_ms();
        let relay_name = relay_name(&acct.relay_url);
        let sha = hex::encode(Sha256::digest(&body));
        let sig = key.sign(&report_message(&sha, ts, &relay_name));
        let r = http
            .post(format!("{base}/v1/report"))
            .header("content-type", "application/json")
            .header("x-zoen-device", key.id())
            .header("x-zoen-ts", ts.to_string())
            .header("x-zoen-sig", sig)
            .body(body)
            .send()
            .await
            .map_err(|e| invalid(e.to_string()))?;
        if !r.status().is_success() {
            return Err(invalid(format!("report refused ({})", r.status())));
        }
        if source_pending {
            self.set_meta(META_SOURCE_SENT, "1")?;
        }
        // Keep exposures that arrived while this report was in flight.
        let now: Vec<Exposure> = self
            .meta(META_EXPOSURES)
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();
        let rest: Vec<&Exposure> = now.iter().filter(|e| !exposures.contains(e)).collect();
        self.set_meta(
            META_EXPOSURES,
            &serde_json::to_string(&rest).unwrap_or_else(|_| "[]".into()),
        )?;
        Ok(GrowthSyncDto {
            config_version: version,
            reported: true,
        })
    }
}

/// The relay's name as it signs logins: the host (and port) of its URL.
fn relay_name(url: &str) -> String {
    let s = url
        .trim_end_matches('/')
        .split("://")
        .last()
        .unwrap_or(url)
        .to_string();
    s.split('/').next().unwrap_or(&s).to_string()
}

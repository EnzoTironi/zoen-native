//! Growth on the device (ADR 0044): where this install came from, the cached remote config,
//! the onboarding plan for that source and arm, flags and remote copy, and the signed
//! report that tells the relay only the source kind, the arms shown and (opt-in) app health.

use roda_proto::experiments::{
    attribution_from_link, report_message, Attribution, ClientReport, Exposure, Health,
    RemoteConfig, SourceKind,
};
use sha2::{Digest, Sha256};

use crate::{sync::AccountMeta, CoreError, Engine, RodaEngine};

fn invalid(reason: String) -> CoreError {
    CoreError::Invalid { reason }
}

const META_UNIT: &str = "growth:unit";
const META_SOURCE: &str = "growth:source";
const META_SOURCE_SENT: &str = "growth:source_sent";
const META_CONFIG: &str = "growth:config";
const META_ETAG: &str = "growth:etag";
const META_EXPOSURES: &str = "growth:exposures";

fn same_account(current: Option<&AccountMeta>, expected: Option<&AccountMeta>) -> bool {
    let scope = |a: &AccountMeta| (a.identity.clone(), a.device.clone(), a.relay_url.clone());
    current.map(scope) == expected.map(scope)
}

fn install_unit(engine: &Engine) -> String {
    if let Some(unit) = engine.store.meta(META_UNIT).ok().flatten() {
        return unit;
    }
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("os randomness");
    let unit = hex::encode(bytes);
    let _ = engine.store.set_meta(META_UNIT, &unit);
    unit
}

fn cached_config(engine: &Engine) -> RemoteConfig {
    engine
        .store
        .meta(META_CONFIG)
        .ok()
        .flatten()
        .and_then(|json| RemoteConfig::parse(&json).ok())
        .unwrap_or_else(RemoteConfig::builtin)
}

fn acquisition(engine: &Engine) -> Attribution {
    engine
        .store
        .meta(META_SOURCE)
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_else(Attribution::organic)
}

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
        install_unit(&self.lock())
    }

    fn config(&self) -> RemoteConfig {
        cached_config(&self.lock())
    }

    fn source(&self) -> Attribution {
        acquisition(&self.lock())
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
    /// there's a registered, unlocked account with a completed relay login, sends what's
    /// pending: the source (once), the arms shown, and opt-in app health. Safe to call often.
    pub async fn growth_sync(
        &self,
        relay_url: Option<String>,
        share_health: bool,
        sessions: u32,
        crashes: u32,
    ) -> Result<GrowthSyncDto, CoreError> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (account, etag) = {
            let e = self.lock();
            (e.net.account.clone(), e.store.meta(META_ETAG)?)
        };
        let relay = relay_url
            .or_else(|| account.as_ref().map(|a| a.relay_url.clone()))
            .ok_or_else(|| invalid("no relay".into()))?;
        let base = crate::net::http_base(&relay);
        let http = crate::net::http_client(std::time::Duration::from_secs(15))
            .map_err(|e| invalid(e.to_string()))?;

        let mut req = http.get(format!("{base}/v1/config"));
        if let Some(etag) = etag {
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
                        let e = self.lock();
                        if !same_account(e.net.account.as_ref(), account.as_ref()) {
                            return Ok(GrowthSyncDto {
                                config_version: cached_config(&e).version,
                                reported: false,
                            });
                        }
                        e.store.set_meta(META_CONFIG, &text)?;
                        if let Some(tag) = etag {
                            e.store.set_meta(META_ETAG, &tag)?;
                        }
                    }
                }
            }
        }
        // Credentials and pending data belong to one account, even if another task signs out.
        let (key, authenticated_relay, source_pending, source_snapshot, exposures, body, version) = {
            let e = self.lock();
            let version = cached_config(&e).version;
            // A relay's signing name can differ from its URL. Only login establishes it.
            let credentials = e.net.report_credentials(&relay);
            let Some((key, authenticated_relay)) =
                credentials.filter(|_| same_account(e.net.account.as_ref(), account.as_ref()))
            else {
                return Ok(GrowthSyncDto {
                    config_version: version,
                    reported: false,
                });
            };
            let source_pending = e.store.meta(META_SOURCE_SENT)?.is_none();
            let source_snapshot = e.store.meta(META_SOURCE)?;
            let exposures: Vec<Exposure> = e
                .store
                .meta(META_EXPOSURES)?
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default();
            let health = (share_health && sessions > 0).then_some(Health { sessions, crashes });
            if !source_pending && exposures.is_empty() && health.is_none() {
                return Ok(GrowthSyncDto {
                    config_version: version,
                    reported: false,
                });
            }
            let report = ClientReport {
                unit: install_unit(&e),
                attribution: source_pending.then(|| acquisition(&e).reportable()),
                exposures: exposures.clone(),
                health,
            };
            let body = serde_json::to_vec(&report).map_err(|err| invalid(err.to_string()))?;
            (
                key,
                authenticated_relay,
                source_pending,
                source_snapshot,
                exposures,
                body,
                version,
            )
        };
        let ts = crate::engine::now_ms();
        let sha = hex::encode(Sha256::digest(&body));
        let sig = key.sign(&report_message(&sha, ts, &authenticated_relay.name));
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
        let e = self.lock();
        // An old account's accepted request must not consume a new account's pending report.
        if e.net.report_credentials(&relay).map(|(_, r)| r) != Some(authenticated_relay) {
            return Ok(GrowthSyncDto {
                config_version: version,
                reported: true,
            });
        }
        if source_pending && e.store.meta(META_SOURCE)? == source_snapshot {
            e.store.set_meta(META_SOURCE_SENT, "1")?;
        }
        // Keep exposures that arrived while this report was in flight.
        let now: Vec<Exposure> = e
            .store
            .meta(META_EXPOSURES)?
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();
        let rest: Vec<&Exposure> = now.iter().filter(|e| !exposures.contains(e)).collect();
        e.store.set_meta(
            META_EXPOSURES,
            &serde_json::to_string(&rest).unwrap_or_else(|_| "[]".into()),
        )?;
        Ok(GrowthSyncDto {
            config_version: version,
            reported: true,
        })
    }
}

#[cfg(test)]
mod tests;

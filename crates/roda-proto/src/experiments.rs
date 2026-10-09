//! Remote config, feature flags, experiments and the onboarding router (ADR 0044).
//!
//! One JSON document, served by the relay at `GET /v1/config` and changed by an admin
//! without an App Store release, says which flags exist, how traffic splits, which copy
//! each arm shows and which onboarding flow each acquisition source gets. Devices cache it
//! and evaluate it themselves; the relay evaluates the same code to check what devices
//! report. Bucketing is deterministic on the install's *unit*: 128 random bits made on
//! first open, before there is an account (onboarding is the first thing tested). It
//! links to nothing, and nobody has to store who is in which arm.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Buckets per split: weights and percentages resolve to 0.01%.
pub const BUCKETS: u32 = 10_000;

/// The built-in config: what a device uses before it ever reaches the relay, and what a
/// relay serves until an admin stores another.
pub const DEFAULT_CONFIG: &str = include_str!("remote-config.json");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct RemoteConfig {
    pub version: u64,
    #[serde(default)]
    pub holdout: Option<Holdout>,
    #[serde(default)]
    pub flags: BTreeMap<String, Flag>,
    /// Copy and UI values everyone gets unless an arm overrides them.
    #[serde(default)]
    pub copy: BTreeMap<String, String>,
    #[serde(default)]
    pub onboarding: Onboarding,
}

/// A slice of accounts kept out of every experiment and rollout, to measure their sum.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Holdout {
    pub salt: String,
    /// 0–100.
    pub percent: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Flag {
    pub salt: String,
    /// Share of accounts in the flag at all (0–100); the rest get the first variant.
    #[serde(default = "full")]
    pub rollout: f64,
    /// First = control / default.
    pub variants: Vec<Variant>,
    /// An experiment is analysed (exposures, stats); a plain flag is a rollout.
    #[serde(default)]
    pub experiment: bool,
    /// Metrics that must not get worse (`retention_d1`, `crash_free`, `send_latency`).
    #[serde(default)]
    pub guardrails: Vec<String>,
}

fn full() -> f64 {
    100.0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Variant {
    pub key: String,
    pub weight: u32,
    /// Copy/UI values this arm overrides.
    #[serde(default)]
    pub copy: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Onboarding {
    #[serde(default)]
    pub flows: BTreeMap<String, Flow>,
    /// First match wins; a rule without `source` matches everyone.
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Flow {
    /// Screen ids the app knows; unknown ones are skipped by the app.
    pub steps: Vec<String>,
    /// `home`, `chat_with_inviter` or `space`.
    pub landing: String,
    #[serde(default)]
    pub copy: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Rule {
    #[serde(default)]
    pub source: Option<SourceKind>,
    /// Only for this campaign id (ads, creators).
    #[serde(default)]
    pub campaign: Option<String>,
    /// Without an experiment: this flow.
    #[serde(default)]
    pub flow: Option<String>,
    /// With one: the arm picks the flow (`arms: { control: default, direct: friend_invite }`).
    #[serde(default)]
    pub experiment: Option<String>,
    #[serde(default)]
    pub arms: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// A friend's personal link: `zoen://friend/<handle>`, `https://tryzoen.com/@<handle>`.
    Friend,
    /// A Space invite: `zoen://join/<code>`, `https://tryzoen.com/j/<code>`.
    Space,
    /// An ad or creator campaign with no friend or Space behind it.
    Campaign,
    /// Nothing: App Store search, word of mouth.
    Organic,
}

impl SourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceKind::Friend => "friend",
            SourceKind::Space => "space",
            SourceKind::Campaign => "campaign",
            SourceKind::Organic => "organic",
        }
    }

    pub fn parse(s: &str) -> Option<SourceKind> {
        Some(match s {
            "friend" => SourceKind::Friend,
            "space" => SourceKind::Space,
            "campaign" => SourceKind::Campaign,
            "organic" => SourceKind::Organic,
            _ => return None,
        })
    }
}

/// Where an install came from, read from the link that opened the app (first touch). The
/// target (a handle or invite code) stays on the device; only `kind` and `campaign` are
/// ever reported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Attribution {
    pub kind: SourceKind,
    #[serde(default)]
    pub campaign: Option<String>,
    /// Local only: the friend's handle or the Space invite code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

impl Attribution {
    pub fn organic() -> Attribution {
        Attribution {
            kind: SourceKind::Organic,
            campaign: None,
            target: None,
        }
    }

    /// What the relay may learn: the kind and the campaign, never the target.
    pub fn reportable(&self) -> Attribution {
        Attribution {
            target: None,
            ..self.clone()
        }
    }
}

/// Campaign ids: 1–32 of `a-z 0-9 _ -`, lowercased. Anything else is dropped, so a link
/// can't smuggle an id or free text into the metrics.
pub fn clean_campaign(raw: &str) -> Option<String> {
    let c = raw.trim().to_lowercase();
    let ok = (1..=32).contains(&c.len())
        && c.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
    ok.then_some(c)
}

fn clean_target(raw: &str) -> Option<String> {
    let t = raw.trim().trim_start_matches('@');
    let ok = (3..=32).contains(&t.len())
        && t.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.');
    ok.then(|| t.to_string())
}

/// Reads a link that opened the app. Hosts other than ours give `None`.
pub fn attribution_from_link(link: &str) -> Option<Attribution> {
    let link = link.trim();
    let (rest, query) = link.split_once('?').unwrap_or((link, ""));
    let path = if let Some(p) = rest.strip_prefix("zoen://") {
        p.to_string()
    } else {
        let no_scheme = rest
            .strip_prefix("https://")
            .or_else(|| rest.strip_prefix("http://"))?;
        let (host, path) = no_scheme.split_once('/').unwrap_or((no_scheme, ""));
        let host = host.to_lowercase();
        if !(host == "tryzoen.com" || host.ends_with(".tryzoen.com") || host == "zoen.app") {
            return None;
        }
        path.to_string()
    };
    let campaign = query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| matches!(*k, "c" | "utm_campaign" | "campaign"))
        .and_then(|(_, v)| clean_campaign(v));
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let (kind, target) = match parts.as_slice() {
        ["friend" | "u", h, ..] => (SourceKind::Friend, clean_target(h)),
        [h, ..] if h.starts_with('@') => (SourceKind::Friend, clean_target(h)),
        ["join" | "j", code, ..] => (SourceKind::Space, clean_target(code)),
        _ if campaign.is_some() => (SourceKind::Campaign, None),
        _ => (SourceKind::Organic, None),
    };
    if kind != SourceKind::Organic && kind != SourceKind::Campaign && target.is_none() {
        return Some(Attribution {
            kind: if campaign.is_some() {
                SourceKind::Campaign
            } else {
                SourceKind::Organic
            },
            campaign,
            target: None,
        });
    }
    Some(Attribution {
        kind,
        campaign,
        target,
    })
}

/// A unit is 32 lowercase hex characters (a fresh random one per install).
pub fn valid_unit(u: &str) -> bool {
    u.len() == 32
        && u.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A deterministic unit, for tests and tools: 32 hex characters of SHA-256 over a tag and
/// a seed.
pub fn unit_for(identity: &str) -> String {
    let d = Sha256::new()
        .chain_update(b"zoen-unit-v1\0")
        .chain_update(identity.as_bytes())
        .finalize();
    hex::encode(&d[..16])
}

/// 0..[`BUCKETS`], uniform, independent per salt.
pub fn bucket(salt: &str, unit: &str) -> u32 {
    let d = Sha256::new()
        .chain_update(salt.as_bytes())
        .chain_update(b"\0")
        .chain_update(unit.as_bytes())
        .finalize();
    u32::from_be_bytes([d[0], d[1], d[2], d[3]]) % BUCKETS
}

fn pct_buckets(p: f64) -> u32 {
    (p.clamp(0.0, 100.0) * (BUCKETS as f64) / 100.0).round() as u32
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Assignment {
    pub flag: String,
    pub variant: String,
    /// In the experiment (not held out, inside the rollout): its exposures count.
    pub enrolled: bool,
}

impl RemoteConfig {
    pub fn parse(json: &str) -> Result<RemoteConfig, String> {
        let c: RemoteConfig = serde_json::from_str(json).map_err(|e| e.to_string())?;
        c.validate()?;
        Ok(c)
    }

    pub fn builtin() -> RemoteConfig {
        RemoteConfig::parse(DEFAULT_CONFIG).expect("built-in config is valid")
    }

    pub fn validate(&self) -> Result<(), String> {
        for (k, f) in &self.flags {
            if f.variants.is_empty() {
                return Err(format!("flag {k} has no variants"));
            }
            if f.variants.iter().map(|v| v.weight).sum::<u32>() == 0 {
                return Err(format!("flag {k} has zero total weight"));
            }
        }
        for r in &self.onboarding.rules {
            let flows: Vec<&String> = r.flow.iter().chain(r.arms.values()).collect();
            for f in flows {
                if !self.onboarding.flows.contains_key(f) {
                    return Err(format!("onboarding rule names an unknown flow {f}"));
                }
            }
            if let Some(e) = &r.experiment {
                let flag = self
                    .flags
                    .get(e)
                    .ok_or_else(|| format!("onboarding rule names an unknown experiment {e}"))?;
                for v in &flag.variants {
                    if !r.arms.contains_key(&v.key) {
                        return Err(format!("experiment {e} arm {} has no flow", v.key));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn in_holdout(&self, unit: &str) -> bool {
        self.holdout
            .as_ref()
            .is_some_and(|h| bucket(&h.salt, unit) < pct_buckets(h.percent))
    }

    /// The arm of one flag for this unit.
    pub fn assign(&self, flag: &str, unit: &str) -> Option<Assignment> {
        let f = self.flags.get(flag)?;
        let control = || Assignment {
            flag: flag.to_string(),
            variant: f.variants[0].key.clone(),
            enrolled: false,
        };
        if self.in_holdout(unit) {
            return Some(control());
        }
        if bucket(&format!("{}:rollout", f.salt), unit) >= pct_buckets(f.rollout) {
            return Some(control());
        }
        let total: u32 = f.variants.iter().map(|v| v.weight).sum();
        let b = (u64::from(bucket(&f.salt, unit)) * u64::from(total) / u64::from(BUCKETS)) as u32;
        let mut acc = 0;
        for v in &f.variants {
            acc += v.weight;
            if b < acc {
                return Some(Assignment {
                    flag: flag.to_string(),
                    variant: v.key.clone(),
                    enrolled: true,
                });
            }
        }
        Some(control())
    }

    pub fn assignments(&self, unit: &str) -> Vec<Assignment> {
        self.flags
            .keys()
            .filter_map(|k| self.assign(k, unit))
            .collect()
    }

    /// Copy for this unit: defaults, then each flag's arm overrides (flags in name order).
    pub fn copy_for(&self, unit: &str) -> BTreeMap<String, String> {
        let mut out = self.copy.clone();
        for a in self.assignments(unit) {
            if let Some(v) = self.flags[&a.flag]
                .variants
                .iter()
                .find(|v| v.key == a.variant)
            {
                out.extend(v.copy.clone());
            }
        }
        out
    }

    /// The onboarding a new account gets for where it came from and its arms.
    pub fn onboarding_for(&self, unit: &str, source: &Attribution) -> OnboardingPlan {
        let matches = |r: &Rule| {
            r.source.is_none_or(|s| s == source.kind)
                && r.campaign
                    .as_ref()
                    .is_none_or(|c| source.campaign.as_ref() == Some(c))
        };
        let mut exposure = None;
        let mut flow_id = "default".to_string();
        if let Some(rule) = self.onboarding.rules.iter().find(|r| matches(r)) {
            if let Some(exp) = &rule.experiment {
                if let Some(a) = self.assign(exp, unit) {
                    flow_id = rule
                        .arms
                        .get(&a.variant)
                        .cloned()
                        .unwrap_or_else(|| "default".into());
                    exposure = Some(a);
                }
            } else if let Some(f) = &rule.flow {
                flow_id = f.clone();
            }
        }
        let flow = self
            .onboarding
            .flows
            .get(&flow_id)
            .cloned()
            .unwrap_or_else(|| Flow {
                steps: vec!["hello".into(), "profile".into(), "done".into()],
                landing: "home".into(),
                copy: BTreeMap::new(),
            });
        let mut copy = self.copy_for(unit);
        copy.extend(flow.copy.clone());
        // A landing that needs a target falls back to home without one.
        let landing = match flow.landing.as_str() {
            "chat_with_inviter" if source.kind == SourceKind::Friend && source.target.is_some() => {
                flow.landing.clone()
            }
            "space" if source.kind == SourceKind::Space && source.target.is_some() => {
                flow.landing.clone()
            }
            _ => "home".into(),
        };
        OnboardingPlan {
            flow: flow_id,
            steps: flow.steps,
            landing,
            landing_target: if landing_needs_target(&flow.landing) {
                source.target.clone()
            } else {
                None
            },
            copy,
            exposure,
        }
    }
}

fn landing_needs_target(l: &str) -> bool {
    matches!(l, "chat_with_inviter" | "space")
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OnboardingPlan {
    pub flow: String,
    pub steps: Vec<String>,
    pub landing: String,
    /// The friend's handle or the invite code (local only).
    pub landing_target: Option<String>,
    pub copy: BTreeMap<String, String>,
    /// The experiment arm that chose this flow, to report as an exposure once shown.
    pub exposure: Option<Assignment>,
}

/// What a device tells the relay (`POST /v1/report`, signed by the device): where the
/// account came from, which arms it was shown, and opt-in aggregate app health. No ids
/// beyond the signing device, no content, no free text.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ClientReport {
    /// The install's unit, so the relay can check the arms against the config.
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub attribution: Option<Attribution>,
    #[serde(default)]
    pub exposures: Vec<Exposure>,
    /// Only when the person opted in to sharing app health.
    #[serde(default)]
    pub health: Option<Health>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Exposure {
    pub flag: String,
    pub variant: String,
}

/// Counts since the last report.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Health {
    pub sessions: u32,
    pub crashes: u32,
}

/// What a device signs to send a [`ClientReport`].
pub fn report_message(body_sha256: &str, ts_ms: i64, relay: &str) -> Vec<u8> {
    format!(
        "{}:client-report:{relay}:{body_sha256}:{ts_ms}",
        crate::PROTOCOL
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_is_valid_and_has_both_onboarding_arms() {
        let c = RemoteConfig::builtin();
        assert!(c.onboarding.flows.contains_key("default"));
        assert!(c.onboarding.flows.contains_key("friend_invite"));
        assert!(c.flags["onboarding_friend_v1"].experiment);
    }

    #[test]
    fn links_become_sources_without_leaking_targets() {
        let f = attribution_from_link("https://tryzoen.com/@ana_b?c=ig_reels_01").unwrap();
        assert_eq!(f.kind, SourceKind::Friend);
        assert_eq!(f.target.as_deref(), Some("ana_b"));
        assert_eq!(f.campaign.as_deref(), Some("ig_reels_01"));
        assert_eq!(f.reportable().target, None);
        let s = attribution_from_link("zoen://join/AB12CD34EF").unwrap();
        assert_eq!(
            (s.kind, s.target.as_deref()),
            (SourceKind::Space, Some("AB12CD34EF"))
        );
        let c = attribution_from_link("https://tryzoen.com/?utm_campaign=Ads-X").unwrap();
        assert_eq!(
            (c.kind, c.campaign.as_deref()),
            (SourceKind::Campaign, Some("ads-x"))
        );
        let bad = attribution_from_link("https://tryzoen.com/?c=Ana%20Silva%20cpf").unwrap();
        assert_eq!((bad.kind, bad.campaign), (SourceKind::Organic, None));
        assert!(attribution_from_link("https://evil.example/@ana").is_none());
    }

    #[test]
    fn bucketing_is_deterministic_uniform_and_weighted() {
        let c = RemoteConfig::builtin();
        let mut direct = 0;
        for i in 0..4000 {
            let u = unit_for(&format!("id{i}"));
            let a = c.assign("onboarding_friend_v1", &u).unwrap();
            assert_eq!(Some(a.clone()), c.assign("onboarding_friend_v1", &u));
            if a.variant == "direct" {
                direct += 1;
            }
        }
        // 50/50 inside a 5% holdout: ~47.5% direct.
        assert!((1700..2100).contains(&direct), "{direct}");
        // Known vector, shared with the Swift side.
        assert_eq!(bucket("s", "u"), bucket("s", "u"));
        assert_ne!(unit_for("a"), unit_for("b"));
    }

    #[test]
    fn onboarding_follows_source_and_arm() {
        let c = RemoteConfig::builtin();
        let friend = attribution_from_link("zoen://friend/ana_b").unwrap();
        let (mut saw_direct, mut saw_control) = (false, false);
        for i in 0..200 {
            let u = unit_for(&format!("x{i}"));
            let p = c.onboarding_for(&u, &friend);
            let arm = p.exposure.as_ref().unwrap().variant.clone();
            if arm == "direct" {
                saw_direct = true;
                assert_eq!(p.flow, "friend_invite");
                assert_eq!(p.landing, "chat_with_inviter");
                assert_eq!(p.landing_target.as_deref(), Some("ana_b"));
            } else {
                saw_control = true;
                assert_eq!(p.flow, "default");
                assert_eq!(p.landing, "home");
            }
        }
        assert!(saw_direct && saw_control);
        let organic = c.onboarding_for(&unit_for("z"), &Attribution::organic());
        assert_eq!(
            (organic.flow.as_str(), organic.landing.as_str()),
            ("default", "home")
        );
        assert!(organic.exposure.is_none());
        let space = c.onboarding_for(
            &unit_for("z"),
            &attribution_from_link("zoen://join/AB12CD34EF").unwrap(),
        );
        assert_eq!(
            (space.flow.as_str(), space.landing.as_str()),
            ("space_link", "space")
        );
    }

    #[test]
    fn bad_configs_are_refused() {
        let mut c = RemoteConfig::builtin();
        c.onboarding.rules[0].arms.clear();
        assert!(c.validate().is_err());
        assert!(RemoteConfig::parse(
            "{\"version\":1,\"flags\":{\"x\":{\"salt\":\"s\",\"variants\":[]}}}"
        )
        .is_err());
    }
}

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

use num_traits::ToPrimitive;
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
    /// `home`, `chat_with_inviter`, `space` or `agent`.
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
    /// An agent link: `zoen://agent/<handle|code>`, `https://tryzoen.com/a/<handle|code>`.
    Agent,
    /// An ad or creator campaign with no friend or Space behind it.
    Campaign,
    /// Nothing: App Store search, word of mouth.
    Organic,
}

impl SourceKind {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceKind::Friend => "friend",
            SourceKind::Space => "space",
            SourceKind::Agent => "agent",
            SourceKind::Campaign => "campaign",
            SourceKind::Organic => "organic",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<SourceKind> {
        Some(match s {
            "friend" => SourceKind::Friend,
            "space" => SourceKind::Space,
            "agent" => SourceKind::Agent,
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
    /// Local only: the friend's handle, Space invite code, or agent handle/code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

impl Attribution {
    #[must_use]
    pub fn organic() -> Attribution {
        Attribution {
            kind: SourceKind::Organic,
            campaign: None,
            target: None,
        }
    }

    /// What the relay may learn: the kind and the campaign, never the target.
    #[must_use]
    pub fn reportable(&self) -> Attribution {
        Attribution {
            target: None,
            ..self.clone()
        }
    }
}

/// Campaign ids: 1–32 of `a-z 0-9 _ -`, lowercased. Anything else is dropped, so a link
/// can't smuggle an id or free text into the metrics.
#[must_use]
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
#[must_use]
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
        ["agent" | "a", h, ..] => (SourceKind::Agent, clean_target(h)),
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
#[must_use]
pub fn valid_unit(u: &str) -> bool {
    u.len() == 32
        && u.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A deterministic unit, for tests and tools: 32 hex characters of SHA-256 over a tag and
/// a seed.
#[must_use]
pub fn unit_for(identity: &str) -> String {
    let d = Sha256::new()
        .chain_update(b"zoen-unit-v1\0")
        .chain_update(identity.as_bytes())
        .finalize();
    hex::encode(&d[..16])
}

/// 0..[`BUCKETS`], uniform, independent per salt.
#[must_use]
pub fn bucket(salt: &str, unit: &str) -> u32 {
    let d = Sha256::new()
        .chain_update(salt.as_bytes())
        .chain_update(b"\0")
        .chain_update(unit.as_bytes())
        .finalize();
    u32::from_be_bytes([d[0], d[1], d[2], d[3]]) % BUCKETS
}

fn pct_buckets(p: f64) -> u32 {
    // Preserve the old clamped conversion, including NaN -> zero, without a lossy cast.
    (p.clamp(0.0, 100.0) * f64::from(BUCKETS) / 100.0)
        .round()
        .to_u32()
        .unwrap_or(0)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Assignment {
    pub flag: String,
    pub variant: String,
    /// In the experiment (not held out, inside the rollout): its exposures count.
    pub enrolled: bool,
}

impl RemoteConfig {
    /// # Errors
    /// Rejects malformed JSON, empty/overflowing variant weights or unknown onboarding flows.
    pub fn parse(json: &str) -> Result<RemoteConfig, String> {
        let c: RemoteConfig = serde_json::from_str(json).map_err(|e| e.to_string())?;
        c.validate()?;
        Ok(c)
    }

    /// # Errors
    /// Rejects an invalid embedded configuration using the same validation as remote updates.
    pub fn builtin() -> Result<RemoteConfig, String> {
        RemoteConfig::parse(DEFAULT_CONFIG)
    }

    /// # Errors
    /// Rejects empty/overflowing weights or onboarding rules that reference missing flows/arms.
    pub fn validate(&self) -> Result<(), String> {
        for (k, f) in &self.flags {
            if f.variants.is_empty() {
                return Err(format!("flag {k} has no variants"));
            }
            let weight = f
                .variants
                .iter()
                .try_fold(0u32, |sum, v| sum.checked_add(v.weight))
                .ok_or_else(|| format!("flag {k} total weight overflows"))?;
            if weight == 0 {
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

    #[must_use]
    pub fn in_holdout(&self, unit: &str) -> bool {
        self.holdout
            .as_ref()
            .is_some_and(|h| bucket(&h.salt, unit) < pct_buckets(h.percent))
    }

    /// The arm of one flag for this unit.
    #[must_use]
    pub fn assign(&self, flag: &str, unit: &str) -> Option<Assignment> {
        let f = self.flags.get(flag)?;
        let first = f.variants.first()?;
        let total = f
            .variants
            .iter()
            .try_fold(0u32, |sum, v| sum.checked_add(v.weight))?;
        if total == 0 {
            return None;
        }
        let control = || Assignment {
            flag: flag.to_string(),
            variant: first.key.clone(),
            enrolled: false,
        };
        if self.in_holdout(unit) {
            return Some(control());
        }
        if bucket(&format!("{}:rollout", f.salt), unit) >= pct_buckets(f.rollout) {
            return Some(control());
        }
        let b =
            u32::try_from(u64::from(bucket(&f.salt, unit)) * u64::from(total) / u64::from(BUCKETS))
                .ok()?;
        let mut acc = 0u32;
        for v in &f.variants {
            acc = acc.checked_add(v.weight)?;
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

    #[must_use]
    pub fn assignments(&self, unit: &str) -> Vec<Assignment> {
        self.flags
            .keys()
            .filter_map(|k| self.assign(k, unit))
            .collect()
    }

    /// Copy for this unit: defaults, then each flag's arm overrides (flags in name order).
    #[must_use]
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
    #[must_use]
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
                flow_id.clone_from(f);
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
            "agent" if source.kind == SourceKind::Agent && source.target.is_some() => {
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
    matches!(l, "chat_with_inviter" | "space" | "agent")
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
#[must_use]
pub fn report_message(body_sha256: &str, ts_ms: i64, relay: &str) -> Vec<u8> {
    format!(
        "{}:client-report:{relay}:{body_sha256}:{ts_ms}",
        crate::PROTOCOL
    )
    .into_bytes()
}

#[cfg(test)]
mod tests;

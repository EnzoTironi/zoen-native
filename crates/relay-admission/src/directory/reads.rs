//! Native lookup normalization, profile bounds and signed ownership filtering.
use serde_json::Value;
use types::{Identity, IdentityKind};

pub const MAX_PROFILE_IDS: usize = 500;
pub const PREFIX_LIMIT: usize = 20;
pub const LOOKUP_INTERVAL_NS: u64 = 1_000_000_000;
pub const LOOKUP_BURST: u32 = 60;

pub fn normalize_lookup(handle: &str) -> String {
    handle.trim().trim_start_matches('@').to_lowercase()
}

/// Preserve native prefix behavior: percent and backslash are stripped, while
/// underscores remain literal rather than acting as SQL LIKE wildcards.
pub fn prefix_pattern(handle: &str) -> String {
    format!("{}%", handle.replace(['\\', '%'], "").replace('_', "\\_"))
}

pub fn profile_ids(ids: Vec<String>) -> Vec<String> {
    ids.into_iter().take(MAX_PROFILE_IDS).collect()
}

pub fn authorized_profile(
    id: &str,
    kind: &str,
    owner: Option<&str>,
    json: Value,
) -> Option<Identity> {
    let profile: Identity = serde_json::from_value(json).ok()?;
    let expected = match profile.kind {
        IdentityKind::Person => "Person",
        IdentityKind::Agent => "Agent",
    };
    (profile.id == id
        && expected == kind
        && profile.owner.as_deref() == owner
        && event_log::agent_owner::profile_authorized(&profile))
    .then_some(profile)
}

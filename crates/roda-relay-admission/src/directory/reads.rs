//! Directory read selection and reply policy. Hosts supply rows from the existing
//! authoritative directory; profile ownership validation stays in shared Rust.
use super::{encoded, internal, parse, Session};
use crate::portable::{parse_u64, BoundaryError};
use roda_proto::{DeviceRecord, Op, Reply, ServerFrame};
use roda_types::{Identity, IdentityKind};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeSet, time::Duration};

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
        && roda_log::agent_owner::profile_authorized(&profile))
    .then_some(profile)
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    Lookup {
        handle: String,
        prefix: bool,
        pattern: String,
    },
    Profiles {
        ids: Vec<String>,
    },
    Devices,
}

pub fn query(op: &Op) -> Option<Query> {
    Some(match op {
        Op::Lookup { handle, prefix } => {
            let handle = normalize_lookup(handle);
            Query::Lookup {
                pattern: prefix_pattern(&handle),
                handle,
                prefix: *prefix,
            }
        }
        Op::Profiles { ids } => Query::Profiles {
            ids: profile_ids(ids.clone()),
        },
        Op::Devices => Query::Devices,
        _ => return None,
    })
}

pub fn spends_lookup_budget(op: &Op) -> bool {
    matches!(
        op,
        Op::Lookup { .. } | Op::DeliverLink { .. } | Op::ClaimKeyPackages { .. }
    )
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileRow {
    pub id: String,
    pub kind: String,
    pub owner: Option<String>,
    pub profile: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceRow {
    pub identity: String,
    pub device: String,
    pub revoked: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Facts {
    pub query: Query,
    pub profiles: Vec<ProfileRow>,
    pub devices: Vec<DeviceRow>,
}

pub fn reply(op: &Op, session: &Session, facts: &Facts) -> Result<Reply, BoundaryError> {
    let expected = query(op).ok_or_else(|| internal("invalid directory read operation"))?;
    if facts.query != expected {
        return Err(internal("directory rows belong to another read query"));
    }
    match &expected {
        Query::Devices => {
            if !facts.profiles.is_empty()
                || facts
                    .devices
                    .iter()
                    .any(|row| row.identity != session.identity)
            {
                return Err(internal("device rows belong to another identity"));
            }
            Ok(Reply::Devices(
                facts
                    .devices
                    .iter()
                    .map(|row| DeviceRecord {
                        device: row.device.clone(),
                        revoked: row.revoked,
                    })
                    .collect(),
            ))
        }
        Query::Lookup { handle, prefix, .. } => {
            let limit = if handle.is_empty() {
                0
            } else if *prefix {
                PREFIX_LIMIT
            } else {
                1
            };
            if !facts.devices.is_empty() || facts.profiles.len() > limit {
                return Err(internal(
                    "directory lookup rows exceed the native query bound",
                ));
            }
            Ok(Reply::Profiles(profiles(&facts.profiles)))
        }
        Query::Profiles { ids } => {
            let requested: BTreeSet<_> = ids.iter().map(String::as_str).collect();
            let mut selected = BTreeSet::new();
            if !facts.devices.is_empty()
                || facts.profiles.iter().any(|row| {
                    !requested.contains(row.id.as_str()) || !selected.insert(row.id.as_str())
                })
            {
                return Err(internal(
                    "profile rows do not match the requested identities",
                ));
            }
            Ok(Reply::Profiles(profiles(&facts.profiles)))
        }
    }
}

fn profiles(rows: &[ProfileRow]) -> Vec<Identity> {
    rows.iter()
        .filter_map(|row| {
            authorized_profile(
                &row.id,
                &row.kind,
                row.owner.as_deref(),
                row.profile.clone(),
            )
        })
        .collect()
}

pub fn lookup_rate_json(context: &str) -> Result<String, BoundaryError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Rate {
        now_ns: String,
        tat_ns: Option<String>,
    }
    let input: Rate = parse(context)?;
    let now = parse_u64(&input.now_ns)?;
    let tat = input.tat_ns.as_deref().map(parse_u64).transpose()?;
    encoded(
        &match crate::gcra::decide(now, tat, 1, LOOKUP_INTERVAL_NS, LOOKUP_BURST) {
            crate::gcra::Decision::Accepted { next_ns } => {
                json!({"accepted":true,"next_ns":next_ns.to_string()})
            }
            crate::gcra::Decision::Rejected { wait_ns } => {
                json!({"accepted":false,"reason":roda_proto::slow_down(Duration::from_nanos(wait_ns))})
            }
        },
    )
}

pub fn reply_json(bytes: &[u8]) -> Result<String, BoundaryError> {
    let frame = ServerFrame::decode(bytes).map_err(|_| internal("invalid directory read reply"))?;
    if frame.encode() != bytes {
        return Err(internal("directory read reply is not canonical"));
    }
    let ServerFrame::Res { id, result } = frame else {
        return Err(internal("expected directory response"));
    };
    encoded(&match result {
        Ok(Reply::Profiles(profiles)) => {
            json!({"id":id.to_string(),"kind":"profiles","profiles":profiles})
        }
        Ok(Reply::Devices(devices)) => {
            json!({"id":id.to_string(),"kind":"devices","devices":devices.into_iter().map(|row| json!({"device":row.device,"revoked":row.revoked})).collect::<Vec<_>>()})
        }
        Err(reason) => json!({"id":id.to_string(),"kind":"rejected","reason":reason}),
        _ => return Err(internal("unsupported directory read reply")),
    })
}

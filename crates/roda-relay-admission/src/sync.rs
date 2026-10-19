//! Catch-up visibility shared by the native relay and portable hosts.
//!
//! The host authenticates the device and reads membership/removal facts. A plan is
//! an admission snapshot, not a delivery lease: every outgoing frame still needs
//! the host's current device-authorization fence.

use crate::portable::{parse_u64, BoundaryError, SessionContext, MAX_FRAME_BYTES};
use roda_proto::{ClientFrame, Cursor};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncRead {
    pub space: String,
    pub from: u64,
    /// A removed member learns of the removal itself, never events after it.
    pub through: Option<u64>,
}

/// Decide one requested cursor using facts for that exact Space and identity.
pub fn cursor_read(cursor: Cursor, member: bool, removed_at: Option<u64>) -> Option<SyncRead> {
    let through = if member {
        None
    } else {
        Some(removed_at.filter(|last| cursor.next_seq <= *last)?)
    };
    Some(SyncRead {
        space: cursor.space,
        from: cursor.next_seq,
        through,
    })
}

/// `all` includes current Spaces absent from the caller's cursors, from genesis.
/// Preserve native cursor order and repeats; they are not silently coalesced.
pub fn include_current(plan: &mut Vec<SyncRead>, current: &[String], all: bool) {
    if all {
        let mut included: BTreeSet<_> = plan.iter().map(|read| read.space.clone()).collect();
        for space in current {
            if included.insert(space.clone()) {
                plan.push(SyncRead {
                    space: space.clone(),
                    from: 0,
                    through: None,
                });
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Facts {
    identity: String,
    current_spaces: Vec<String>,
    /// Exact decimal removal sequence per Space, read for `identity`.
    removed_at: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Context {
    session: SessionContext,
    facts: Facts,
}

#[derive(Serialize)]
struct PortableRead {
    space: String,
    from: String,
    through: Option<String>,
}

fn invalid(reason: &str) -> BoundaryError {
    BoundaryError::internal("invalid_sync_context", reason)
}

/// Binary ClientFrame plus private facts; no client-supplied membership is used.
/// UInt64 values remain canonical decimal strings across the WASM/JSON boundary.
pub fn plan_json(frame: &[u8], context: &str) -> Result<String, BoundaryError> {
    if frame.is_empty() || frame.len() > MAX_FRAME_BYTES {
        return Err(BoundaryError::no("sync", "invalid sync frame size"));
    }
    let decoded =
        ClientFrame::decode(frame).map_err(|_| BoundaryError::no("sync", "invalid sync frame"))?;
    if decoded.encode() != frame {
        return Err(BoundaryError::no("sync", "sync frame is not canonical"));
    }
    let ClientFrame::Sync { cursors, all } = decoded else {
        return Err(BoundaryError::no("sync", "expected sync"));
    };
    if context.len() > MAX_FRAME_BYTES {
        return Err(invalid("sync context too large"));
    }
    let context: Context =
        serde_json::from_str(context).map_err(|_| invalid("invalid trusted sync context"))?;
    if !crate::identity_key(&context.session.authenticated_identity)
        || !crate::identity_key(&context.session.authenticated_device)
        || context.session.authenticated_identity != context.facts.identity
    {
        return Err(invalid("membership facts belong to another identity"));
    }
    let removed: BTreeMap<_, _> = context
        .facts
        .removed_at
        .iter()
        .map(|(space, at)| Ok((space.as_str(), parse_u64(at)?)))
        .collect::<Result<_, BoundaryError>>()?;
    let current: BTreeSet<_> = context
        .facts
        .current_spaces
        .iter()
        .map(String::as_str)
        .collect();
    let mut plan: Vec<_> = cursors
        .into_iter()
        .filter_map(|cursor| {
            let member = current.contains(cursor.space.as_str());
            let removed_at = removed.get(cursor.space.as_str()).copied();
            cursor_read(cursor, member, removed_at)
        })
        .collect();
    include_current(&mut plan, &context.facts.current_spaces, all);
    serde_json::to_string(
        &plan
            .into_iter()
            .map(|read| PortableRead {
                space: read.space,
                from: read.from.to_string(),
                through: read.through.map(|last| last.to_string()),
            })
            .collect::<Vec<_>>(),
    )
    .map_err(|_| invalid("sync plan encoding failed"))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(js_name = planSync)]
    pub fn plan(frame: &[u8], context: &str) -> Result<String, JsValue> {
        super::plan_json(frame, context).map_err(|error| {
            JsValue::from_str(
                &serde_json::to_string(&error).unwrap_or_else(|_| "sync planning failed".into()),
            )
        })
    }
}

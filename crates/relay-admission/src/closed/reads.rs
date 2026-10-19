//! Private Space-owned catch-up visibility. The host loads this projection from
//! the same local transaction as its page. Directory owns active-device admission
//! and must fence every actual socket send again after any intervening await.
use super::{encode, internal, read_json, ClosedProjection};
use crate::portable::{parse_u64, BoundaryError, SessionContext};
use protocol::Cursor;
use serde::{Deserialize, Serialize};

pub const MAX_PAGE_ENTRIES: u32 = 256;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Visibility {
    pub member: bool,
    pub removed_at: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ReadPlan {
    pub from: String,
    /// Inclusive and capped to the authoritative head in this transaction.
    /// None means this session has no visible row at or after `from`.
    pub through: Option<String>,
    pub limit: u32,
    pub visibility: Visibility,
}

fn validate_session(session: &SessionContext) -> Result<(), BoundaryError> {
    if !crate::identity_key(&session.authenticated_identity)
        || !crate::identity_key(&session.authenticated_device)
    {
        return Err(internal("invalid trusted Space read session"));
    }
    Ok(())
}

pub fn visibility(
    state: &ClosedProjection,
    session: &SessionContext,
) -> Result<Visibility, BoundaryError> {
    state.validate(&state.space, true)?;
    validate_session(session)?;
    let who = &session.authenticated_identity;
    Ok(Visibility {
        member: state.members.contains_key(who),
        removed_at: state.removed_at.get(who).cloned(),
    })
}

/// Current co-members for profile invalidation. Discovery is not membership:
/// unknown/removed callers learn no roster, and an identity never returns itself.
/// The trusted Directory owns active-device authorization and final socket sends.
pub fn co_members(
    state: &ClosedProjection,
    session: &SessionContext,
) -> Result<Vec<String>, BoundaryError> {
    if !visibility(state, session)?.member {
        return Ok(vec![]);
    }
    Ok(state
        .members
        .keys()
        .filter(|identity| *identity != &session.authenticated_identity)
        .cloned()
        .collect())
}

pub fn plan(
    state: &ClosedProjection,
    session: &SessionContext,
    from: u64,
    limit: u32,
) -> Result<ReadPlan, BoundaryError> {
    if limit == 0 || limit > MAX_PAGE_ENTRIES {
        return Err(internal("invalid authorized Space read limit"));
    }
    let visibility = visibility(state, session)?;
    let removed = visibility
        .removed_at
        .as_deref()
        .map(parse_u64)
        .transpose()?;
    let read = crate::sync::cursor_read(
        Cursor {
            space: state.space.clone(),
            next_seq: from,
        },
        visibility.member,
        removed,
    );
    let head = state
        .head
        .as_ref()
        .map(|head| parse_u64(&head.seq))
        .transpose()?;
    let through = read.and_then(|read| {
        head.map(|head| read.through.unwrap_or(head).min(head))
            .filter(|last| from <= *last)
    });
    Ok(ReadPlan {
        from: from.to_string(),
        through: through.map(|value| value.to_string()),
        limit,
        visibility,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Context {
    space_id: String,
    session: SessionContext,
    projection: Option<ClosedProjection>,
}

fn context(input: &str) -> Result<(ClosedProjection, SessionContext), BoundaryError> {
    let context: Context = read_json(input)?;
    // Empty/unknown Spaces remain read-only; this does not create a binding.
    let state = context
        .projection
        .unwrap_or_else(|| ClosedProjection::empty(&context.space_id));
    state.validate(&context.space_id, true)?;
    Ok((state, context.session))
}

pub fn visibility_json(input: &str) -> Result<String, BoundaryError> {
    let (state, session) = context(input)?;
    encode(&visibility(&state, &session)?)
}

pub fn co_members_json(input: &str) -> Result<String, BoundaryError> {
    let (state, session) = context(input)?;
    encode(&co_members(&state, &session)?)
}

pub fn plan_json(input: &str, from: &str, limit: u32) -> Result<String, BoundaryError> {
    let (state, session) = context(input)?;
    encode(&plan(&state, &session, parse_u64(from)?, limit)?)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;
    fn error(value: crate::portable::BoundaryError) -> JsValue {
        JsValue::from_str(
            &serde_json::to_string(&value).unwrap_or_else(|_| "Space read failed".into()),
        )
    }
    #[wasm_bindgen(js_name = closedVisibility)]
    pub fn visibility(input: &str) -> Result<String, JsValue> {
        super::visibility_json(input).map_err(error)
    }
    #[wasm_bindgen(js_name = closedCoMembers)]
    pub fn co_members(input: &str) -> Result<String, JsValue> {
        super::co_members_json(input).map_err(error)
    }
    #[wasm_bindgen(js_name = closedReadPlan)]
    pub fn plan(input: &str, from: &str, limit: u32) -> Result<String, JsValue> {
        super::plan_json(input, from, limit).map_err(error)
    }
}

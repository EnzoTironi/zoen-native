//! Prospective discovery facts for a private, Directory-admitted publication.
//! This is not a capability, session authorizer, durable ticket or mutation plan.
//! The host binds tickets to exact bytes/context and re-runs this at commit.
use super::{closed_publish, closed_replay_json, encode, internal, read_json, ClosedContext};
use crate::portable::{verify_publish, BoundaryError, SessionContext};
use serde::Serialize;
use types::EventBody;

#[derive(Debug, Serialize)]
pub struct Metadata {
    pub space_id: String,
    pub frame_hash: String,
    pub identity: String,
    pub device: String,
    pub client_id: String,
}

#[derive(Debug, Serialize)]
pub struct Preview {
    #[serde(flatten)]
    pub metadata: Metadata,
    pub grants: Vec<String>,
}

pub fn metadata(frame: &[u8], session: &SessionContext) -> Result<Metadata, BoundaryError> {
    let verified = verify_publish(frame, session)?;
    Ok(Metadata {
        space_id: verified.space().into(),
        frame_hash: event_log::sha256_hex(frame),
        identity: verified.author().into(),
        device: session.authenticated_device.clone(),
        client_id: verified.client_id().into(),
    })
}

pub fn preview(
    frame: &[u8],
    context: &ClosedContext,
    receipt: Option<&[u8]>,
) -> Result<Preview, BoundaryError> {
    let metadata = metadata(frame, &context.session)?;
    if let Some(receipt) = receipt {
        let state = context
            .projection
            .as_ref()
            .ok_or_else(|| internal("stored receipt has no projection"))?;
        // Validate the actual signed retry and original canonical receipt, using
        // CURRENT duplicate audience semantics. A retry grants no membership.
        closed_replay_json(frame, &encode(&context.session)?, &encode(state)?, receipt)?;
        return Ok(Preview {
            metadata,
            grants: vec![],
        });
    }
    let accepted = closed_publish(frame, context)?;
    let grants = if let Some(joined) = accepted.joined {
        // Includes an admitted role-change Add: catalogue insertion is idempotent.
        vec![joined]
    } else {
        let verified = verify_publish(frame, &context.session)?;
        if matches!(verified.env.body(), Some(EventBody::SpaceCreated { .. })) {
            vec![accepted.projection.creator]
        } else {
            vec![]
        }
    };
    Ok(Preview { metadata, grants })
}

pub fn metadata_json(frame: &[u8], session: &str) -> Result<String, BoundaryError> {
    encode(&metadata(frame, &read_json(session)?)?)
}

pub fn preview_json(frame: &[u8], context: &str, receipt: &[u8]) -> Result<String, BoundaryError> {
    encode(&preview(
        frame,
        &read_json(context)?,
        (!receipt.is_empty()).then_some(receipt),
    )?)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;
    fn error(value: crate::portable::BoundaryError) -> JsValue {
        JsValue::from_str(
            &serde_json::to_string(&value)
                .unwrap_or_else(|_| "relay publication preview failed".into()),
        )
    }
    #[wasm_bindgen(js_name=relayPublicationMetadata)]
    pub fn metadata(frame: &[u8], session: &str) -> Result<String, JsValue> {
        super::metadata_json(frame, session).map_err(error)
    }
    #[wasm_bindgen(js_name=closedRelayPreview)]
    pub fn preview(frame: &[u8], context: &str, receipt: &[u8]) -> Result<String, JsValue> {
        super::preview_json(frame, context, receipt).map_err(error)
    }
}

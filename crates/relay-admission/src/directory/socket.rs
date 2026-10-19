//! Canonical binary control frames for the private Directory-owned socket lane.
//! Durable authorization and the synchronous final send remain host responsibilities.
use super::{decode, encoded, internal, no, parse, Session};
use base64::{engine::general_purpose::STANDARD, Engine};
use protocol::{ClientFrame, ErrorCode, ServerFrame};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn incoming(bytes: &[u8]) -> Result<Value, crate::portable::BoundaryError> {
    Ok(match decode(bytes)? {
        ClientFrame::Hello { capabilities, .. } => {
            json!({"kind":"hello","claim_receipts":capabilities.iter().any(|value|value==protocol::KEY_PACKAGE_CLAIM_CAPABILITY)})
        }
        ClientFrame::Auth { .. } => json!({"kind":"auth"}),
        ClientFrame::Req { id, .. } => json!({"kind":"request","id":id.to_string()}),
        ClientFrame::Ping => json!({"kind":"ping"}),
        _ => json!({"kind":"unsupported"}),
    })
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Control {
    Ready {
        session: Session,
    },
    Pong {
        server_time_ms: Option<String>,
    },
    Error {
        stage: String,
        reason: String,
        permanent: bool,
    },
    Refused {
        id: String,
        reason: String,
    },
}
fn frame(bytes: Vec<u8>) -> Result<String, crate::portable::BoundaryError> {
    encoded(&json!({"frame_base64":STANDARD.encode(bytes)}))
}
pub fn control_json(context: &str) -> Result<String, crate::portable::BoundaryError> {
    let control: Control = parse(context)?;
    let result = match control {
        Control::Ready { session } => ServerFrame::Ready {
            identity: session.identity,
            registered: session.registered,
        },
        Control::Pong { server_time_ms } => ServerFrame::Pong {
            server_time_ms: server_time_ms
                .map(|value| {
                    value
                        .parse::<i64>()
                        .map_err(|_| internal("invalid socket clock"))
                })
                .transpose()?,
        },
        Control::Refused { id, reason } => {
            let id = crate::portable::parse_u64(&id)
                .map_err(|_| internal("invalid socket request id"))?;
            ServerFrame::Res {
                id,
                result: Err(reason),
            }
        }
        Control::Error {
            stage,
            reason,
            permanent,
        } => {
            let code = if !permanent {
                ErrorCode::Unavailable
            } else if stage == "hello" && reason.starts_with("protocol ") {
                ErrorCode::UpgradeRequired
            } else if [
                "device certificate invalid",
                "login signature invalid",
                "this device was unlinked",
                "link this device first",
            ]
            .contains(&reason.as_str())
            {
                ErrorCode::Unauthorized
            } else {
                ErrorCode::Other
            };
            ServerFrame::error(code, reason)
        }
    };
    frame(result.encode())
}
/// Private trusted-service delivery only: this validates wire bounds/canonical bytes,
/// not Space membership. The sender must already own the data's audience decision.
pub fn delivery_json(bytes: &[u8]) -> Result<String, crate::portable::BoundaryError> {
    if bytes.is_empty() || bytes.len() > crate::portable::MAX_FRAME_BYTES {
        return Err(no("invalid socket delivery size"));
    }
    let decoded = ServerFrame::decode(bytes).map_err(|_| no("invalid socket delivery"))?;
    if decoded.encode() != bytes {
        return Err(no("socket delivery is not canonical"));
    }
    if matches!(
        decoded,
        ServerFrame::Challenge { .. } | ServerFrame::Ready { .. }
    ) {
        return Err(no("handshake frames cannot use private delivery"));
    }
    frame(bytes.to_vec())
}
pub fn incoming_json(bytes: &[u8]) -> Result<String, crate::portable::BoundaryError> {
    encoded(&incoming(bytes)?)
}

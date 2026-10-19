//! Native protocol codec and bounded metadata for authenticated relay composition.
//! Directory owns durable active-device admission; Space owns membership/log.
use base64::{engine::general_purpose::STANDARD, Engine};
use protocol::{ClientFrame, Op, Reply, Sequenced, ServerFrame};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

use crate::portable::{parse_u64, BoundaryError, MAX_FRAME_BYTES};

fn no(reason: &str) -> BoundaryError {
    BoundaryError::no("relay_socket", reason)
}
fn internal(reason: &str) -> BoundaryError {
    BoundaryError::internal("invalid_relay_socket_context", reason)
}
fn encode(value: &impl serde::Serialize) -> Result<String, BoundaryError> {
    serde_json::to_string(value).map_err(|_| internal("relay socket encoding failed"))
}
fn parse<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, BoundaryError> {
    if value.len() > MAX_FRAME_BYTES {
        return Err(internal("relay socket context too large"));
    }
    serde_json::from_str(value).map_err(|_| internal("invalid relay socket context"))
}
fn decode(bytes: &[u8]) -> Result<ClientFrame, BoundaryError> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(no("invalid relay frame size"));
    }
    let frame = ClientFrame::decode(bytes).map_err(|_| no("invalid relay frame"))?;
    // Preserve current private canonical-input bound explicitly. Native prost
    // accepts a broader byte language; compatibility tests must retain that gap.
    if frame.encode() != bytes {
        return Err(no("relay frame is not canonical"));
    }
    Ok(frame)
}

/// Metadata only, never an authenticated operation or membership capability.
/// The host first bounds queued bytes, then obtains real Directory admission.
pub fn incoming_json(bytes: &[u8]) -> Result<String, BoundaryError> {
    let value = match decode(bytes)? {
        ClientFrame::Publish { env } => json!({
            "kind":"publish", "space":env.space(), "client_id":env.client_id(),
            "frame_hash":event_log::sha256_hex(bytes)
        }),
        ClientFrame::Req { id, op } => json!({
            "kind":"request", "id":id.to_string(),
            "register":matches!(&op,Op::Register { .. }),
            "refill":matches!(&op,Op::PublishKeyPackages { .. }),
            // Metadata only: use after the actual Directory request succeeds.
            "profile_changed":match &op {
                Op::PutProfile { profile } => Some(json!({
                    "identity":profile.identity,"version":profile.version.to_string()
                })),
                _ => None
            }
        }),
        ClientFrame::Sync { cursors, all } => json!({
            "kind":"sync", "all":all,
            "cursors":cursors.into_iter().map(|cursor| json!({
                "space":cursor.space,"next_seq":cursor.next_seq.to_string()
            })).collect::<Vec<_>>()
        }),
        _ => crate::directory::socket::incoming(bytes)?,
    };
    encode(&value)
}

/// Limit the advertised public-composition surface to actual implemented features.
/// Reuse Directory's real certificate/protocol/challenge validation and native codec.
pub fn hello_json(bytes: &[u8], context: &str) -> Result<String, BoundaryError> {
    #[derive(Deserialize)]
    struct HelloResult {
        hello: crate::directory::Hello,
        challenge_base64: String,
    }
    let mut result: HelloResult = parse(&crate::directory::hello_json(bytes, context)?)?;
    result
        .hello
        .capabilities
        .retain(|cap| cap == "profiles" || cap == protocol::KEY_PACKAGE_CLAIM_CAPABILITY);
    let bytes = STANDARD
        .decode(&result.challenge_base64)
        .map_err(|_| internal("invalid Directory challenge bytes"))?;
    let mut challenge =
        ServerFrame::decode(&bytes).map_err(|_| internal("invalid Directory challenge"))?;
    let ServerFrame::Challenge { capabilities, .. } = &mut challenge else {
        return Err(internal("expected Directory challenge"));
    };
    *capabilities = result.hello.capabilities.clone();
    encode(&json!({"hello":result.hello,"challenge_base64":STANDARD.encode(challenge.encode())}))
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Control {
    Accepted {
        space: String,
        client_id: String,
        seq: String,
    },
    Rejected {
        space: String,
        client_id: String,
        reason: String,
        permanent: bool,
    },
    Joined {
        space: String,
    },
    SyncDone,
    KeyPackagesLow {
        device: String,
        remaining: u32,
    },
    ProfileChanged {
        identity: String,
        version: String,
    },
}

/// Actual native ServerFrame codec. JSON is internal metadata, never public wire.
pub fn control_json(value: &str) -> Result<String, BoundaryError> {
    let control: Control = parse(value)?;
    let frame = match control {
        Control::Accepted {
            space,
            client_id,
            seq,
        } => ServerFrame::Accepted {
            space,
            client_id,
            seq: parse_u64(&seq)?,
        },
        Control::Rejected {
            space,
            client_id,
            reason,
            permanent,
        } => ServerFrame::Rejected {
            space,
            client_id,
            reason,
            permanent,
        },
        Control::Joined { space } => ServerFrame::Joined { space },
        Control::SyncDone => ServerFrame::SyncDone,
        Control::KeyPackagesLow { device, remaining } => {
            ServerFrame::KeyPackagesLow { device, remaining }
        }
        Control::ProfileChanged { identity, version } => ServerFrame::ProfileChanged {
            identity,
            version: parse_u64(&version)?,
        },
    };
    let bytes = frame.encode();
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(internal("outgoing relay frame exceeds byte bound"));
    }
    encode(&json!({"frame_base64":STANDARD.encode(bytes)}))
}

/// Validate canonical stored Sequenced bytes and derive correlation fields in
/// Rust. The host cannot provide a different CID, author, Space or chain hash.
/// This does not establish membership: Space's authoritative read/dispatch does.
pub fn event_json(bytes: &[u8]) -> Result<String, BoundaryError> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(internal("invalid relay event size"));
    }
    let ev = Sequenced::decode(bytes).map_err(|_| internal("invalid stored relay event"))?;
    if ev.encode() != bytes {
        return Err(internal("stored relay event is not canonical"));
    }
    ev.env
        .verify()
        .map_err(|_| internal("stored relay event signature is invalid"))?;
    let hash = event_log::chain_hash(ev.env.space(), ev.seq, &ev.prev, &ev.env.wire_hash());
    if ev.hash != hash {
        return Err(internal("stored relay event chain hash is invalid"));
    }
    let wire = ServerFrame::Event { ev: ev.clone() }.encode();
    if wire.len() > MAX_FRAME_BYTES {
        return Err(internal("outgoing relay event exceeds byte bound"));
    }
    let value = json!({
        "space":ev.env.space(), "client_id":ev.env.client_id(), "author":ev.env.author(),
        "seq":ev.seq.to_string(), "prev":ev.prev, "hash":ev.hash, "device":ev.env.device(),
        "frame_base64":STANDARD.encode(wire)
    });
    encode(&value)
}

/// Derive successful request effects from the actual canonical native reply.
/// Public identity/device keys only; package data never enters host JSON metadata.
pub fn request_effects_json(bytes: &[u8]) -> Result<String, BoundaryError> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(internal("invalid request reply size"));
    }
    let reply = ServerFrame::decode(bytes).map_err(|_| internal("invalid request reply"))?;
    if reply.encode() != bytes {
        return Err(internal("request reply is not canonical"));
    }
    let ServerFrame::Res { result, .. } = reply else {
        return Err(internal("expected request reply"));
    };
    let ok = result.is_ok();
    let claimed = match result {
        Ok(Reply::KeyPackages(records)) => {
            if records.len() > crate::directory::key_packages::PRIVATE_MAX_RECORDS {
                return Err(internal("claim stock notification exceeds private bound"));
            }
            records
                .into_iter()
                .map(|record| json!({"identity":record.identity,"device":record.device}))
                .collect::<Vec<_>>()
        }
        _ => vec![],
    };
    encode(&json!({"ok":ok,"claimed":claimed}))
}

/// Same native GCRA defaults, arithmetic and refusal text. The host commits the
/// device charge BEFORE trying account quota, even when the latter refuses.
pub fn rate_json(value: &str) -> Result<String, BoundaryError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Rate {
        scope: String,
        now_ns: String,
        tat_ns: Option<String>,
    }
    let rate: Rate = parse(value)?;
    let (interval, burst) = match rate.scope.as_str() {
        "publish_device" => (1_000_000_000 / 20, 60),
        "publish_account" => (1_000_000_000 / 40, 120),
        "request_device" => (1_000_000_000 / 50, 200),
        _ => return Err(internal("unknown relay rate scope")),
    };
    let now = parse_u64(&rate.now_ns)?;
    let tat = rate.tat_ns.as_deref().map(parse_u64).transpose()?;
    let value: Value = match crate::gcra::decide(now, tat, 1, interval, burst) {
        crate::gcra::Decision::Accepted { next_ns } => {
            json!({"accepted":true,"next_ns":next_ns.to_string()})
        }
        crate::gcra::Decision::Rejected { wait_ns } => {
            json!({"accepted":false,"reason":protocol::slow_down(Duration::from_nanos(wait_ns))})
        }
    };
    encode(&value)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;
    fn error(value: crate::portable::BoundaryError) -> JsValue {
        JsValue::from_str(
            &serde_json::to_string(&value).unwrap_or_else(|_| "relay socket failure".into()),
        )
    }
    #[wasm_bindgen(js_name = relaySocketHello)]
    pub fn hello(bytes: &[u8], context: &str) -> Result<String, JsValue> {
        super::hello_json(bytes, context).map_err(error)
    }
    #[wasm_bindgen(js_name = relaySocketFrame)]
    pub fn incoming(bytes: &[u8]) -> Result<String, JsValue> {
        super::incoming_json(bytes).map_err(error)
    }
    #[wasm_bindgen(js_name = relaySocketControl)]
    pub fn control(value: &str) -> Result<String, JsValue> {
        super::control_json(value).map_err(error)
    }
    #[wasm_bindgen(js_name = relaySocketEvent)]
    pub fn event(bytes: &[u8]) -> Result<String, JsValue> {
        super::event_json(bytes).map_err(error)
    }
    #[wasm_bindgen(js_name = relaySocketRequestEffects)]
    pub fn effects(bytes: &[u8]) -> Result<String, JsValue> {
        super::request_effects_json(bytes).map_err(error)
    }
    #[wasm_bindgen(js_name = relaySocketRate)]
    pub fn rate(value: &str) -> Result<String, JsValue> {
        super::rate_json(value).map_err(error)
    }
}

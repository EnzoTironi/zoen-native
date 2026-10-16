use roda_proto::{ClientFrame, ErrorCode, Op, Reply, ServerFrame, CAPABILITIES, PROTOCOL_VERSION};
use serde::Serialize;

pub fn hello(identity: &str, device: &str, cert: &str) -> Vec<u8> {
    ClientFrame::Hello {
        protocol: PROTOCOL_VERSION,
        capabilities: CAPABILITIES.iter().map(|c| c.to_string()).collect(),
        identity: identity.to_string(),
        device: device.to_string(),
        cert: cert.to_string(),
    }
    .encode()
}

pub fn request(id: u64, kind: &str, value: &str) -> Result<Vec<u8>, String> {
    let op = match kind {
        "fetch_link" => Op::FetchLink {
            id: value.to_string(),
        },
        "devices" => Op::Devices,
        "unlink" => Op::Unlink {
            device: roda_log::parse_device_key(value)
                .map(|_| value.to_string())
                .ok_or("invalid device public key")?,
        },
        "preview_invite" => Op::PreviewInvite {
            code: value.to_string(),
        },
        "profiles" => Op::Profiles {
            ids: serde_json::from_str(value).map_err(|e| e.to_string())?,
        },
        _ => return Err("unsupported browser request".into()),
    };
    Ok(ClientFrame::Req { id, op }.encode())
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerMessage {
    Challenge {
        nonce: String,
        relay: String,
        protocol: u32,
    },
    Ready {
        identity: String,
        registered: bool,
    },
    Response {
        id: u64,
        value: serde_json::Value,
    },
    RequestError {
        id: u64,
        message: String,
    },
    Event,
    SyncDone,
    Joined {
        space: String,
    },
    Accepted {
        space: String,
        client_id: String,
        seq: u64,
    },
    Rejected {
        space: String,
        client_id: String,
        reason: String,
        permanent: bool,
    },
    KeyPackagesLow {
        device: String,
        remaining: u32,
    },
    Error {
        code: &'static str,
        message: String,
    },
    Other,
}

pub fn decode_server(bytes: &[u8]) -> Result<ServerMessage, String> {
    let frame = ServerFrame::decode(bytes).map_err(|e| e.to_string())?;
    Ok(match frame {
        ServerFrame::Challenge {
            nonce,
            relay,
            protocol,
            ..
        } => ServerMessage::Challenge {
            nonce,
            relay,
            protocol,
        },
        ServerFrame::Ready {
            identity,
            registered,
        } => ServerMessage::Ready {
            identity,
            registered,
        },
        ServerFrame::Res {
            id,
            result: Err(message),
        } => ServerMessage::RequestError { id, message },
        ServerFrame::Res {
            id,
            result: Ok(reply),
        } => ServerMessage::Response {
            id,
            value: match reply {
                Reply::Link(sealed) => serde_json::json!({"kind":"link","sealed":sealed}),
                Reply::AgentState(a) => {
                    serde_json::json!({"kind":"agent_state","status":a.status,"definition_version":a.definition_version})
                }
                Reply::Devices(devices) => {
                    serde_json::json!({"kind":"devices","devices":devices.iter().map(|d| serde_json::json!({"device":d.device,"revoked":d.revoked})).collect::<Vec<_>>()})
                }
                Reply::Profiles(profiles) => {
                    serde_json::json!({"kind":"profiles","profiles":profiles})
                }
                Reply::Preview(p) => {
                    serde_json::json!({"kind":"preview","space":p.space,"role":p.role,"title":p.title,"members":p.members})
                }
                Reply::KeyPackages(records) => {
                    serde_json::json!({"kind":"key_packages","records":records})
                }
                Reply::Done => serde_json::json!({"kind":"done"}),
                _ => serde_json::json!({"kind":"other"}),
            },
        },
        ServerFrame::Event { .. } => ServerMessage::Event,
        ServerFrame::SyncDone => ServerMessage::SyncDone,
        ServerFrame::Joined { space } => ServerMessage::Joined { space },
        ServerFrame::Accepted {
            space,
            client_id,
            seq,
        } => ServerMessage::Accepted {
            space,
            client_id,
            seq,
        },
        ServerFrame::Rejected {
            space,
            client_id,
            reason,
            permanent,
        } => ServerMessage::Rejected {
            space,
            client_id,
            reason,
            permanent,
        },
        ServerFrame::KeyPackagesLow { device, remaining } => {
            ServerMessage::KeyPackagesLow { device, remaining }
        }
        ServerFrame::Error { code, message } => ServerMessage::Error {
            code: match code {
                ErrorCode::Unauthorized => "unauthorized",
                ErrorCode::UpgradeRequired => "upgrade_required",
                ErrorCode::RateLimited => "rate_limited",
                ErrorCode::Unavailable => "unavailable",
                ErrorCode::Other => "other",
            },
            message,
        },
        _ => ServerMessage::Other,
    })
}

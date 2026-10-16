use roda_proto::{
    ClientFrame, EphemeralKind, ErrorCode, Op, Reply, ServerFrame, CAPABILITIES, PROTOCOL_VERSION,
};
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
    match kind {
        "typing" | "stopped_typing" => {
            return Ok(ClientFrame::Ephemeral {
                space: value.to_string(),
                kind: if kind == "typing" {
                    EphemeralKind::Typing
                } else {
                    EphemeralKind::StoppedTyping
                },
            }
            .encode())
        }
        "presence_privacy" => {
            return Ok(ClientFrame::SetPresencePrivacy {
                hide_last_seen: serde_json::from_str(value).map_err(|e| e.to_string())?,
            }
            .encode())
        }
        _ => {}
    }
    let op = match kind {
        "fetch_link" => Op::FetchLink {
            id: value.to_string(),
        },
        "devices" => Op::Devices,
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
    Presence {
        identity: String,
        online: bool,
        last_seen_ms: Option<u64>,
        hide_last_seen: Option<bool>,
    },
    Ephemeral {
        space: String,
        from: String,
        activity: &'static str,
    },
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
        ServerFrame::Presence {
            identity,
            online,
            last_seen_ms,
            hide_last_seen,
        } => ServerMessage::Presence {
            identity,
            online,
            last_seen_ms,
            hide_last_seen,
        },
        ServerFrame::Ephemeral { space, from, kind } => ServerMessage::Ephemeral {
            space,
            from,
            activity: match kind {
                EphemeralKind::Typing => "typing",
                EphemeralKind::StoppedTyping => "stopped_typing",
                _ => "other",
            },
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_and_typing_are_not_lost_at_the_browser_boundary() {
        let frame = ServerFrame::Presence {
            identity: "person".into(),
            online: false,
            last_seen_ms: Some(123),
            hide_last_seen: Some(true),
        };
        let json = serde_json::to_value(decode_server(&frame.encode()).unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind":"presence", "identity":"person", "online":false, "last_seen_ms":123, "hide_last_seen":true})
        );
        let frame = ServerFrame::Ephemeral {
            space: "group".into(),
            from: "person".into(),
            kind: EphemeralKind::Typing,
        };
        let json = serde_json::to_value(decode_server(&frame.encode()).unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind":"ephemeral", "space":"group", "from":"person", "activity":"typing"})
        );
        let frame = ServerFrame::Ephemeral {
            space: "group".into(),
            from: "person".into(),
            kind: EphemeralKind::Read { seq: 2 },
        };
        let json = serde_json::to_value(decode_server(&frame.encode()).unwrap()).unwrap();
        assert_eq!(json["activity"], "other");
    }

    #[test]
    fn transient_browser_commands_encode_native_frames() {
        assert_eq!(
            ClientFrame::decode(&request(0, "typing", "group").unwrap()).unwrap(),
            ClientFrame::Ephemeral {
                space: "group".into(),
                kind: EphemeralKind::Typing
            }
        );
        assert_eq!(
            ClientFrame::decode(&request(0, "stopped_typing", "group").unwrap()).unwrap(),
            ClientFrame::Ephemeral {
                space: "group".into(),
                kind: EphemeralKind::StoppedTyping
            }
        );
        assert_eq!(
            ClientFrame::decode(&request(0, "presence_privacy", "true").unwrap()).unwrap(),
            ClientFrame::SetPresencePrivacy {
                hide_last_seen: true
            }
        );
        assert!(request(0, "presence_privacy", "not a bool").is_err());
    }
}

use protocol::{ClientFrame, ErrorCode, Op, Reply, ServerFrame, CAPABILITIES, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use types::Identity;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateInvite {
    space: String,
    agent: Option<String>,
    role: types::Role,
    max_uses: u32,
    ttl_secs: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlAgent {
    agent: String,
    action: String,
}

fn identity(value: &str) -> Result<String, String> {
    event_log::parse_device_key(value)
        .map(|_| value.to_owned())
        .ok_or_else(|| "invalid identity public key".into())
}

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
        "register" => {
            let profile: Identity =
                serde_json::from_str(value).map_err(|error| error.to_string())?;
            if profile.kind != types::IdentityKind::Person
                || !profile.name.is_empty()
                || !profile.bio.is_empty()
                || profile.owner.is_some()
                || profile.owner_proof.is_some()
                || protocol::normalize_handle(&profile.handle).as_deref()
                    != Some(profile.handle.as_str())
            {
                return Err("invalid public profile".into());
            }
            identity(&profile.id)?;
            Op::Register { profile }
        }
        "fetch_link" => Op::FetchLink {
            id: value.to_string(),
        },
        "devices" => Op::Devices,
        "unlink" => Op::Unlink {
            device: event_log::parse_device_key(value)
                .map(|_| value.to_string())
                .ok_or("invalid device public key")?,
        },
        "preview_invite" => Op::PreviewInvite {
            code: value.to_string(),
        },
        "profiles" => Op::Profiles {
            ids: serde_json::from_str(value).map_err(|e| e.to_string())?,
        },
        "lookup" => Op::Lookup {
            handle: value.trim().trim_start_matches('@').to_lowercase(),
            prefix: true,
        },
        "create_invite" => {
            let args: CreateInvite =
                serde_json::from_str(value).map_err(|error| error.to_string())?;
            if args.max_uses == 0
                || args.max_uses > 10_000
                || args.ttl_secs == 0
                || args.ttl_secs > 365 * 24 * 3600
            {
                return Err("invalid invite limits".into());
            }
            if let Some(agent) = &args.agent {
                identity(agent)?;
            }
            Op::CreateInvite {
                space: args.space,
                agent: args.agent,
                role: args.role,
                max_uses: args.max_uses,
                ttl_secs: args.ttl_secs,
            }
        }
        "redeem_agent_invite" => Op::RedeemAgentInvite { code: value.into() },
        "revoke_agent_invite" => Op::RevokeAgentInvite {
            agent: identity(value)?,
        },
        "agent_status" => Op::Agent(protocol::AgentRequest::Status {
            agent: identity(value)?,
        }),
        "control_agent" => {
            let args: ControlAgent =
                serde_json::from_str(value).map_err(|error| error.to_string())?;
            Op::Agent(protocol::AgentRequest::Control {
                agent: identity(&args.agent)?,
                action: protocol::AgentAction::parse(&args.action)
                    .ok_or("invalid agent control")?,
            })
        }
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
        capabilities: Vec<String>,
        server_time_ms: Option<String>,
    },
    Pong {
        server_time_ms: Option<String>,
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
        seq: String,
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
    Ephemeral {
        space: String,
        from: String,
        event: &'static str,
        detail: String,
    },
    Presence {
        identity: String,
        online: bool,
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
            capabilities,
            server_time_ms,
        } => ServerMessage::Challenge {
            nonce,
            relay,
            protocol,
            capabilities,
            server_time_ms: server_time_ms.map(|time| time.to_string()),
        },
        ServerFrame::Pong { server_time_ms } => ServerMessage::Pong {
            server_time_ms: server_time_ms.map(|time| time.to_string()),
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
                Reply::Registered(profile) => {
                    serde_json::json!({"kind":"registered","profile":profile})
                }
                Reply::Link(sealed) => serde_json::json!({"kind":"link","sealed":sealed}),
                Reply::AgentState(a) => {
                    serde_json::json!({"kind":"agent_state","status":a.status,"definition_version":a.definition_version})
                }
                Reply::CompletePage(_) => serde_json::json!({"kind":"complete_page"}),
                Reply::Devices(devices) => {
                    serde_json::json!({"kind":"devices","devices":devices.iter().map(|d| serde_json::json!({"device":d.device,"revoked":d.revoked})).collect::<Vec<_>>()})
                }
                Reply::Profiles(profiles) => {
                    serde_json::json!({"kind":"profiles","profiles":profiles})
                }
                Reply::Preview(p) => {
                    serde_json::json!({"kind":"preview","space":p.space,"role":p.role,"title":p.title,"members":p.members,"invite_kind":p.kind,"agent":p.agent,"inviter":p.inviter})
                }
                Reply::Invite(invite) => {
                    serde_json::json!({"kind":"invite","code":invite.code,"expires_at_ms":invite.expires_at_ms.to_string()})
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
            seq: seq.to_string(),
        },
        ServerFrame::Ephemeral { space, from, kind } => {
            let (event, detail) = match kind {
                protocol::EphemeralKind::Typing => ("typing", String::new()),
                protocol::EphemeralKind::StoppedTyping => ("stopped", String::new()),
                protocol::EphemeralKind::Status { status } => ("status", status),
                protocol::EphemeralKind::Read { seq } => ("read", seq.to_string()),
                protocol::EphemeralKind::Delivered { seq } => ("delivered", seq.to_string()),
            };
            ServerMessage::Ephemeral {
                space,
                from,
                event,
                detail,
            }
        }
        ServerFrame::Presence { identity, online } => ServerMessage::Presence { identity, online },
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
    fn browser_unlink_uses_the_existing_authenticated_device_operation() {
        let device = event_log::Signer::generate().id();
        let frame = ClientFrame::decode(&request(42, "unlink", &device).unwrap()).unwrap();
        assert!(
            matches!(frame, ClientFrame::Req { id: 42, op: Op::Unlink { device: target } } if target == device)
        );
        assert!(request(43, "unlink", "not-a-device").is_err());
    }
}

#[cfg(test)]
mod relay_tests {
    use super::*;
    #[test]
    fn owner_controls_and_invites_use_existing_authenticated_protocol_and_exact_times() {
        let agent = event_log::Signer::generate().id();
        let request_value = serde_json::json!({"agent":agent,"action":"pause"}).to_string();
        assert!(matches!(
            ClientFrame::decode(&request(45, "control_agent", &request_value).unwrap()).unwrap(),
            ClientFrame::Req {
                op: Op::Agent(protocol::AgentRequest::Control {
                    action: protocol::AgentAction::Pause,
                    ..
                }),
                ..
            }
        ));
        assert!(request(
            46,
            "control_agent",
            &serde_json::json!({"agent":agent,"action":"erase_all"}).to_string()
        )
        .is_err());
        let invite = ServerFrame::Res {
            id: 47,
            result: Ok(Reply::Invite(protocol::InviteCreated {
                code: "0123456789".into(),
                expires_at_ms: i64::MAX,
            })),
        };
        let json = serde_json::to_value(decode_server(&invite.encode()).unwrap()).unwrap();
        assert_eq!(json["value"]["expires_at_ms"], i64::MAX.to_string());
        let accepted = ServerFrame::Accepted {
            space: "sp_fixture".into(),
            client_id: "client".into(),
            seq: u64::MAX,
        };
        let json = serde_json::to_value(decode_server(&accepted.encode()).unwrap()).unwrap();
        assert_eq!(json["seq"], u64::MAX.to_string());
        let read = ServerFrame::Ephemeral {
            space: "sp_fixture".into(),
            from: agent,
            kind: protocol::EphemeralKind::Read { seq: u64::MAX },
        };
        let json = serde_json::to_value(decode_server(&read.encode()).unwrap()).unwrap();
        assert_eq!(json["detail"], u64::MAX.to_string());
    }
}

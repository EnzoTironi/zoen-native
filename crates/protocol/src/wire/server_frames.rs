use super::*;

impl ServerFrame {
    pub fn encode(&self) -> Vec<u8> {
        use pb_server_frame::F;
        let f = match self {
            ServerFrame::Challenge {
                nonce,
                relay,
                protocol,
                capabilities,
                server_time_ms,
            } => F::Challenge(PbChallenge {
                nonce: nonce.clone(),
                relay: relay.clone(),
                protocol: *protocol,
                capabilities: capabilities.clone(),
                server_time_ms: *server_time_ms,
            }),
            ServerFrame::Ready {
                identity,
                registered,
            } => F::Ready(PbReady {
                identity: identity.clone(),
                registered: *registered,
            }),
            ServerFrame::Res { id, result } => F::Res(PbRes {
                id: *id,
                r: Some(match result {
                    Err(e) => pb_res::R::Err(e.clone()),
                    Ok(Reply::Registered(p)) => pb_res::R::Registered(profile_to(p)),
                    Ok(Reply::Profiles(ps)) => pb_res::R::Profiles(PbProfiles {
                        profiles: ps.iter().map(profile_to).collect(),
                    }),
                    Ok(Reply::Invite(i)) => pb_res::R::Invite(PbInviteCreated {
                        code: i.code.clone(),
                        expires_at_ms: i.expires_at_ms,
                    }),
                    Ok(Reply::Preview(p)) => pb_res::R::Preview(PbInvitePreview {
                        space: p.space.clone(),
                        role: role_to(p.role),
                        title: p.title.clone(),
                        members: p.members,
                        inviter: p.inviter.as_ref().map(profile_to),
                        kind: p.kind.clone(),
                        agent: p.agent.as_deref().map(profile_to).map(Box::new),
                    }),
                    Ok(Reply::Done) => pb_res::R::Done(PbEmpty {}),
                    Ok(Reply::RuntimeBinding(p)) => {
                        pb_res::R::RuntimeBinding(PbRuntimeBindingProof {
                            cell: p.cell.clone(),
                            tag: p.tag.clone(),
                        })
                    }
                    Ok(Reply::CallTurn(c)) => pb_res::R::CallTurn(PbCallTurnReply {
                        urls: c.urls.clone(),
                        username: c.username.clone(),
                        credential: c.credential.clone(),
                        expires_at_ms: c.expires_at_ms,
                    }),
                    Ok(Reply::AgreementKeys(ks)) => pb_res::R::AgreementKeys(PbAgreementKeys {
                        keys: ks.iter().map(agreement_to).collect(),
                    }),
                    Ok(Reply::SealedProfiles(ps)) => pb_res::R::SealedProfiles(PbSealedProfiles {
                        profiles: ps.iter().map(sealed_profile_to).collect(),
                    }),
                    Ok(Reply::KeyPackages(ks)) => pb_res::R::KeyPackages(PbKeyPackages {
                        packages: ks
                            .iter()
                            .map(|k| PbKeyPackage {
                                identity: k.identity.clone(),
                                device: k.device.clone(),
                                data: k.data.clone(),
                            })
                            .collect(),
                    }),
                    Ok(Reply::Link(sealed)) => pb_res::R::Link(PbLinkBox {
                        id: String::new(),
                        sealed: sealed.clone().unwrap_or_default(),
                        device: None,
                    }),
                    Ok(Reply::AgentState(a)) => pb_res::R::AgentState(PbAgentState {
                        status: a.status.clone(),
                        kind: a.kind.clone(),
                        definition_version: a.definition_version,
                        definition: a.definition.clone(),
                        cursors: a
                            .cursors
                            .iter()
                            .map(|(space, seq)| PbAgentCursor {
                                space: space.clone(),
                                seq: *seq,
                            })
                            .collect(),
                    }),
                    Ok(Reply::Devices(ds)) => pb_res::R::Devices(PbDevices {
                        devices: ds
                            .iter()
                            .map(|d| PbDevice {
                                device: d.device.clone(),
                                revoked: d.revoked,
                            })
                            .collect(),
                    }),
                }),
            }),
            ServerFrame::Event { ev } => F::Event(PbSequenced {
                seq: ev.seq,
                prev: ev.prev.clone(),
                hash: ev.hash.clone(),
                env: Some(envelope_to(&ev.env)),
            }),
            ServerFrame::Accepted {
                space,
                client_id,
                seq,
            } => F::Accepted(PbAccepted {
                space: space.clone(),
                client_id: client_id.clone(),
                seq: *seq,
            }),
            ServerFrame::Rejected {
                space,
                client_id,
                reason,
                permanent,
            } => F::Rejected(PbRejected {
                space: space.clone(),
                client_id: client_id.clone(),
                reason: reason.clone(),
                permanent: *permanent,
            }),
            ServerFrame::Ephemeral { space, from, kind } => {
                F::Ephemeral(ephemeral_to(space, from, kind))
            }
            ServerFrame::Presence { identity, online } => F::Presence(PbPresence {
                identity: identity.clone(),
                online: *online,
            }),
            ServerFrame::ProfileChanged { identity, version } => {
                F::ProfileChanged(PbProfileChanged {
                    identity: identity.clone(),
                    version: *version,
                })
            }
            ServerFrame::KeyPackagesLow { device, remaining } => {
                F::KeyPackagesLow(PbKeyPackagesLow {
                    device: device.clone(),
                    remaining: *remaining,
                })
            }
            ServerFrame::DeviceMessage { from, to, sealed } => F::DeviceMessage(PbDeviceMessage {
                from: from.clone(),
                to: to.clone(),
                sealed: sealed.clone(),
            }),
            ServerFrame::Joined { space } => F::Joined(space.clone()),
            ServerFrame::SyncDone => F::SyncDone(PbEmpty {}),
            ServerFrame::Pong { server_time_ms } => F::Pong(PbClock {
                server_time_ms: *server_time_ms,
            }),
            ServerFrame::Error { code, message } => F::Error(PbError {
                code: match code {
                    ErrorCode::Other => PbErrorCode::Other,
                    ErrorCode::UpgradeRequired => PbErrorCode::UpgradeRequired,
                    ErrorCode::Unauthorized => PbErrorCode::Unauthorized,
                    ErrorCode::Unavailable => PbErrorCode::Unavailable,
                    ErrorCode::RateLimited => PbErrorCode::RateLimited,
                } as i32,
                message: message.clone(),
            }),
        };
        PbServerFrame { f: Some(f) }.encode_to_vec()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        use pb_server_frame::F;
        let pb = PbServerFrame::decode(bytes).map_err(|e| DecodeError(e.to_string()))?;
        Ok(match pb.f.ok_or_else(|| bad("empty server frame"))? {
            F::Challenge(c) => ServerFrame::Challenge {
                nonce: c.nonce,
                relay: c.relay,
                protocol: c.protocol,
                capabilities: c.capabilities,
                server_time_ms: c.server_time_ms,
            },
            F::Ready(r) => ServerFrame::Ready {
                identity: r.identity,
                registered: r.registered,
            },
            F::Res(r) => ServerFrame::Res {
                id: r.id,
                result: match r.r.ok_or_else(|| bad("empty result"))? {
                    pb_res::R::Err(e) => Err(e),
                    pb_res::R::Registered(p) => Ok(Reply::Registered(profile_from(p)?)),
                    pb_res::R::Profiles(ps) => Ok(Reply::Profiles(
                        ps.profiles
                            .into_iter()
                            .map(profile_from)
                            .collect::<Result<_, _>>()?,
                    )),
                    pb_res::R::Invite(i) => Ok(Reply::Invite(InviteCreated {
                        code: i.code,
                        expires_at_ms: i.expires_at_ms,
                    })),
                    pb_res::R::Preview(p) => Ok(Reply::Preview(InvitePreview {
                        space: p.space,
                        role: role_from(p.role)?,
                        title: p.title,
                        members: p.members,
                        inviter: p.inviter.map(profile_from).transpose()?,
                        kind: if p.kind.is_empty() {
                            "space".into()
                        } else {
                            p.kind
                        },
                        agent: p.agent.map(|a| profile_from(*a)).transpose()?.map(Box::new),
                    })),
                    pb_res::R::Done(_) => Ok(Reply::Done),
                    pb_res::R::RuntimeBinding(p) => {
                        if !crate::runtime_binding::valid_namespace(&p.cell) || p.tag.len() != 32 {
                            return Err(bad("runtime binding proof"));
                        }
                        Ok(Reply::RuntimeBinding(crate::runtime_binding::Proof {
                            cell: p.cell,
                            tag: p.tag,
                        }))
                    }
                    pb_res::R::CallTurn(c) => {
                        let credentials = crate::turn::TurnCredentials {
                            urls: c.urls,
                            username: c.username,
                            credential: c.credential,
                            expires_at_ms: c.expires_at_ms,
                        };
                        if !credentials.valid() {
                            return Err(bad("invalid TURN credentials"));
                        }
                        Ok(Reply::CallTurn(credentials))
                    }
                    pb_res::R::AgreementKeys(ks) => Ok(Reply::AgreementKeys(
                        ks.keys
                            .into_iter()
                            .map(agreement_from)
                            .collect::<Result<_, _>>()?,
                    )),
                    pb_res::R::SealedProfiles(ps) => Ok(Reply::SealedProfiles(
                        ps.profiles
                            .into_iter()
                            .map(sealed_profile_from)
                            .collect::<Result<_, _>>()?,
                    )),
                    pb_res::R::KeyPackages(ks) => Ok(Reply::KeyPackages(
                        ks.packages
                            .into_iter()
                            .map(|k| KeyPackageRecord {
                                identity: k.identity,
                                device: k.device,
                                data: k.data,
                            })
                            .collect(),
                    )),
                    pb_res::R::Link(b) => {
                        Ok(Reply::Link((!b.sealed.is_empty()).then_some(b.sealed)))
                    }
                    pb_res::R::AgentState(a) => Ok(Reply::AgentState(AgentStateRecord {
                        status: a.status,
                        kind: a.kind,
                        definition_version: a.definition_version,
                        definition: a.definition,
                        cursors: a.cursors.into_iter().map(|c| (c.space, c.seq)).collect(),
                    })),
                    pb_res::R::Devices(ds) => Ok(Reply::Devices(
                        ds.devices
                            .into_iter()
                            .map(|d| DeviceRecord {
                                device: d.device,
                                revoked: d.revoked,
                            })
                            .collect(),
                    )),
                },
            },
            F::Event(s) => ServerFrame::Event {
                ev: Sequenced {
                    seq: s.seq,
                    prev: s.prev,
                    hash: s.hash,
                    env: envelope_from(s.env.ok_or_else(|| bad("event without envelope"))?)?,
                },
            },
            F::Accepted(a) => ServerFrame::Accepted {
                space: a.space,
                client_id: a.client_id,
                seq: a.seq,
            },
            F::Rejected(r) => ServerFrame::Rejected {
                space: r.space,
                client_id: r.client_id,
                reason: r.reason,
                permanent: r.permanent,
            },
            F::Ephemeral(e) => ServerFrame::Ephemeral {
                space: e.space,
                from: e.from,
                kind: ephemeral_kind(e.kind)?,
            },
            F::Presence(p) => ServerFrame::Presence {
                identity: p.identity,
                online: p.online,
            },
            F::ProfileChanged(p) => ServerFrame::ProfileChanged {
                identity: p.identity,
                version: p.version,
            },
            F::KeyPackagesLow(k) => ServerFrame::KeyPackagesLow {
                device: k.device,
                remaining: k.remaining,
            },
            F::DeviceMessage(m) => ServerFrame::DeviceMessage {
                from: m.from,
                to: m.to,
                sealed: m.sealed,
            },
            F::Joined(space) => ServerFrame::Joined { space },
            F::SyncDone(_) => ServerFrame::SyncDone,
            F::Pong(c) => ServerFrame::Pong {
                server_time_ms: c.server_time_ms,
            },
            F::Error(e) => ServerFrame::Error {
                code: match PbErrorCode::try_from(e.code).unwrap_or(PbErrorCode::Other) {
                    PbErrorCode::Other => ErrorCode::Other,
                    PbErrorCode::UpgradeRequired => ErrorCode::UpgradeRequired,
                    PbErrorCode::Unauthorized => ErrorCode::Unauthorized,
                    PbErrorCode::Unavailable => ErrorCode::Unavailable,
                    PbErrorCode::RateLimited => ErrorCode::RateLimited,
                },
                message: e.message,
            },
        })
    }
}

use super::*;

impl ClientFrame {
    pub fn encode(&self) -> Vec<u8> {
        use pb_client_frame::F;
        let f = match self {
            ClientFrame::Hello {
                protocol,
                capabilities,
                identity,
                device,
                cert,
            } => F::Hello(PbHello {
                protocol: *protocol,
                capabilities: capabilities.clone(),
                identity: identity.clone(),
                device: device.clone(),
                cert: cert.clone(),
            }),
            ClientFrame::Auth { sig } => F::Auth(sig.clone()),
            ClientFrame::Req { id, op } => F::Req(PbReq {
                id: *id,
                op: Some(match op {
                    Op::Register { profile } => pb_req::Op::Register(profile_to(profile)),
                    Op::Lookup { handle, prefix } => pb_req::Op::Lookup(PbLookup {
                        handle: handle.clone(),
                        prefix: *prefix,
                    }),
                    Op::Profiles { ids } => pb_req::Op::Profiles(PbIds { ids: ids.clone() }),
                    Op::CreateInvite {
                        space,
                        agent,
                        role,
                        max_uses,
                        ttl_secs,
                    } => pb_req::Op::CreateInvite(PbCreateInvite {
                        space: space.clone(),
                        role: role_to(*role),
                        max_uses: *max_uses,
                        ttl_secs: *ttl_secs,
                        agent: agent.clone(),
                    }),
                    Op::PreviewInvite { code } => pb_req::Op::PreviewInvite(code.clone()),
                    Op::RedeemAgentInvite { code } => pb_req::Op::RedeemAgentInvite(code.clone()),
                    Op::RevokeAgentInvite { agent } => pb_req::Op::RevokeAgentInvite(agent.clone()),
                    Op::RegisterPush { platform, token } => pb_req::Op::RegisterPush(PbPushToken {
                        platform: platform.clone(),
                        token: token.clone(),
                    }),
                    Op::PublishAgreementKey { public, signed } => {
                        pb_req::Op::PublishAgreementKey(PbAgreementKey {
                            identity: String::new(),
                            public: public.clone(),
                            signed: signed_to(signed),
                        })
                    }
                    Op::AgreementKeys { ids } => {
                        pb_req::Op::AgreementKeys(PbIds { ids: ids.clone() })
                    }
                    Op::PutProfile { profile } => {
                        pb_req::Op::PutProfile(sealed_profile_to(profile))
                    }
                    Op::GetProfiles { ids } => pb_req::Op::GetProfiles(PbIds { ids: ids.clone() }),
                    Op::PublishKeyPackages {
                        packages,
                        last_resort,
                    } => pb_req::Op::PublishKeyPackages(PbPublishKeyPackages {
                        packages: packages.clone(),
                        last_resort: last_resort.clone(),
                    }),
                    Op::ClaimKeyPackages { ids, operation_id } => {
                        pb_req::Op::ClaimKeyPackages(PbClaimKeyPackages {
                            ids: ids.clone(),
                            operation_id: operation_id.clone(),
                        })
                    }
                    Op::DeliverLink { id, sealed, device } => pb_req::Op::DeliverLink(PbLinkBox {
                        id: id.clone(),
                        sealed: sealed.clone(),
                        device: device.as_ref().map(|d| PbDeviceCertificate {
                            device: d.device.clone(),
                            cert: d.cert.clone(),
                        }),
                    }),
                    Op::FetchLink { id } => pb_req::Op::FetchLink(id.clone()),
                    Op::Devices => pb_req::Op::Devices(PbEmpty {}),
                    Op::Unlink { device } => pb_req::Op::Unlink(device.clone()),
                    Op::SendDevice { to, sealed } => pb_req::Op::SendDevice(PbDeviceMessage {
                        from: String::new(),
                        to: to.clone(),
                        sealed: sealed.clone(),
                    }),
                    Op::Agent(a) => pb_req::Op::Agent(agent_req_to(a)),
                    Op::CallTurn { space, peer } => pb_req::Op::CallTurn(PbCallTurnRequest {
                        space: space.clone(),
                        peer: peer.clone(),
                    }),
                }),
            }),
            ClientFrame::Publish { env } => F::Publish(envelope_to(env)),
            ClientFrame::Sync { cursors, all } => F::Sync(PbSync {
                cursors: cursors
                    .iter()
                    .map(|c| PbCursor {
                        space: c.space.clone(),
                        next_seq: c.next_seq,
                    })
                    .collect(),
                all: *all,
            }),
            ClientFrame::Ephemeral { space, kind } => F::Ephemeral(ephemeral_to(space, "", kind)),
            ClientFrame::Ping => F::Ping(PbEmpty {}),
        };
        PbClientFrame { f: Some(f) }.encode_to_vec()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        use pb_client_frame::F;
        let pb = PbClientFrame::decode(bytes).map_err(|e| DecodeError(e.to_string()))?;
        Ok(match pb.f.ok_or_else(|| bad("empty client frame"))? {
            F::Hello(h) => ClientFrame::Hello {
                protocol: h.protocol,
                capabilities: h.capabilities,
                identity: h.identity,
                device: h.device,
                cert: h.cert,
            },
            F::Auth(sig) => ClientFrame::Auth { sig },
            F::Req(r) => ClientFrame::Req {
                id: r.id,
                op: match r.op.ok_or_else(|| bad("empty request"))? {
                    pb_req::Op::Register(p) => Op::Register {
                        profile: profile_from(p)?,
                    },
                    pb_req::Op::Lookup(l) => Op::Lookup {
                        handle: l.handle,
                        prefix: l.prefix,
                    },
                    pb_req::Op::Profiles(p) => Op::Profiles { ids: p.ids },
                    pb_req::Op::CreateInvite(c) => Op::CreateInvite {
                        space: c.space,
                        agent: c.agent.filter(|a| !a.is_empty()),
                        role: role_from(c.role)?,
                        max_uses: c.max_uses,
                        ttl_secs: c.ttl_secs,
                    },
                    pb_req::Op::PreviewInvite(code) => Op::PreviewInvite { code },
                    pb_req::Op::RedeemAgentInvite(code) => Op::RedeemAgentInvite { code },
                    pb_req::Op::RevokeAgentInvite(agent) => Op::RevokeAgentInvite { agent },
                    pb_req::Op::RegisterPush(p) => Op::RegisterPush {
                        platform: p.platform,
                        token: p.token,
                    },
                    pb_req::Op::PublishAgreementKey(k) => Op::PublishAgreementKey {
                        public: k.public,
                        signed: signed_from(k.signed)?,
                    },
                    pb_req::Op::AgreementKeys(p) => Op::AgreementKeys { ids: p.ids },
                    pb_req::Op::PutProfile(p) => Op::PutProfile {
                        profile: sealed_profile_from(p)?,
                    },
                    pb_req::Op::GetProfiles(p) => Op::GetProfiles { ids: p.ids },
                    pb_req::Op::PublishKeyPackages(p) => Op::PublishKeyPackages {
                        packages: p.packages,
                        last_resort: p.last_resort,
                    },
                    pb_req::Op::ClaimKeyPackages(p) => Op::ClaimKeyPackages {
                        ids: p.ids,
                        operation_id: p.operation_id,
                    },
                    pb_req::Op::DeliverLink(b) => Op::DeliverLink {
                        id: b.id,
                        sealed: b.sealed,
                        device: b.device.map(|d| DeviceCertificate {
                            device: d.device,
                            cert: d.cert,
                        }),
                    },
                    pb_req::Op::FetchLink(id) => Op::FetchLink { id },
                    pb_req::Op::Devices(_) => Op::Devices,
                    pb_req::Op::Unlink(device) => Op::Unlink { device },
                    pb_req::Op::SendDevice(m) => Op::SendDevice {
                        to: m.to,
                        sealed: m.sealed,
                    },
                    pb_req::Op::Agent(a) => Op::Agent(agent_req_from(a)?),
                    pb_req::Op::CallTurn(r) => Op::CallTurn {
                        space: r.space,
                        peer: r.peer,
                    },
                },
            },
            // Only the relay prunes: a device can't hand it a stub.
            F::Publish(e) => {
                let env = envelope_from(e)?;
                if env.is_pruned() {
                    return Err(bad("a pruned envelope"));
                }
                ClientFrame::Publish { env }
            }
            F::Sync(s) => ClientFrame::Sync {
                cursors: s
                    .cursors
                    .into_iter()
                    .map(|c| Cursor {
                        space: c.space,
                        next_seq: c.next_seq,
                    })
                    .collect(),
                all: s.all,
            },
            F::Ephemeral(e) => ClientFrame::Ephemeral {
                space: e.space,
                kind: ephemeral_kind(e.kind)?,
            },
            F::Ping(_) => ClientFrame::Ping,
        })
    }
}

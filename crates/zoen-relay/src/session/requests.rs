use super::*;

impl Session {
    /// Agent citizens: the runtime's own state and cursors, or an owner's control.
    pub(super) async fn agent(&self, req: AgentRequest) -> Result<Reply, String> {
        use crate::agents;
        let pool = &self.st.pool;
        let me = &self.identity;
        let r = match req {
            AgentRequest::State => {
                return self
                    .complete_legacy(protocol::complete_reads::Kind::AgentState)
                    .await;
            }
            AgentRequest::Advance { space, seq } => {
                if !matches!(self.st.log.role(&space, me).await, Ok(Some(_))) {
                    return Err(agents::AgentCitizenError::NotMember.to_string());
                }
                agents::advance_cursor(pool, me, &self.device, &space, seq).await
            }
            AgentRequest::Define {
                agent,
                version,
                kind,
                definition,
            } => {
                if definition.len() > 64 * 1024 {
                    return Err("definition too large".into());
                }
                agents::define(pool, me, &self.device, &agent, version, &kind, &definition).await
            }
            AgentRequest::Status { agent } => {
                return agents::owner_status(pool, me, &agent)
                    .await
                    .map(Reply::AgentState)
                    .map_err(|e| e.to_string());
            }
            AgentRequest::ControlStatus { agent } => {
                return agents::control_status(pool, me, &agent)
                    .await
                    .map(Reply::AgentControl)
                    .map_err(|e| e.to_string());
            }
            AgentRequest::ControlReviewed {
                agent,
                action,
                operation,
                revision,
            } => {
                let result = agents::control_reviewed(
                    pool,
                    me,
                    &self.device,
                    &agent,
                    action,
                    &operation,
                    revision,
                )
                .await
                .map_err(|e| e.to_string())?;
                if result.status == "revoked" {
                    self.st
                        .log
                        .revoke_agent_invite(&agent)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                return Ok(Reply::AgentControl(result));
            }
            AgentRequest::Control { agent, action } => {
                agents::control(pool, me, &agent, action)
                    .await
                    .map_err(|e| e.to_string())?;
                // Turning an agent off also takes its share link down, here on the relay
                // so no client can leave it up.
                if action == protocol::AgentAction::Revoke {
                    self.st.log.revoke_agent_invite(&agent).await?;
                }
                return Ok(Reply::Done);
            }
        };
        r.map(|()| Reply::Done).map_err(|e| e.to_string())
    }

    pub(super) async fn request(
        &mut self,
        op: Op,
        registered: bool,
        authorization: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<Reply, String> {
        let pool = &self.st.pool;
        match op {
            Op::Register { profile } => {
                let profile = relay_admission::directory::prepare_profile(profile, &self.identity)?;
                let handle = profile.handle.clone();
                if !registered {
                    self.st
                        .limits
                        .register_ip
                        .check(&self.ip)
                        .map_err(limits::slow_down)?;
                }
                db::register(
                    authorization.ok_or("registration authorization unavailable")?,
                    &profile,
                    &handle,
                    &self.device,
                    &self.cert,
                    !registered,
                )
                .await?;
                Ok(Reply::Registered(profile))
            }
            Op::FetchLink { id } => {
                if !valid_hex64(&id) {
                    return Err("bad link id".into());
                }
                Ok(Reply::Link(
                    db::read_link_box(pool, &id)
                        .await
                        .map_err(|e| e.to_string())?,
                ))
            }
            _ if !registered => Err("register first".into()),
            Op::DeliverLink { id, sealed, device } => {
                self.st
                    .limits
                    .lookup_account
                    .check(&self.identity)
                    .map_err(limits::slow_down)?;
                if !valid_hex64(&id) || sealed.is_empty() || sealed.len() > db::MAX_LINK_BOX {
                    return Err("bad link box".into());
                }
                let authorization = authorization.ok_or("link authorization unavailable")?;
                if let Some(device) = &device {
                    db::enroll_device(authorization, &self.identity, &device.device, &device.cert)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                if !db::put_link_box(
                    authorization,
                    &id,
                    &sealed,
                    &self.identity,
                    &self.device,
                    device.as_ref(),
                )
                .await
                .map_err(|e| e.to_string())?
                {
                    return Err("a link box is already there".into());
                }
                Ok(Reply::Done)
            }
            Op::Agent(req) => self.agent(req).await,
            Op::RuntimeBinding(request) => {
                // A failed/replaced challenge must not retain the old checkpoint.
                self.managed_binding = None;
                let proof = crate::runtime_binding::prove(
                    &self.st,
                    &self.identity,
                    &self.device,
                    request.clone(),
                )
                .await
                .map_err(str::to_owned)?;
                if self.st.log.managed_device(&self.device).is_some() {
                    self.managed_binding = Some(request);
                }
                Ok(Reply::RuntimeBinding(proof))
            }
            Op::CallTurn { space, peer } => self.call_turn(&space, &peer).await,
            Op::CompleteRead(request) => self.complete_read(request).await,
            Op::Devices => {
                self.complete_legacy(protocol::complete_reads::Kind::Devices)
                    .await
            }
            Op::Unlink { device } => {
                let authorization = authorization.ok_or("unlink authorization unavailable")?;
                if !db::revoke_device(authorization, &self.identity, &device)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    return Err("not a linked device of yours".into());
                }
                tracing::info!(identity = %pseudo(&self.identity), "device unlinked");
                Ok(Reply::Done)
            }
            Op::SendDevice { to, sealed } => {
                self.st
                    .limits
                    .lookup_account
                    .check(&self.identity)
                    .map_err(limits::slow_down)?;
                if sealed.is_empty() || sealed.len() > db::MAX_LINK_BOX * 16 {
                    return Err("bad device message".into());
                }
                if !db::device_linked(pool, &self.identity, &to)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    return Err("not a linked device of yours".into());
                }
                self.st.fanout.send(
                    std::slice::from_ref(&self.identity),
                    &ServerFrame::DeviceMessage {
                        from: self.device.clone(),
                        to,
                        sealed,
                    },
                    None,
                );
                Ok(Reply::Done)
            }
            Op::Lookup { handle, prefix } => {
                self.st
                    .limits
                    .lookup_account
                    .check(&self.identity)
                    .map_err(limits::slow_down)?;
                let h = relay_admission::directory::reads::normalize_lookup(&handle);
                if h.is_empty() {
                    return Ok(Reply::Profiles(Vec::new()));
                }
                Ok(Reply::Profiles(
                    db::lookup(pool, &h, prefix)
                        .await
                        .map_err(|e| e.to_string())?,
                ))
            }
            Op::Profiles { ids } => {
                let ids = relay_admission::directory::reads::profile_ids(ids);
                Ok(Reply::Profiles(
                    db::profiles(pool, &ids).await.map_err(|e| e.to_string())?,
                ))
            }
            Op::CreateInvite {
                space,
                agent,
                role,
                max_uses,
                ttl_secs,
            } => {
                let invite = if let Some(agent) = agent {
                    self.own_agent(&agent).await?;
                    // A turned-off agent never gets a new link.
                    if crate::agents::owner_status(pool, &self.identity, &agent)
                        .await
                        .map_err(|e| e.to_string())?
                        .status
                        == "revoked"
                    {
                        return Err(crate::agents::AgentCitizenError::Revoked.to_string());
                    }
                    self.st
                        .log
                        .create_agent_invite(&self.identity, &agent, max_uses, ttl_secs)
                        .await?
                } else {
                    self.st
                        .log
                        .create_invite(&self.identity, &space, role, max_uses, ttl_secs)
                        .await?
                };
                self.st.analytics.count("invites_created", 1);
                Ok(Reply::Invite(invite))
            }
            Op::PublishAgreementKey { public, signed } => {
                relay_admission::directory::sealed_profiles::validate_agreement(
                    &self.identity,
                    &self.device,
                    &public,
                    &signed,
                )?;
                db::put_agreement_key(pool, &self.identity, &public, &signed)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(Reply::Done)
            }
            Op::AgreementKeys { ids } => {
                let ids = relay_admission::directory::reads::profile_ids(ids);
                Ok(Reply::AgreementKeys(
                    db::agreement_keys(pool, &ids)
                        .await
                        .map_err(|e| e.to_string())?,
                ))
            }
            Op::PutProfile { profile } => {
                relay_admission::directory::sealed_profiles::validate_profile(
                    &self.identity,
                    &self.device,
                    &profile,
                )?;
                if !db::put_profile(pool, &profile)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    return Err("stale profile version".into());
                }
                if let Ok(mut to) = self.st.log.co_members(&self.identity).await {
                    to.push(self.identity.clone());
                    self.st.fanout.send(
                        &to,
                        &ServerFrame::ProfileChanged {
                            identity: self.identity.clone(),
                            version: profile.version,
                        },
                        self.hub_id,
                    );
                }
                Ok(Reply::Done)
            }
            Op::PublishKeyPackages {
                packages,
                last_resort,
            } => {
                let publications = relay_admission::directory::key_packages::validate_publications(
                    &self.identity,
                    &self.device,
                    &packages,
                    last_resort.as_deref(),
                )?;
                if !db::put_key_packages(
                    pool,
                    &self.identity,
                    &self.device,
                    &publications,
                    last_resort.as_deref(),
                )
                .await
                .map_err(|e| e.to_string())?
                {
                    return Err("too many key packages stored".into());
                }
                Ok(Reply::Done)
            }
            Op::ClaimKeyPackages { ids, operation_id } => {
                self.st
                    .limits
                    .lookup_account
                    .check(&self.identity)
                    .map_err(limits::slow_down)?;
                let claimed = if let Some(operation) = operation_id {
                    crate::key_package_claims::claim(
                        pool,
                        &self.identity,
                        &self.device,
                        &operation,
                        &ids,
                    )
                    .await
                    .map_err(|e| e.to_string())?
                } else {
                    let ids: Vec<String> = ids.into_iter().take(50).collect();
                    db::claim_key_packages(pool, &ids)
                        .await
                        .map_err(|e| e.to_string())?
                };
                self.key_packages_low(&claimed).await;
                Ok(Reply::KeyPackages(claimed))
            }
            Op::GetProfiles { ids } => {
                let ids = relay_admission::directory::reads::profile_ids(ids);
                Ok(Reply::SealedProfiles(
                    db::sealed_profiles(pool, &ids)
                        .await
                        .map_err(|e| e.to_string())?,
                ))
            }
            Op::PreviewInvite { code } => {
                self.spend_invite_guess()?;
                let i = self.st.log.preview_invite(&code).await?;
                self.preview(i).await
            }
            Op::RedeemAgentInvite { code } => {
                self.spend_invite_guess()?;
                let observed = self.st.log.preview_invite(&code).await?;
                self.agent_invite_allowed(&observed).await?;
                let i = self
                    .st
                    .log
                    .redeem_agent_invite(&code, &self.identity)
                    .await?;
                self.st.analytics.count("agent_invites_redeemed", 1);
                self.preview(i).await
            }
            Op::RevokeAgentInvite { agent } => {
                self.own_agent(&agent).await?;
                self.st.log.revoke_agent_invite(&agent).await?;
                Ok(Reply::Done)
            }
            Op::RegisterPush { platform, token } => {
                if !crate::push::valid_registration(&platform, &token) {
                    return Err("bad push registration".into());
                }
                db::set_push_token(
                    &self.st.pool,
                    &self.identity,
                    &self.device,
                    &platform,
                    &token,
                )
                .await
                .map_err(|e| e.to_string())?;
                Ok(Reply::Done)
            }
        }
    }

    pub(super) fn spend_invite_guess(&self) -> Result<(), String> {
        self.st
            .limits
            .invite_account
            .check(&self.identity)
            .map_err(limits::slow_down)
    }

    /// Only an agent's owner shares or revokes its link.
    pub(super) async fn own_agent(&self, agent: &str) -> Result<(), String> {
        let profiles = db::profiles(&self.st.pool, &[agent.to_string()])
            .await
            .map_err(|e| e.to_string())?;
        let profile = profiles.first().ok_or("unknown agent")?;
        if profile.kind != IdentityKind::Agent
            || profile.owner.as_deref() != Some(self.identity.as_str())
        {
            return Err("only the owner shares an agent".into());
        }
        Ok(())
    }

    async fn agent_invite_allowed(&self, i: &crate::log::InviteInfo) -> Result<(), String> {
        if let Some(agent) = &i.agent {
            if !crate::agents::invite_live(&self.st.pool, agent)
                .await
                .map_err(|e| e.to_string())?
            {
                return Err(crate::agents::AgentCitizenError::Revoked.to_string());
            }
        }
        Ok(())
    }
    pub(super) async fn preview(&self, i: crate::log::InviteInfo) -> Result<Reply, String> {
        // Also recheck after redemption: revocation can race its FDB transaction.
        self.agent_invite_allowed(&i).await?;
        let mut ids = vec![i.inviter.clone()];
        ids.extend(i.agent.clone());
        let found = db::profiles(&self.st.pool, &ids)
            .await
            .map_err(|e| e.to_string())?;
        let inviter = found.iter().find(|p| p.id == i.inviter).cloned();
        let agent = i
            .agent
            .as_ref()
            .and_then(|id| found.iter().find(|p| p.id == *id).cloned())
            .map(Box::new);
        if i.agent.is_some() && agent.is_none() {
            return Err("unknown agent".into());
        }
        let (kind, title) = match &agent {
            Some(a) => ("agent", a.name.clone()),
            None => ("space", i.title),
        };
        Ok(Reply::Preview(protocol::InvitePreview {
            space: i.space,
            role: i.role,
            title,
            members: i.members,
            inviter,
            kind: kind.into(),
            agent,
        }))
    }
}

use super::*;
impl<S: Signer> Client<S> {
    pub fn claim_clock(&mut self, value: &str) -> Result<(), String> {
        self.membership_net.clock = if value.is_empty() {
            None
        } else {
            Some(
                KeyPackageClaimClock::from_server_ms(
                    value.parse().map_err(|_| "Invalid relay clock.")?,
                )
                .ok_or("Invalid relay clock.")?,
            )
        };
        Ok(())
    }

    pub(in crate::client) fn enqueue_control(
        &mut self,
        space: &str,
        body: EventBody,
        now: i64,
    ) -> Result<(), String> {
        let pending = self.clear_event(space, body, now)?;
        self.enqueue_handshake(pending)
    }
    pub(super) fn enqueue_handshake(&mut self, mut pending: PendingEnvelope) -> Result<(), String> {
        let id = pending.envelope()?.client_id().to_string();
        self.saved.outbox_sequence = self
            .saved
            .outbox_sequence
            .checked_add(1)
            .ok_or("Outbox order overflow.")?;
        pending.order = Some(self.saved.outbox_sequence);
        self.saved.outbox.insert(id, pending);
        Ok(())
    }
    pub(super) fn handshake(
        &self,
        space: &str,
        sealed: Sealed,
        now: i64,
    ) -> Result<PendingEnvelope, String> {
        self.sign_content(
            SignedContent {
                v: event_log::content::SEALED_CONTENT_VERSION,
                space: space.into(),
                client_id: types::new_ulid(now),
                author: self.account.identity.clone(),
                device: Some(self.account.device.clone()),
                at_ms: now,
                seen: self.head(space).as_ref().map(Into::into),
                payload: Some(Payload::Sealed(sealed)),
            }
            .encode(),
            true,
        )
    }
    pub(super) fn creating(
        &mut self,
        title: String,
        kind: SpaceKind,
        privacy: Privacy,
        members: Vec<String>,
        now: i64,
    ) -> Result<String, String> {
        let title = title.trim().to_owned();
        if kind != SpaceKind::Direct && title.is_empty() {
            return Err("Dê um nome à conversa.".into());
        }
        if title.len() > 4096 || members.len() > 4999 {
            return Err("O grupo é grande demais.".into());
        }
        let members = members
            .into_iter()
            .filter(|id| id != &self.account.identity)
            .collect::<BTreeSet<_>>();
        for id in &members {
            if event_log::parse_device_key(id).is_none() || !self.saved.profiles.contains_key(id) {
                return Err("Pessoa não encontrada.".into());
            }
        }
        let space = format!("sp_{}", types::new_ulid(now).to_ascii_lowercase());
        let pending = self.clear_event(
            &space,
            EventBody::SpaceCreated {
                title,
                kind,
                privacy,
            },
            now,
        )?;
        self.saved
            .outbox_sequence
            .checked_add(1)
            .ok_or("Outbox order overflow.")?;
        if privacy == Privacy::EndToEnd {
            self.device()?
                .create_group(&space)
                .map_err(|error| error.to_string())?;
        }
        self.enqueue_handshake(pending)?;
        self.saved
            .membership
            .creates
            .insert(space.clone(), members.into_iter().collect());
        Ok(space)
    }
    pub(super) fn require_admin(&self, space: &str) -> Result<Role, String> {
        let role = self
            .roster(space)
            .get(&self.account.identity)
            .copied()
            .ok_or("Você não faz parte desta conversa.")?;
        if !matches!(role, Role::Owner | Role::Admin) {
            return Err("Só quem administra a conversa pode fazer isso.".into());
        }
        Ok(role)
    }

    pub fn membership_request(
        &mut self,
        request: &str,
        now: i64,
    ) -> Result<DocumentResponse, String> {
        self.view_now_ms = now;
        if !self.synced {
            return Err("Conecte ao servidor antes de alterar a conversa.".into());
        }
        if request.len() > 512 * 1024 {
            return Err("Pedido grande demais.".into());
        }
        let command: Command = serde_json::from_str(request).map_err(|error| error.to_string())?;
        let result = match command {
            Command::RefreshDevices {} => {
                self.membership_net.devices_requested = false;
                self.membership_net.devices_due = 0;
                Value::Null
            }
            Command::StartDirect { identity_id } => {
                if identity_id == self.account.identity {
                    return Err("Escolha outra pessoa para conversar.".into());
                }
                if let Some(space) = self
                    .creating_views()
                    .iter()
                    .find(|view| view.kind == "direct" && view.roster.contains_key(&identity_id))
                    .map(|view| view.id.clone())
                {
                    json!(space)
                } else if let Some(space) = self.logs.keys().find(|space| {
                    self.metadata(space)
                        .is_some_and(|(_, kind, _)| kind == SpaceKind::Direct)
                        && {
                            let roster = self.roster(space);
                            roster.len() == 2
                                && roster.contains_key(&self.account.identity)
                                && roster.contains_key(&identity_id)
                        }
                }) {
                    json!(space)
                } else {
                    json!(self.creating(
                        String::new(),
                        SpaceKind::Direct,
                        Privacy::EndToEnd,
                        vec![identity_id],
                        now
                    )?)
                }
            }
            Command::CreateGroup { title, member_ids } => {
                json!(self.creating(title, SpaceKind::Group, Privacy::EndToEnd, member_ids, now)?)
            }
            Command::CreateCommunity { title } => json!(self.creating(
                title,
                SpaceKind::Community,
                Privacy::Closed,
                Vec::new(),
                now
            )?),
            Command::AddMember {
                space_id,
                identity_id,
            }
            | Command::AddAdmin {
                space_id,
                identity_id,
            } => {
                let command: Value =
                    serde_json::from_str(request).map_err(|error| error.to_string())?;
                let role = if command["op"] == "add_admin" {
                    Role::Admin
                } else {
                    Role::Member
                };
                self.require_admin(&space_id)?;
                if identity_id == self.account.identity
                    || event_log::parse_device_key(&identity_id).is_none()
                    || !self.saved.profiles.contains_key(&identity_id)
                {
                    return Err("Pessoa não encontrada.".into());
                }
                let roster = self.roster(&space_id);
                if self
                    .metadata(&space_id)
                    .is_some_and(|(_, kind, _)| kind == SpaceKind::Direct)
                    && roster.len() >= 2
                    && !roster.contains_key(&identity_id)
                {
                    return Err("Uma conversa direta tem duas pessoas.".into());
                }
                self.enqueue_control(
                    &space_id,
                    EventBody::MemberAdded {
                        identity: identity_id,
                        role,
                    },
                    now,
                )?;
                Value::Null
            }
            Command::RemoveMember {
                space_id,
                identity_id,
            } => {
                let role = self.require_admin(&space_id)?;
                let target = self
                    .roster(&space_id)
                    .get(&identity_id)
                    .copied()
                    .ok_or("Membro não encontrado.")?;
                if role == Role::Admin && target == Role::Owner {
                    return Err("Só o dono pode remover outro dono.".into());
                }
                self.enqueue_control(
                    &space_id,
                    EventBody::MemberRemoved {
                        identity: identity_id,
                    },
                    now,
                )?;
                Value::Null
            }
            Command::LeaveSpace { space_id } => {
                if !self.roster(&space_id).contains_key(&self.account.identity) {
                    return Err("Você não faz parte desta conversa.".into());
                }
                self.enqueue_control(
                    &space_id,
                    EventBody::MemberRemoved {
                        identity: self.account.identity.clone(),
                    },
                    now,
                )?;
                Value::Null
            }
            Command::EncryptChat { space_id } => {
                let (_, kind, privacy) =
                    self.metadata(&space_id).ok_or("Conversa não encontrada.")?;
                if privacy != Privacy::EndToEnd {
                    let role = self.roster(&space_id).get(&self.account.identity).copied();
                    if privacy != Privacy::Closed
                        || !matches!(kind, SpaceKind::Direct | SpaceKind::Group)
                    {
                        return Err("Espaços públicos continuam legíveis.".into());
                    }
                    if !matches!(role, Some(Role::Owner | Role::Admin))
                        && !(kind == SpaceKind::Direct && role == Some(Role::Member))
                    {
                        return Err("Só quem administra o grupo ativa a criptografia.".into());
                    }
                    self.enqueue_control(&space_id, EventBody::SpaceEncrypted, now)?;
                }
                Value::Null
            }
        };
        let update = self.update(Vec::new())?;
        Ok(DocumentResponse {
            result,
            update: Some(update),
            downloads: Vec::new(),
            uploads: self.pending_document_uploads(),
        })
    }
}

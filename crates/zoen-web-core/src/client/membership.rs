//! Browser storage adapter for the shared MLS membership planner. Claims are
//! journaled before consuming key packages, and recovery blobs gate publication.
use super::*;
use roda_mls::{
    leaf_name,
    reconciliation::{self, Owed},
};
use roda_proto::{KeyPackageClaimClock, KeyPackageRecord};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const CLAIM_REQUEST: u64 = 0x8000_0030;
const DEVICES_REQUEST: u64 = 0x8000_0031;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Journal {
    creates: BTreeMap<String, Vec<String>>,
    claims: BTreeMap<String, Claim>,
    joins: BTreeMap<String, BTreeMap<String, String>>,
    stranded: BTreeMap<String, BTreeSet<String>>,
    welcomes: BTreeMap<String, BTreeSet<String>>,
    pub(super) revoked: BTreeSet<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Claim {
    operation: String,
    device: String,
    epoch: u64,
    intent: Owed,
    targets: Vec<String>,
    records: Option<Vec<KeyPackageRecord>>,
}
#[derive(Default)]
pub(super) struct Network {
    pub(super) clock: Option<KeyPackageClaimClock>,
    claiming: Option<(String, String, i64)>,
    retry: BTreeMap<String, i64>,
    turns: BTreeMap<String, i64>,
    devices_requested: bool,
    devices_due: i64,
    pub(super) stuck: BTreeSet<String>,
}
impl Network {
    pub(super) fn restart(&mut self) {
        let clock = self.clock;
        *self = Self {
            clock,
            ..Default::default()
        };
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    StartDirect {
        identity_id: String,
    },
    CreateGroup {
        title: String,
        member_ids: Vec<String>,
    },
    CreateCommunity {
        title: String,
    },
    AddMember {
        space_id: String,
        identity_id: String,
    },
    AddAdmin {
        space_id: String,
        identity_id: String,
    },
    RemoveMember {
        space_id: String,
        identity_id: String,
    },
    LeaveSpace {
        space_id: String,
    },
    EncryptChat {
        space_id: String,
    },
    // Internal host wake after a confirmed device revocation.
    RefreshDevices {},
}

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

    pub(super) fn enqueue_control(
        &mut self,
        space: &str,
        body: EventBody,
        now: i64,
    ) -> Result<(), String> {
        let pending = self.clear_event(space, body, now)?;
        self.enqueue_handshake(pending)
    }
    fn enqueue_handshake(&mut self, mut pending: PendingEnvelope) -> Result<(), String> {
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
    fn handshake(&self, space: &str, sealed: Sealed, now: i64) -> Result<PendingEnvelope, String> {
        self.sign_content(
            SignedContent {
                v: roda_log::content::SEALED_CONTENT_VERSION,
                space: space.into(),
                client_id: roda_types::new_ulid(now),
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
    fn creating(
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
            if roda_log::parse_device_key(id).is_none() || !self.saved.profiles.contains_key(id) {
                return Err("Pessoa não encontrada.".into());
            }
        }
        let space = format!("sp_{}", roda_types::new_ulid(now).to_ascii_lowercase());
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
    fn require_admin(&self, space: &str) -> Result<Role, String> {
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
                    || roda_log::parse_device_key(&identity_id).is_none()
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

    pub(super) fn note_membership_event(&mut self, event: &Event) -> Result<(), String> {
        let space = &event.space;
        if matches!(event.body, EventBody::SpaceCreated { .. })
            && event.author == self.account.identity
        {
            if let Some(members) = self.saved.membership.creates.remove(space) {
                for identity in members {
                    self.enqueue_control(
                        space,
                        EventBody::MemberAdded {
                            identity,
                            role: Role::Member,
                        },
                        self.view_now_ms,
                    )?;
                }
            }
        }
        if matches!(event.body, EventBody::SpaceEncrypted)
            && event.device.as_deref() == Some(self.account.device.as_str())
            && self
                .device()?
                .group_state(space)
                .map_err(|error| error.to_string())?
                == GroupState::Missing
        {
            self.device()?
                .create_group(space)
                .map_err(|error| error.to_string())?;
        }
        if let EventBody::DeviceJoining { device } = &event.body {
            self.membership_net.devices_due = 0;
            if device != &self.account.device
                && self
                    .metadata(space)
                    .is_some_and(|(_, _, privacy)| privacy == Privacy::EndToEnd)
            {
                if let Ok(leaves) = self.device()?.leaves(space) {
                    let stale = leaves
                        .get(&(event.author.clone(), device.clone()))
                        .map(hex::encode)
                        .unwrap_or_default();
                    self.saved
                        .membership
                        .joins
                        .entry(space.clone())
                        .or_default()
                        .insert(leaf_name(&event.author, device), stale);
                }
            }
        }
        self.saved.membership.welcomes.remove(&event.client_id);
        Ok(())
    }

    pub(super) fn membership_waiting(&self, space: &str) -> bool {
        self.saved
            .outbox
            .values()
            .filter(|pending| pending.failure.is_none())
            .any(|pending| {
                pending.envelope().is_ok_and(|envelope| {
                    envelope.space() == space
                        && (matches!(
                            envelope.body(),
                            Some(
                                EventBody::MemberAdded { .. }
                                    | EventBody::MemberRemoved { .. }
                                    | EventBody::SpaceEncrypted
                            )
                        ) || envelope.sealed_kind() == Some(SealedKind::Welcome))
                })
            })
    }
    fn owed_membership(&self, space: &str) -> Result<Option<Owed>, String> {
        if self.membership_waiting(space) {
            return Ok(None);
        }
        let roster = self.roster(space);
        let Some(role) = roster.get(&self.account.identity) else {
            return Ok(None);
        };
        let Some(leaves) = self
            .device()?
            .committed_leaves(space)
            .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        let empty = BTreeSet::new();
        let joins = BTreeMap::new();
        Ok(reconciliation::plan(reconciliation::Input {
            me: &self.account.identity,
            admin: matches!(role, Role::Owner | Role::Admin),
            reconciling: false,
            listed: &roster.into_keys().collect(),
            leaves: &leaves,
            joins: self.saved.membership.joins.get(space).unwrap_or(&joins),
            stranded: self.saved.membership.stranded.get(space).unwrap_or(&empty),
            revoked: &self.saved.membership.revoked,
        }))
    }
    fn settle_membership(&mut self, space: &str) -> Result<(), String> {
        let Some(leaves) = self
            .device()?
            .committed_leaves(space)
            .map_err(|error| error.to_string())?
        else {
            return Ok(());
        };
        let roster = self.roster(space);
        if let Some(stranded) = self.saved.membership.stranded.get_mut(space) {
            let gone = stranded
                .iter()
                .filter(|name| {
                    name.split_once('/').is_none_or(|(id, device)| {
                        !leaves.contains_key(&(id.into(), device.into()))
                    })
                })
                .cloned()
                .collect::<Vec<_>>();
            for name in gone {
                stranded.remove(&name);
                self.saved
                    .membership
                    .joins
                    .entry(space.into())
                    .or_default()
                    .entry(name)
                    .or_default();
            }
        }
        if let Some(joins) = self.saved.membership.joins.get_mut(space) {
            joins.retain(|name, stale| {
                name.split_once('/').is_some_and(|(id, device)| {
                    roster.contains_key(id)
                        && leaves
                            .get(&(id.into(), device.into()))
                            .is_none_or(|key| hex::encode(key) == *stale)
                })
            });
        }
        Ok(())
    }
    fn recovery_recipients(&self, space: &str) -> Result<Option<Vec<(String, String)>>, String> {
        let Some(mine) = self.agreement_key() else {
            return Ok(None);
        };
        let mut recipients = Vec::new();
        let mut legacy = false;
        for identity in self.roster(space).into_keys() {
            if identity == self.account.identity {
                recipients.push((identity, mine.public_hex()));
            } else if let Some(public) = self.saved.profile.agreement.get(&identity) {
                recipients.push((identity, public.clone()));
            } else if self.profile_net.no_agreement.contains(&identity) {
                legacy = true;
            } else {
                return Err("waiting for member recovery keys".into());
            }
        }
        Ok((!legacy).then_some(recipients))
    }
    fn stage_membership(
        &mut self,
        space: &str,
        packages: &[Vec<u8>],
        remove: &BTreeSet<String>,
    ) -> Result<(), String> {
        let recipients = self.recovery_recipients(space)?;
        let before = self
            .provider
            .storage()
            .values
            .read()
            .map_err(|_| "MLS storage lock failed")?
            .clone();
        self.saved
            .outbox_sequence
            .checked_add(2)
            .ok_or("Outbox order overflow.")?;
        let result = (|| {
            let commit = self
                .device()?
                .commit(space, packages, remove)
                .map_err(|error| error.to_string())?;
            let mut sealed = Sealed::new(SealedKind::Commit, SUITE_ID, commit.commit);
            let mut upload = None;
            if let Some(recipients) = recipients {
                let summary = roda_mls::recovery::verify_group_info(
                    space,
                    &commit.group_info,
                    Some(&self.account.device),
                )
                .map_err(|error| error.to_string())?;
                if !summary
                    .identities
                    .is_subset(&self.roster(space).into_keys().collect())
                {
                    return Err("Recovery context includes an unlisted member.".into());
                }
                let recipients = recipients
                    .iter()
                    .map(|(id, key)| (id.as_str(), key.as_str()))
                    .collect::<Vec<_>>();
                let bytes = roda_log::recovery::seal(
                    space,
                    summary.epoch,
                    &self.account.device,
                    &commit.group_info,
                    &recipients,
                )
                .ok_or("Could not seal recovery context.")?;
                let blob = hex::encode(Sha256::digest(&bytes));
                sealed.recovery = roda_log::recovery::RecoveryRef {
                    version: 1,
                    epoch: summary.epoch,
                    blob: blob.clone(),
                }
                .encode();
                upload = Some((blob, bytes));
            }
            let mut pending = self.handshake(space, sealed, self.view_now_ms)?;
            let commit_id = pending.envelope()?.client_id().to_string();
            let welcome = commit
                .welcome
                .map(|welcome| -> Result<_, String> {
                    let pending = self.handshake(
                        space,
                        Sealed::new(SealedKind::Welcome, SUITE_ID, welcome),
                        self.view_now_ms,
                    )?;
                    let id = pending.envelope()?.client_id().to_owned();
                    let newcomers = packages
                        .iter()
                        .filter_map(|package| roda_mls::key_package_leaf(package).ok())
                        .map(|leaf| leaf_name(&leaf.identity, &leaf.device))
                        .collect();
                    Ok((id, pending, newcomers))
                })
                .transpose()?;
            // All fallible crypto/signing/encoding is complete. Mutate the durable
            // journal in one infallible block; do not copy every retained media blob.
            let order = self.saved.outbox_sequence;
            if let Some((blob, bytes)) = upload {
                pending.blob_uploads.push(blob.clone());
                self.saved.local_chunks.insert(blob, bytes);
            }
            pending.order = Some(order + 1);
            self.saved.outbox.insert(commit_id.clone(), pending);
            self.saved.outbox_sequence = order + 1;
            if let Some((id, mut welcome, newcomers)) = welcome {
                welcome.after_commit = Some(commit_id);
                welcome.order = Some(order + 2);
                self.saved.membership.welcomes.insert(id.clone(), newcomers);
                self.saved.outbox.insert(id, welcome);
                self.saved.outbox_sequence = order + 2;
            }
            self.saved.membership.claims.remove(space);
            Ok(())
        })();
        if result.is_err() {
            *self
                .provider
                .storage()
                .values
                .write()
                .map_err(|_| "MLS storage lock failed")? = before;
        }
        result
    }
    pub(super) fn membership_reply(
        &mut self,
        id: u64,
        result: &Result<Reply, String>,
    ) -> Result<bool, String> {
        if id == DEVICES_REQUEST {
            self.membership_net.devices_requested = false;
            self.membership_net.devices_due =
                self.view_now_ms
                    .saturating_add(if result.is_ok() { 60_000 } else { 5000 });
            if let Ok(Reply::Devices(devices)) = result {
                self.saved.membership.revoked = devices
                    .iter()
                    .filter(|device| device.revoked)
                    .map(|device| device.device.clone())
                    .collect();
            }
            return Ok(true);
        }
        if id != CLAIM_REQUEST {
            return Ok(false);
        }
        let Some((space, operation, _)) = self.membership_net.claiming.take() else {
            return Ok(true);
        };
        if let Some(claim) = self
            .saved
            .membership
            .claims
            .get_mut(&space)
            .filter(|claim| claim.operation == operation)
        {
            match result {
                Ok(Reply::KeyPackages(records)) => claim.records = Some(records.clone()),
                Err(error) if error == roda_proto::KEY_PACKAGE_CLAIM_EXPIRED => {
                    self.saved.membership.claims.remove(&space);
                    self.membership_net.clock = None;
                }
                _ => {}
            }
        }
        self.membership_net
            .retry
            .insert(space, self.view_now_ms.saturating_add(5000));
        Ok(true)
    }
    pub(super) fn creating_views(&self) -> Vec<ConversationView> {
        self.saved
            .outbox
            .values()
            .filter_map(|pending| {
                if pending.failure.is_some() {
                    return None;
                }
                let envelope = pending.envelope().ok()?;
                let EventBody::SpaceCreated {
                    title,
                    kind,
                    privacy,
                } = envelope.body()?
                else {
                    return None;
                };
                if self.logs.contains_key(envelope.space()) {
                    return None;
                }
                let mut roster = self
                    .saved
                    .membership
                    .creates
                    .get(envelope.space())
                    .into_iter()
                    .flatten()
                    .map(|identity| (identity.clone(), Role::Member))
                    .collect::<BTreeMap<_, _>>();
                roster.insert(self.account.identity.clone(), Role::Owner);
                let title = if kind == SpaceKind::Direct {
                    roster
                        .keys()
                        .find(|id| **id != self.account.identity)
                        .and_then(|id| self.saved.profiles.get(id))
                        .map(|person| {
                            if person.name.is_empty() {
                                format!("@{}", person.handle)
                            } else {
                                person.name.clone()
                            }
                        })
                        .unwrap_or_else(|| "Conversa".into())
                } else {
                    title
                };
                Some(ConversationView {
                    id: envelope.space().into(),
                    title,
                    kind: match kind {
                        SpaceKind::Direct => "direct",
                        SpaceKind::Group => "group",
                        SpaceKind::Community => "community",
                        SpaceKind::Personal => return None,
                    },
                    privacy,
                    profiles: roster
                        .keys()
                        .filter_map(|id| self.conversation_profile(id))
                        .collect(),
                    members: roster.len(),
                    roster,
                    can_send: false,
                    waiting: Some("Criando a conversa…".into()),
                    unread: 0,
                    mentions: 0,
                    unreadable: 0,
                    messages: Vec::new(),
                    event_count: "0".into(),
                    prefs: ChatPreferencesView {
                        pinned: false,
                        archived: false,
                        muted_until_ms: None,
                    },
                })
            })
            .collect()
    }

    pub(super) fn membership_deadline(&self) -> Option<i64> {
        self.membership_net
            .turns
            .values()
            .chain(self.membership_net.retry.values())
            .copied()
            .chain(std::iter::once(self.membership_net.devices_due))
            .chain(
                self.membership_net
                    .claiming
                    .iter()
                    .map(|(_, _, deadline)| *deadline),
            )
            .filter(|deadline| *deadline > self.view_now_ms)
            .min()
    }

    pub(super) fn membership_refused(&mut self, id: &str) -> Result<bool, String> {
        let Some(pending) = self.saved.outbox.get(id) else {
            return Ok(false);
        };
        let envelope = pending.envelope()?;
        let space = envelope.space().to_owned();
        match envelope.sealed_kind() {
            Some(SealedKind::Commit) => {
                // Restore the committed epoch before replaying the winner. User events
                // remain in the durable outbox and will be resealed against that epoch.
                self.device()?
                    .abandon(&space)
                    .map_err(|error| error.to_string())?;
                let dropped = self
                    .saved
                    .outbox
                    .iter()
                    .filter(|(other, pending)| {
                        *other == id || pending.after_commit.as_deref() == Some(id)
                    })
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>();
                for id in dropped {
                    self.saved.outbox.remove(&id);
                    self.saved.membership.welcomes.remove(&id);
                    self.sent.remove(&id);
                }
            }
            Some(SealedKind::Welcome) => {
                let stranded = self
                    .saved
                    .membership
                    .welcomes
                    .remove(id)
                    .unwrap_or_default();
                self.saved
                    .membership
                    .stranded
                    .entry(space.clone())
                    .or_default()
                    .extend(stranded);
                self.saved.outbox.remove(id);
                self.sent.remove(id);
            }
            _ => return Ok(false),
        }
        self.saved.membership.claims.remove(&space);
        self.membership_net
            .retry
            .insert(space, self.view_now_ms.saturating_add(500));
        Ok(true)
    }

    pub(super) fn membership_frames(&mut self, frames: &mut Vec<Vec<u8>>) -> Result<(), String> {
        if !self.synced {
            return Ok(());
        }
        if self
            .membership_net
            .claiming
            .as_ref()
            .is_some_and(|(_, _, deadline)| *deadline <= self.view_now_ms)
        {
            self.membership_net.claiming = None;
        }
        if self.membership_net.devices_requested
            && self.membership_net.devices_due <= self.view_now_ms
        {
            self.membership_net.devices_requested = false;
        }
        if !self.membership_net.devices_requested
            && self.membership_net.devices_due <= self.view_now_ms
            && self.logs.keys().any(|space| {
                self.metadata(space).is_some_and(|(_, kind, privacy)| {
                    kind != SpaceKind::Personal && privacy == Privacy::EndToEnd
                })
            })
        {
            frames.push(
                ClientFrame::Req {
                    id: DEVICES_REQUEST,
                    op: Op::Devices,
                }
                .encode(),
            );
            self.membership_net.devices_requested = true;
            self.membership_net.devices_due = self.view_now_ms.saturating_add(15_000);
        }
        for space in self.logs.keys().cloned().collect::<Vec<_>>() {
            if !self.metadata(&space).is_some_and(|(_, kind, privacy)| {
                kind != SpaceKind::Personal && privacy == Privacy::EndToEnd
            }) {
                continue;
            }
            self.settle_membership(&space)?;
            let Some(owed) = self.owed_membership(&space)? else {
                self.membership_net.turns.remove(&space);
                self.membership_net.retry.remove(&space);
                continue;
            };
            let ahead = self
                .roster(&space)
                .iter()
                .filter(|(identity, role)| {
                    matches!(role, Role::Owner | Role::Admin)
                        && identity.as_str() < self.account.identity.as_str()
                })
                .count() as i64;
            if ahead > 0 {
                let due = self
                    .membership_net
                    .turns
                    .entry(space.clone())
                    .or_insert(self.view_now_ms.saturating_add(ahead.saturating_mul(400)));
                if *due > self.view_now_ms {
                    continue;
                }
            }
            if self
                .membership_net
                .retry
                .get(&space)
                .is_some_and(|at| *at > self.view_now_ms)
                && self
                    .saved
                    .membership
                    .claims
                    .get(&space)
                    .is_none_or(|claim| claim.records.is_none())
            {
                continue;
            }
            if self.recovery_recipients(&space).is_err() {
                continue;
            }
            if owed.adds_nobody() {
                if self.stage_membership(&space, &[], &owed.remove).is_err() {
                    self.membership_net
                        .retry
                        .insert(space.clone(), self.view_now_ms.saturating_add(5000));
                }
                continue;
            }
            if self.membership_net.claiming.is_some() {
                continue;
            }
            let epoch = self
                .device()?
                .epoch(&space)
                .map_err(|error| error.to_string())?;
            let saved = self
                .saved
                .membership
                .claims
                .get(&space)
                .filter(|claim| {
                    claim.device == self.account.device
                        && claim.epoch == epoch
                        && claim.intent == owed
                })
                .cloned();
            let claim = if let Some(saved) = saved {
                saved
            } else {
                let Some(clock) = self.membership_net.clock else {
                    continue;
                };
                if self
                    .membership_net
                    .retry
                    .get(&space)
                    .is_some_and(|at| *at > self.view_now_ms)
                {
                    continue;
                }
                let mut random = [0; 16];
                getrandom::getrandom(&mut random).map_err(|error| error.to_string())?;
                let mut value = ((clock.milliseconds() as u128) << 80)
                    | (u128::from_be_bytes(random) & ((1u128 << 80) - 1));
                let alphabet = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
                let mut operation = [0; 26];
                for byte in operation.iter_mut().rev() {
                    *byte = alphabet[(value & 31) as usize];
                    value >>= 5;
                }
                Claim {
                    operation: String::from_utf8(operation.to_vec())
                        .map_err(|error| error.to_string())?,
                    device: self.account.device.clone(),
                    epoch,
                    intent: owed.clone(),
                    targets: owed.to_claim(),
                    records: None,
                }
            };
            self.saved
                .membership
                .claims
                .insert(space.clone(), claim.clone());
            if let Some(records) = claim.records {
                let records = records
                    .into_iter()
                    .filter(|record| {
                        roda_mls::key_package_leaf(&record.data).is_ok_and(|leaf| {
                            leaf.identity == record.identity
                                && leaf.device == record.device
                                && claim.targets.contains(&leaf.identity)
                                && (owed.add.contains(&leaf.identity)
                                    || owed
                                        .add_devices
                                        .contains(&(leaf.identity.clone(), leaf.device.clone())))
                                && !owed
                                    .remove
                                    .contains(&leaf_name(&record.identity, &record.device))
                        })
                    })
                    .collect::<Vec<_>>();
                if owed.add.iter().any(|id| {
                    claim.targets.contains(id)
                        && !records.iter().any(|record| &record.identity == id)
                }) {
                    self.membership_net.stuck.insert(space.clone());
                }
                let packages = records
                    .into_iter()
                    .map(|record| record.data)
                    .collect::<Vec<_>>();
                if packages.is_empty() && owed.remove.is_empty() {
                    self.saved.membership.claims.remove(&space);
                    continue;
                }
                if self
                    .stage_membership(&space, &packages, &owed.remove)
                    .is_err()
                {
                    // Bad/noncontributory member keys cannot tear down the account.
                    self.saved.membership.claims.remove(&space);
                    self.membership_net
                        .retry
                        .insert(space.clone(), self.view_now_ms.saturating_add(5000));
                }
            } else {
                self.membership_net.claiming = Some((
                    space.clone(),
                    claim.operation.clone(),
                    self.view_now_ms.saturating_add(15_000),
                ));
                frames.push(
                    ClientFrame::Req {
                        id: CLAIM_REQUEST,
                        op: Op::ClaimKeyPackages {
                            ids: claim.targets,
                            operation_id: Some(claim.operation),
                        },
                    }
                    .encode(),
                );
            }
        }
        Ok(())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "membership_tests.rs"]
mod tests;

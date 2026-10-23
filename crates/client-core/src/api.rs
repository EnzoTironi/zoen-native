//! The account and sync surface of the core (UniFFI).
//!
//! Swift supplies two things: a [`SecretVault`] (the Keychain) and a [`CoreListener`]
//! (callbacks that trigger a refresh on the main actor). Everything else stays in Rust.

mod agent_runtime;
pub(crate) mod call_turn;
mod sync_completion;

pub use agent_runtime::agent_id_for_root;

use std::{sync::Arc, time::Duration};

use protocol::{EphemeralKind, InvitePreview, Op, Reply};
use types::{new_id, EventBody, Identity, Privacy, Role, SpaceKind};

use crate::{
    dto::{AgentControl, AgentStatus, GroupKeysDto, Persona, PrivacyDto},
    i18n::t,
    net::{ConnState, Net, NetStatus},
    profile::{PhotoChange, ProfileDto, VAULT_AGREEMENT},
    sync::{VAULT_DEVICE, VAULT_IDENTITY},
    Client, CoreError,
};

/// Where the device's secrets live (Keychain on Apple platforms, a 0600 file in the CLI).
#[uniffi::export(with_foreign)]
pub trait SecretVault: Send + Sync {
    fn load(&self, key: String) -> Option<Vec<u8>>;
    fn save(&self, key: String, value: Vec<u8>) -> bool;
    fn delete(&self, key: String);
}

/// Called from the network thread, never while the core is locked. Hop to the main actor.
#[uniffi::export(with_foreign)]
pub trait CoreListener: Send + Sync {
    /// Spaces whose projection changed (empty = profiles or other global state).
    fn on_change(&self, space_ids: Vec<String>);
    /// `kind`: "typing", "stopped", "status" (detail = processing/building/…), "read" (detail = seq).
    fn on_ephemeral(&self, space_id: String, from_id: String, kind: String, detail: String);
    fn on_presence(&self, identity_id: String, online: bool);
    fn on_connection(&self, status: ConnectionDto);
    /// The relay refused something you sent (shown as a toast).
    fn on_error(&self, message: String);
    /// Someone's profile (name, bio or photo) changed on this device: re-read `get_profile`.
    fn on_profile_changed(&self, identity_id: String);
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AccountDto {
    pub identity_id: String,
    pub device_id: String,
    pub name: String,
    pub handle: String,
    pub relay_url: String,
    /// The relay has your profile and handle.
    pub registered: bool,
    /// The device key is loaded (from the Keychain) and can sign.
    pub unlocked: bool,
    /// A social signup has local keys, but its provider binding is not confirmed yet.
    #[serde(default)]
    #[uniffi(default = false)]
    pub setup_pending: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AgentAuthorizationDto {
    pub owner_id: String,
    pub agent_id: String,
    pub device_id: String,
    pub certificate: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ConnectionDto {
    /// "offline", "connecting" or "online".
    pub state: String,
    /// Caught up with the relay on this connection.
    pub synced: bool,
    /// Events still waiting to be sent.
    pub pending: u64,
    pub error: Option<String>,
}

impl ConnectionDto {
    pub(crate) fn from_status(st: &NetStatus, pending: u64) -> Self {
        Self {
            state: match st.state {
                ConnState::Offline => "offline",
                ConnState::Connecting => "connecting",
                ConnState::Online => "online",
            }
            .into(),
            synced: st.synced,
            pending,
            error: st.error.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct InviteDto {
    pub code: String,
    /// Canonical public link from `protocol::links`, shared by every platform.
    pub link: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct InvitePreviewDto {
    pub space_id: String,
    pub title: String,
    pub members: u32,
    pub inviter: Option<Persona>,
    /// `space` or `agent`.
    pub kind: String,
    /// Agent persona when `kind` is `agent`.
    pub agent: Option<Persona>,
}

fn link_no_longer_valid() -> CoreError {
    invalid(t(
        "Esse link de agente não vale mais.",
        "That agent link is no longer valid.",
    ))
}

pub(crate) fn invalid(reason: impl Into<String>) -> CoreError {
    CoreError::Invalid {
        reason: reason.into(),
    }
}

fn unexpected() -> CoreError {
    net_err("the relay answered something else".into())
}

fn net_err(e: String) -> CoreError {
    match e.as_str() {
        "offline" => invalid(t(
            "Sem conexão com o servidor agora.",
            "Can't reach the server right now.",
        )),
        "handle_taken" => invalid(t("Esse @ já é de outra pessoa.", "That @ is taken.")),
        _ => invalid(e),
    }
}

pub fn normalize_code(raw: &str) -> String {
    let s = raw.trim();
    let s = s.rsplit('/').next().unwrap_or(s);
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase()
}

pub(crate) fn account_dto_from(e: &crate::engine::Engine) -> Option<AccountDto> {
    let a = e.account()?;
    let me = e.my_profile();
    Some(AccountDto {
        identity_id: a.identity.clone(),
        device_id: a.device.clone(),
        name: me.as_ref().map(|m| m.name.clone()).unwrap_or_default(),
        handle: me.as_ref().map(|m| m.handle.clone()).unwrap_or_default(),
        relay_url: a.relay_url.clone(),
        registered: a.registered,
        unlocked: e.is_unlocked(),
        setup_pending: e.provider_setup_pending(),
    })
}

impl Client {
    pub(crate) fn account_dto(&self) -> Option<AccountDto> {
        account_dto_from(&self.lock())
    }

    pub(crate) fn with_net<T>(&self, f: impl FnOnce(&Net) -> T) -> Option<T> {
        self.net
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(f)
    }

    /// Opens the chat an agent link points at. A device that already has a direct chat
    /// with that agent goes straight there; otherwise one use is spent on the relay
    /// (each person counts once per code).
    async fn enter_agent_invite(
        &self,
        code: String,
        p: &InvitePreview,
    ) -> Result<String, CoreError> {
        if let Some(chat) = p
            .agent
            .as_ref()
            .and_then(|a| self.lock().direct_with(&a.id))
        {
            return Ok(chat);
        }
        let Reply::Preview(p) = self.request(Op::RedeemAgentInvite { code }).await? else {
            return Err(unexpected());
        };
        let agent = p.agent.ok_or_else(link_no_longer_valid)?;
        let mut e = self.lock();
        e.put_profiles(std::iter::once((*agent).clone()).chain(p.inviter).collect())?;
        e.start_direct(&agent.id)
    }

    async fn preview(&self, code: String) -> Result<InvitePreview, CoreError> {
        match self.request(Op::PreviewInvite { code }).await? {
            Reply::Preview(p) => Ok(p),
            _ => Err(unexpected()),
        }
    }

    pub(crate) async fn request(&self, op: Op) -> Result<Reply, CoreError> {
        // Clone the command channel out of the lock; never hold a lock across an await.
        let fut = {
            let guard = self.net.lock().unwrap_or_else(|p| p.into_inner());
            let net = guard.as_ref().ok_or_else(|| net_err("offline".into()))?;
            net.request_handle()
        };
        fut.send(op).await.map_err(net_err)
    }
}

// Keep this complete exported family in api: UniFFI metadata includes module_path!().
include!("api/account_surface.rs");

#[uniffi::export(async_runtime = "tokio")]
impl Client {
    /// People on Zoen by @ (prefix search). Their profiles are kept locally.
    pub async fn find_people(&self, query: String) -> Result<Vec<Persona>, CoreError> {
        let q = query.trim().trim_start_matches('@').to_lowercase();
        if q.is_empty() {
            return Ok(vec![]);
        }
        let Reply::Profiles(found) = self
            .request(Op::Lookup {
                handle: q,
                prefix: true,
            })
            .await?
        else {
            return Err(unexpected());
        };
        let mut e = self.lock();
        let me = e.me.clone().unwrap_or_default();
        let found: Vec<Identity> = found.into_iter().filter(|p| p.id != me).collect();
        e.put_profiles(found.clone())?;
        Ok(found.iter().map(|p| e.persona(&p.id)).collect())
    }

    pub async fn create_invite(&self, space_id: String) -> Result<InviteDto, CoreError> {
        let Reply::Invite(c) = self
            .request(Op::CreateInvite {
                space: space_id,
                agent: None,
                role: Role::Member,
                max_uses: 50,
                ttl_secs: 7 * 24 * 3600,
            })
            .await?
        else {
            return Err(unexpected());
        };
        Ok(InviteDto {
            link: protocol::links::space(&c.code),
            code: c.code,
            expires_at_ms: c.expires_at_ms,
        })
    }

    /// Shareable link for an agent (first-class citizen). Opens a chat with the agent.
    pub async fn create_agent_invite(&self, agent_id: String) -> Result<InviteDto, CoreError> {
        let Reply::Invite(c) = self
            .request(Op::CreateInvite {
                space: String::new(),
                agent: Some(agent_id),
                role: Role::Member,
                max_uses: 10_000,
                ttl_secs: 365 * 24 * 3600,
            })
            .await?
        else {
            return Err(unexpected());
        };
        Ok(InviteDto {
            link: protocol::links::agent(&c.code),
            code: c.code,
            expires_at_ms: c.expires_at_ms,
        })
    }

    /// Owner: one of your agents' status on the relay.
    pub async fn agent_status(&self, agent_id: String) -> Result<AgentStatus, CoreError> {
        let Reply::AgentState(s) = self
            .request(Op::Agent(protocol::AgentRequest::Status {
                agent: agent_id,
            }))
            .await?
        else {
            return Err(unexpected());
        };
        match s.status.as_str() {
            "active" => Ok(AgentStatus::Active),
            "paused" => Ok(AgentStatus::Paused),
            "revoked" => Ok(AgentStatus::Revoked),
            _ => Err(unexpected()),
        }
    }

    /// Owner: pause, resume or revoke one of your agents. Revoking is final and also
    /// takes its share link down.
    pub async fn control_agent(
        &self,
        agent_id: String,
        action: AgentControl,
    ) -> Result<(), CoreError> {
        let action = match action {
            AgentControl::Pause => protocol::AgentAction::Pause,
            AgentControl::Resume => protocol::AgentAction::Resume,
            AgentControl::Revoke => protocol::AgentAction::Revoke,
        };
        self.agent_done(protocol::AgentRequest::Control {
            agent: agent_id,
            action,
        })
        .await
    }

    /// Takes the agent's share link down; the next `create_agent_invite` mints a new one.
    pub async fn revoke_agent_invite(&self, agent_id: String) -> Result<(), CoreError> {
        match self
            .request(Op::RevokeAgentInvite { agent: agent_id })
            .await?
        {
            Reply::Done => Ok(()),
            _ => Err(unexpected()),
        }
    }

    /// Registers this device's push token with the relay ("apns", "apns-sandbox" or "fcm").
    /// An empty token clears it. The relay only ever sends a content-free wake.
    pub async fn register_push(&self, platform: String, token: String) -> Result<(), CoreError> {
        match self.request(Op::RegisterPush { platform, token }).await? {
            Reply::Done => Ok(()),
            _ => Err(unexpected()),
        }
    }

    /// Opens what `zoen://agent/<x>` or `https://tryzoen.com/a/<x>` points at: an invite
    /// code is redeemed, anything else (or a code that doesn't work) is tried as the
    /// agent's handle. Returns the direct chat with the agent.
    pub async fn open_agent_link(&self, target: String) -> Result<String, CoreError> {
        let raw = target.trim();
        let raw = raw
            .rsplit('/')
            .next()
            .unwrap_or(raw)
            .trim_start_matches('@');
        let code_shaped = protocol::valid_invite_code(raw);
        if code_shaped {
            let code = raw.to_uppercase();
            if let Ok(p) = self.preview(code.clone()).await {
                if p.kind == "agent" {
                    if let Ok(chat) = self.enter_agent_invite(code, &p).await {
                        return Ok(chat);
                    }
                }
            }
        }
        let handle = raw.to_lowercase();
        let Reply::Profiles(found) = self
            .request(Op::Lookup {
                handle: handle.clone(),
                prefix: false,
            })
            .await?
        else {
            return Err(unexpected());
        };
        let agent = found
            .into_iter()
            .find(|p| p.kind == types::IdentityKind::Agent && p.handle == handle);
        let agent = match agent {
            Some(a) => a,
            // Looked like a link code, and no agent goes by that name either: the link died.
            None if code_shaped => return Err(link_no_longer_valid()),
            None => {
                return Err(CoreError::NotFound {
                    what: t("agente", "agent"),
                });
            }
        };
        let mut e = self.lock();
        let id = agent.id.clone();
        e.put_profiles(vec![agent])?;
        e.start_direct(&id)
    }

    pub async fn preview_invite(&self, code: String) -> Result<InvitePreviewDto, CoreError> {
        let p = self.preview(normalize_code(&code)).await?;
        let mut e = self.lock();
        let mut profiles = Vec::new();
        if let Some(i) = p.inviter.clone() {
            profiles.push(i);
        }
        if let Some(a) = p.agent.clone() {
            profiles.push(*a);
        }
        if !profiles.is_empty() {
            e.put_profiles(profiles)?;
        }
        let inviter = p.inviter.as_ref().map(|i| e.persona(&i.id));
        let agent = p.agent.as_ref().map(|a| e.persona(&a.id));
        Ok(InvitePreviewDto {
            space_id: p.space,
            title: p.title,
            members: p.members,
            inviter,
            kind: p.kind,
            agent,
        })
    }

    /// Joins with a code or link. Returns the Space id (or the direct chat with an agent).
    pub async fn join_invite(&self, code: String) -> Result<String, CoreError> {
        let code = normalize_code(&code);
        let p = self.preview(code.clone()).await?;
        if p.kind == "agent" {
            return self.enter_agent_invite(code, &p).await;
        }
        self.lock().queue_join(&p.space, p.role, &code)?;
        Ok(p.space)
    }

    /// Waits (up to `timeout_ms`) until the chat's current photo is on this device (it may
    /// still be on its way through the relay). Returns its bytes.
    pub async fn wait_for_background_media(
        &self,
        space_id: String,
        timeout_ms: u64,
    ) -> Option<Vec<u8>> {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let got = {
                let e = self.lock();
                e.background(&space_id)
                    .ok()
                    .flatten()
                    .and_then(|b| b.media)
                    .and_then(|m| e.media(&m.sha256).ok().flatten())
            };
            if got.is_some() || tokio::time::Instant::now() >= deadline {
                return got;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Waits (up to `timeout_ms`) until online, caught up and with nothing left to send.
    pub async fn wait_until_idle(&self, timeout_ms: u64) -> ConnectionDto {
        self.wait_sync_completion(timeout_ms).await.0
    }

    /// Strict completion for CLI and native bounded sync borrowers. A deadline is
    /// not completion, including a scheduled MLS retry with an empty outbox.
    /// This returns the observation ending the same wait, not a later recheck or
    /// an atomic projection snapshot. Callers retain their original whole budget.
    pub async fn wait_until_settled(&self, timeout_ms: u64) -> Result<ConnectionDto, CoreError> {
        let (connection, observed) = self.wait_sync_completion(timeout_ms).await;
        if observed.satisfied() {
            Ok(connection)
        } else {
            Err(invalid(connection.error.unwrap_or_else(|| {
                "Sync did not finish before the deadline; queued work is saved for retry.".into()
            })))
        }
    }
}

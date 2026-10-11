//! The account and sync surface of the core (UniFFI).
//!
//! Swift supplies two things: a [`SecretVault`] (the Keychain) and a [`CoreListener`]
//! (callbacks that trigger a refresh on the main actor). Everything else stays in Rust.

use std::{sync::Arc, time::Duration};

use roda_proto::{EphemeralKind, InvitePreview, Op, Reply};
use roda_types::{new_id, EventBody, Identity, Privacy, Role, SpaceKind};

use crate::{
    dto::{GroupKeysDto, Persona, PrivacyDto},
    i18n::t,
    net::{ConnState, Net, NetStatus},
    profile::{PhotoChange, ProfileDto, VAULT_AGREEMENT},
    sync::{VAULT_DEVICE, VAULT_IDENTITY},
    CoreError, RodaEngine,
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

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
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
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
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

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct InviteDto {
    pub code: String,
    /// `zoen://join/CODE` (universal link once the domain and AASA exist).
    pub link: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct InvitePreviewDto {
    pub space_id: String,
    pub title: String,
    pub members: u32,
    pub inviter: Option<Persona>,
}

fn invalid(reason: impl Into<String>) -> CoreError {
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

impl RodaEngine {
    fn sync_idle(&self, connection: &ConnectionDto) -> bool {
        connection.state == "online" && connection.synced && connection.pending == 0 && {
            let engine = self.lock();
            engine.net.unknown.is_empty()
                && engine.uploads_pending() == 0
                && engine.profiles_settled()
                && engine.mls_settled()
        }
    }

    /// Rust CLI contract; no new exported ABI. A deadline is not completion, including
    /// a scheduled MLS retry with an empty event outbox.
    pub async fn wait_until_settled(&self, timeout_ms: u64) -> Result<ConnectionDto, CoreError> {
        let connection = self.wait_until_idle(timeout_ms).await;
        if self.sync_idle(&connection) {
            Ok(connection)
        } else {
            Err(invalid(connection.error.unwrap_or_else(|| {
                "Sync did not finish before the deadline; queued work is saved for retry.".into()
            })))
        }
    }

    pub(crate) fn account_dto_pub(&self) -> Option<AccountDto> {
        self.account_dto()
    }

    pub(crate) fn account_dto(&self) -> Option<AccountDto> {
        let e = self.lock();
        let a = e.account()?.clone();
        let me = e.my_profile();
        Some(AccountDto {
            identity_id: a.identity.clone(),
            device_id: a.device.clone(),
            name: me.as_ref().map(|m| m.name.clone()).unwrap_or_default(),
            handle: me.as_ref().map(|m| m.handle.clone()).unwrap_or_default(),
            relay_url: a.relay_url.clone(),
            registered: a.registered,
            unlocked: e.is_unlocked(),
        })
    }

    fn with_net<T>(&self, f: impl FnOnce(&Net) -> T) -> Option<T> {
        self.net
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(f)
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

#[uniffi::export]
impl RodaEngine {
    /// The account on this device, if any.
    pub fn account(&self) -> Option<AccountDto> {
        self.account_dto()
    }

    /// Creates an identity key and a device key, keeps both secrets in `vault`, your
    /// profile in the local store, and your on-device Zoen. Registration with the relay
    /// happens when sync starts (now if online, later if not).
    pub fn create_account(
        &self,
        name: String,
        handle: String,
        relay_url: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<AccountDto, CoreError> {
        {
            let mut e = self.lock();
            let (root, device, agreement) = e.create_account(&name, &handle, &relay_url)?;
            if !vault.save(VAULT_IDENTITY.into(), root.to_vec())
                || !vault.save(VAULT_DEVICE.into(), device.to_vec())
                || !vault.save(VAULT_AGREEMENT.into(), agreement.to_vec())
            {
                // No vault, no account: never leave a key only in memory.
                let _ = e.wipe();
                let _ = e.store.meta_delete("account");
                return Err(invalid(t(
                    "Não deu para guardar a chave no Keychain.",
                    "Couldn't store the key in the Keychain.",
                )));
            }
        }
        self.account_dto().ok_or_else(|| invalid("account"))
    }

    /// Loads the device key from `vault`. `false` = no account here yet.
    pub fn unlock(&self, vault: Arc<dyn SecretVault>) -> Result<bool, CoreError> {
        let mut e = self.lock();
        if e.account().is_none() {
            return Ok(false);
        }
        if !e.unlock(vault.load(VAULT_DEVICE.into()))? {
            return Ok(false);
        }
        for peer in crate::link_api::load_peers(&vault) {
            e.add_peer(peer);
        }
        if let Some(fresh) = e.unlock_profile(vault.load(VAULT_AGREEMENT.into()))? {
            if !vault.save(VAULT_AGREEMENT.into(), fresh.to_vec()) {
                e.net.profiles.agreement = None;
                return Err(invalid(t(
                    "Não deu para guardar a chave no Keychain.",
                    "Couldn't store the key in the Keychain.",
                )));
            }
        }
        Ok(true)
    }

    /// Connects to the relay and keeps the connection (reconnects with backoff).
    pub fn start_sync(&self, listener: Option<Arc<dyn CoreListener>>) -> Result<(), CoreError> {
        let mut guard = self.net.lock().unwrap_or_else(|p| p.into_inner());
        if guard.is_some() {
            return Ok(());
        }
        if !self.lock().is_unlocked() {
            return Err(CoreError::Forbidden {
                reason: t(
                    "Sem chave deste aparelho.",
                    "This device's key isn't loaded.",
                ),
            });
        }
        *guard = Some(Net::start(self.inner.clone(), listener, self.lang).map_err(invalid)?);
        Ok(())
    }

    /// Forgets everything on this device: chats, account and keys. Your account and
    /// chats stay on the relay for the other members; this device just signs out.
    pub fn erase_device(&self, vault: Arc<dyn SecretVault>) -> Result<(), CoreError> {
        self.stop_sync();
        {
            let mut e = self.lock();
            e.wipe()?;
            e.net = Default::default();
        }
        vault.delete(VAULT_IDENTITY.into());
        vault.delete(VAULT_DEVICE.into());
        vault.delete(VAULT_AGREEMENT.into());
        Ok(())
    }

    pub fn stop_sync(&self) {
        let net = self.net.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(n) = net {
            self.lock().net.poke = None;
            drop(n);
        }
    }

    pub fn connection(&self) -> ConnectionDto {
        let pending = self.lock().outbox_len();
        self.with_net(|n| ConnectionDto::from_status(&n.status(), pending))
            .unwrap_or(ConnectionDto {
                state: "offline".into(),
                synced: false,
                pending,
                error: None,
            })
    }

    /// Changes your name/@ (re-registers with the relay).
    pub fn update_profile(
        &self,
        name: String,
        handle: String,
        bio: String,
    ) -> Result<AccountDto, CoreError> {
        self.lock().update_profile(&name, &handle, &bio)?;
        self.with_net(|n| n.flush());
        self.account_dto().ok_or_else(|| invalid("account"))
    }

    /// Opens (or reuses) the DM with someone you found with `find_people`.
    pub fn start_direct(&self, identity_id: String) -> Result<String, CoreError> {
        self.lock().start_direct(&identity_id)
    }

    pub fn create_group(
        &self,
        title: String,
        member_ids: Vec<String>,
    ) -> Result<String, CoreError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(invalid(t("Dê um nome ao grupo.", "Give the group a name.")));
        }
        self.create_group_with(title.to_string(), member_ids, PrivacyDto::EndToEnd)
    }

    /// Turns on end-to-end encryption in a relay-readable chat or group (ADR 0027). There is
    /// no way back: privacy only goes up.
    pub fn encrypt_chat(&self, space_id: String) -> Result<(), CoreError> {
        self.lock().encrypt_space(&space_id)
    }

    /// A group with a chosen privacy. `EndToEnd` (what `create_group` makes) is an MLS
    /// group: the relay orders and stores ciphertext only (ADR 0026).
    pub fn create_group_with(
        &self,
        title: String,
        member_ids: Vec<String>,
        privacy: PrivacyDto,
    ) -> Result<String, CoreError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(invalid(t("Dê um nome ao grupo.", "Give the group a name.")));
        }
        let privacy = match privacy {
            PrivacyDto::EndToEnd => Privacy::EndToEnd,
            PrivacyDto::Closed => Privacy::Closed,
            PrivacyDto::Public => Privacy::Public,
        };
        self.lock()
            .create_synced_space(title, SpaceKind::Group, privacy, &member_ids)
    }

    /// The leaves (`identity/device`) of an end-to-end chat's MLS group on this device.
    pub fn group_devices(&self, space_id: String) -> Vec<String> {
        self.lock()
            .mls_leaves(&space_id)
            .unwrap_or_default()
            .into_iter()
            .map(|(id, dev)| roda_mls::leaf_name(&id, &dev))
            .collect()
    }

    /// The MLS group of an end-to-end chat as this device has it.
    pub fn group_keys(&self, space_id: String) -> Option<GroupKeysDto> {
        self.lock()
            .mls_status(&space_id)
            .map(|(epoch, digest, members)| GroupKeysDto {
                epoch,
                digest,
                members,
            })
    }

    /// A Space (community): closed by default. Synced when signed in; local-only in the demo.
    pub fn create_community(&self, title: String) -> Result<String, CoreError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(invalid(t(
                "Dê um nome ao espaço.",
                "Give the space a name.",
            )));
        }
        let mut e = self.lock();
        if e.net.author.is_some() {
            return e.create_synced_space(title, SpaceKind::Community, Privacy::Closed, &[]);
        }
        let me = e.me_id()?;
        let space = new_id("sp");
        e.append(
            &space,
            &me,
            EventBody::SpaceCreated {
                title: title.to_string(),
                kind: SpaceKind::Community,
                privacy: Privacy::Closed,
            },
        )?;
        Ok(space)
    }

    pub fn add_member(&self, space_id: String, identity_id: String) -> Result<(), CoreError> {
        self.lock()
            .add_member(&space_id, &identity_id, Role::Member)
    }

    /// Adds someone who can also add, remove and commit for the group.
    pub fn add_admin(&self, space_id: String, identity_id: String) -> Result<(), CoreError> {
        self.lock().add_member(&space_id, &identity_id, Role::Admin)
    }

    pub fn remove_member(&self, space_id: String, identity_id: String) -> Result<(), CoreError> {
        self.lock().remove_member(&space_id, &identity_id)
    }

    pub fn leave_space(&self, space_id: String) -> Result<(), CoreError> {
        self.lock().leave_space(&space_id)
    }

    /// Typing indicator (throttled by the caller: call on change, and `false` on clear/send).
    pub fn set_typing(&self, space_id: String, typing: bool) {
        if !self.lock().net.synced.contains(&space_id) {
            return;
        }
        self.with_net(|n| {
            n.ephemeral(
                space_id,
                if typing {
                    EphemeralKind::Typing
                } else {
                    EphemeralKind::StoppedTyping
                },
            )
        });
    }

    /// What you (or your agent) are doing in a chat: "processing", "building", "in_call"…
    pub fn set_status(&self, space_id: String, status: String) {
        if !self.lock().net.synced.contains(&space_id) {
            return;
        }
        self.with_net(|n| n.ephemeral(space_id, EphemeralKind::Status { status }));
    }

    pub fn is_online(&self, identity_id: String) -> bool {
        self.lock().is_online(&identity_id)
    }

    /// Whether this chat goes through the relay (vs. living only on this device).
    pub fn is_synced(&self, space_id: String) -> bool {
        self.lock().net.synced.contains(&space_id)
    }

    /// A person's profile as this device can read it: name, bio and photo for contacts
    /// (people you share a chat or Space with), only the @handle for everyone else.
    pub fn get_profile(&self, identity_id: String) -> Result<ProfileDto, CoreError> {
        self.lock().profile_view(&identity_id)
    }

    /// Edits your encrypted profile. The relay stores only ciphertext; your contacts
    /// get it through the key you share with them. `photo`: keep, remove or set (≤ 5 MB).
    pub fn update_my_profile(
        &self,
        name: String,
        bio: String,
        photo: PhotoChange,
    ) -> Result<ProfileDto, CoreError> {
        let mut e = self.lock();
        e.update_my_profile(&name, &bio, photo)?;
        let me = e.me_id()?;
        e.profile_view(&me)
    }

    /// Stops sharing your profile with someone: your profile key rotates, and they keep
    /// only what they had already seen.
    pub fn block_person(&self, identity_id: String) -> Result<(), CoreError> {
        self.lock().block_person(&identity_id)
    }

    pub fn unblock_person(&self, identity_id: String) -> Result<(), CoreError> {
        self.lock().unblock_person(&identity_id)
    }

    pub fn blocked_people(&self) -> Result<Vec<String>, CoreError> {
        self.lock().blocked_people()
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl RodaEngine {
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
                role: Role::Member,
                max_uses: 50,
                ttl_secs: 7 * 24 * 3600,
            })
            .await?
        else {
            return Err(unexpected());
        };
        Ok(InviteDto {
            link: format!("zoen://join/{}", c.code),
            code: c.code,
            expires_at_ms: c.expires_at_ms,
        })
    }

    pub async fn preview_invite(&self, code: String) -> Result<InvitePreviewDto, CoreError> {
        let p = self.preview(normalize_code(&code)).await?;
        let mut e = self.lock();
        let inviter = match p.inviter {
            Some(i) => {
                let id = i.id.clone();
                e.put_profiles(vec![i])?;
                Some(e.persona(&id))
            }
            None => None,
        };
        Ok(InvitePreviewDto {
            space_id: p.space,
            title: p.title,
            members: p.members,
            inviter,
        })
    }

    /// Joins with a code or link. Returns the Space id; its history arrives with sync.
    pub async fn join_invite(&self, code: String) -> Result<String, CoreError> {
        let code = normalize_code(&code);
        let p = self.preview(code.clone()).await?;
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
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let c = self.connection();
            let idle = self.sync_idle(&c);
            if idle || tokio::time::Instant::now() >= deadline {
                return c;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    }
}

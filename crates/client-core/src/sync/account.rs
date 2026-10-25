//! Local account creation, authorization and device unlock.

use super::{
    new_id, normalize_handle, t, AccountMeta, Author, CoreError, Engine, EventBody, Identity,
    IdentityId, IdentityKind, PreparedAccount, Privacy, Role, Signer, SpaceKind, R,
};

pub(super) struct AccountDetails<'a> {
    pub(super) name: &'a str,
    pub(super) handle: &'a str,
    pub(super) relay_url: &'a str,
}

impl Engine {
    // ── account ──

    pub fn account(&self) -> Option<&AccountMeta> {
        self.net.account.as_ref()
    }

    pub fn is_unlocked(&self) -> bool {
        self.net.author.is_some()
    }

    /// Authorizes an agent root from this account's unlocked, certified device.
    /// The relay must still check enrollment at the agent's first registration.
    pub fn authorize_agent(&self, agent: &str) -> R<types::AgentOwnerProof> {
        self.net
            .author
            .as_ref()
            .filter(|author| {
                self.account()
                    .is_some_and(|a| a.identity == author.identity)
            })
            .and_then(|author| event_log::agent_owner::authorize(author, agent))
            .ok_or_else(|| CoreError::Forbidden {
                reason: "agent authorization needs an unlocked owner device and a valid agent key"
                    .into(),
            })
    }

    pub(crate) fn save_linked_account(&mut self, a: AccountMeta) -> R<()> {
        self.save_account(a)
    }

    pub(super) fn save_account(&mut self, a: AccountMeta) -> R<()> {
        self.store.set_meta(
            "account",
            &serde_json::to_string(&a).map_err(|e| CoreError::Storage {
                message: e.to_string(),
            })?,
        )?;
        self.net.account = Some(a);
        Ok(())
    }

    /// Creates this device's account. Returns the identity, device and agreement secrets for
    /// the vault; nothing secret is written to SQLite.
    pub fn create_account(
        &mut self,
        name: &str,
        handle: &str,
        relay_url: &str,
    ) -> R<([u8; 32], [u8; 32], [u8; 32])> {
        self.create_account_as(name, handle, relay_url, Signer::generate()?, None, false)
    }

    /// An Agent citizen's own account: its root key, a fresh certified device and the
    /// owner device's proof (ADR 0049). Same registration, MLS and sync as a person.
    pub fn create_agent_account(
        &mut self,
        name: &str,
        handle: &str,
        relay_url: &str,
        root_secret: [u8; 32],
        owner: &str,
        proof: types::AgentOwnerProof,
    ) -> R<([u8; 32], [u8; 32], [u8; 32])> {
        let root = Signer::from_secret(&root_secret);
        if !event_log::agent_owner::verify(owner, &root.id(), &proof) {
            return Err(CoreError::Forbidden {
                reason: "invalid agent owner proof".into(),
            });
        }
        self.create_account_as(
            name,
            handle,
            relay_url,
            root,
            Some((owner.to_string(), proof)),
            false,
        )
    }

    pub(super) fn create_account_as(
        &mut self,
        name: &str,
        handle: &str,
        relay_url: &str,
        root: Signer,
        agent: Option<(String, types::AgentOwnerProof)>,
        provider_signup: bool,
    ) -> R<([u8; 32], [u8; 32], [u8; 32])> {
        let personal_agent = agent.is_none();
        self.create_account_as_with_keys(
            AccountDetails {
                name,
                handle,
                relay_url,
            },
            root,
            agent,
            provider_signup,
            || super::account_preparation::AccountKeys::generate(personal_agent),
        )
    }

    pub(super) fn create_account_as_with_keys(
        &mut self,
        details: AccountDetails<'_>,
        root: Signer,
        agent: Option<(String, types::AgentOwnerProof)>,
        provider_signup: bool,
        prepare: impl FnOnce()
            -> Result<super::account_preparation::AccountKeys, event_log::CryptoError>,
    ) -> R<([u8; 32], [u8; 32], [u8; 32])> {
        let AccountDetails {
            name,
            handle,
            relay_url,
        } = details;
        if self.net.account.is_some() {
            return Err(CoreError::Invalid {
                reason: t(
                    "Este aparelho já tem uma conta.",
                    "This device already has an account.",
                ),
            });
        }
        let handle = normalize_handle(handle).ok_or_else(|| CoreError::Invalid {
            reason: t(
                "O @ precisa ter de 3 a 24 letras, números, '.' ou '_', começando com letra.",
                "Your @ needs 3–24 letters, digits, '.' or '_', starting with a letter.",
            ),
        })?;
        let name = name.trim();
        if name.is_empty() {
            return Err(CoreError::Invalid {
                reason: t("Diga seu nome.", "Tell us your name."),
            });
        }
        // Every fallible key constructor runs before any deletion or canonical write.
        let prepared = prepare()?;
        let root_secret = root.secret();
        let device = prepared.device.secret();
        let agreement = prepared.agreement.secret();
        self.replace_prepared_account(
            PreparedAccount {
                root: &root_secret,
                device: &device,
                agreement: &agreement,
                profile_key: Some(&prepared.profile),
                personal_agent_key: prepared.personal_agent.as_ref(),
                name,
                handle: &handle,
                relay: relay_url.trim_end_matches('/'),
                agent: agent.as_ref().map(|(owner, proof)| (owner.as_str(), proof)),
            },
            provider_signup,
        )?;
        Ok((root_secret, device, agreement))
    }

    /// After a backup's tables land (ADR 0046): this device becomes a new device of the
    /// restored identity, with its own key and certificate. The relay already knows the
    /// identity, so it isn't registered again.
    pub(crate) fn install_restored_account(
        &mut self,
        root: &Signer,
        device: Signer,
        relay_url: &str,
    ) -> R<()> {
        let author = Author::device(root, device.clone());
        self.store.set_meta("me", &root.id())?;
        self.save_account(AccountMeta {
            identity: root.id(),
            device: device.id(),
            cert: author.cert.clone().unwrap_or_default(),
            relay_url: relay_url.trim_end_matches('/').to_string(),
            registered: true,
        })?;
        self.net.author = Some(author);
        self.reload()?;
        Ok(())
    }

    /// Your on-device Zoen (local agent, local DM) until the agent runtime takes over.
    pub(super) fn seed_personal_agent(&mut self, me: &str, key: Option<Signer>) -> R<()> {
        let at = crate::engine::now_ms();
        self.ensure_personal_space()?;
        let existing = self
            .identities
            .values()
            .find(|person| {
                person.kind == IdentityKind::Agent
                    && person.owner.as_deref() == Some(me)
                    && person.handle == "zoen"
            })
            .map(|person| person.id.clone());
        let zoen = match existing {
            Some(id) => id,
            None => self.create_identity_with_signer(
                Identity {
                    owner_proof: None,
                    id: String::new(),
                    kind: IdentityKind::Agent,
                    name: "Zoen".into(),
                    handle: "zoen".into(),
                    tint_hex: "#7C5CFF".into(),
                    glyph: Some("sparkles".into()),
                    owner: Some(me.to_string()),
                    bio: t(
                        "Seu agente. Transforma conversa em coisa.",
                        "Your agent. Turns conversation into things.",
                    ),
                },
                true,
                key.ok_or_else(|| CoreError::Storage {
                    message: "personal agent key was not prepared".into(),
                })?,
            )?,
        };
        if self.state.spaces.values().any(|space| {
            space.kind == SpaceKind::Direct
                && space.members.iter().any(|(id, _)| id == me)
                && space.members.iter().any(|(id, _)| id == &zoen)
        }) {
            return Ok(());
        }
        let partial = self
            .state
            .spaces
            .values()
            .find(|space| {
                space.kind == SpaceKind::Direct
                    && space.privacy == Privacy::EndToEnd
                    && space.title == "Zoen"
                    && space.members.len() == 1
                    && space
                        .members
                        .iter()
                        .any(|(id, role)| id == me && *role == Role::Owner)
            })
            .map(|space| space.id.clone());
        let dm = match partial {
            Some(id) => id,
            None => {
                let id = new_id("sp")?;
                self.append_at(
                    &id,
                    me,
                    at,
                    EventBody::SpaceCreated {
                        title: "Zoen".into(),
                        kind: SpaceKind::Direct,
                        privacy: Privacy::EndToEnd,
                    },
                )?;
                id
            }
        };
        self.append_at(
            &dm,
            me,
            at,
            EventBody::MemberAdded {
                identity: zoen,
                role: Role::Member,
            },
        )?;
        Ok(())
    }

    /// Loads the device key from the vault. `false` = no account (or key missing).
    pub fn unlock(&mut self, device_secret: Option<Vec<u8>>) -> R<bool> {
        let Some(acct) = self.net.account.clone() else {
            return Ok(false);
        };
        let Some(secret) = device_secret.and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok())
        else {
            return Ok(false);
        };
        let key = Signer::from_secret(&secret);
        if key.id() != acct.device {
            return Err(CoreError::Invalid {
                reason: t(
                    "A chave deste aparelho não confere com a conta.",
                    "This device's key doesn't match the account.",
                ),
            });
        }
        self.net.author = Some(Author::certified(
            acct.identity.clone(),
            key,
            acct.cert.clone(),
        ));
        self.net
            .mls
            .wake_recovery_many(self.net.synced.iter().cloned());
        Ok(true)
    }

    pub fn set_registered(&mut self, registered: bool) -> R<()> {
        if let Some(mut a) = self.net.account.clone() {
            if a.registered != registered {
                a.registered = registered;
                self.save_account(a)?;
            }
        }
        Ok(())
    }

    /// (identity, device key, cert, profile, registered) for the network task.
    pub fn net_credentials(&self) -> Option<(IdentityId, Signer, String, Identity, bool, String)> {
        let a = self.net.account.as_ref()?;
        let author = self.net.author.as_ref()?;
        Some((
            a.identity.clone(),
            author.key.clone(),
            a.cert.clone(),
            self.my_profile()?,
            a.registered,
            a.relay_url.clone(),
        ))
    }
}

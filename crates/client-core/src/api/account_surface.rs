// Synchronous account, conversation and profile FFI family, in the parent api scope.
#[uniffi::export]
impl Client {
    /// The account on this device, if any.
    pub fn account(&self) -> Option<AccountDto> {
        let _language = crate::i18n::enter(self.lang);
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
        let _language = crate::i18n::enter(self.lang);
        {
            let mut e = self.lock();
            if crate::provider_auth::ProviderSetup::load(&vault)?.is_some() {
                return Err(invalid(
                    "Conclua a entrada já preparada antes de criar outra conta.",
                ));
            }
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
        let _language = crate::i18n::enter(self.lang);
        self.resume_provider_setup(vault.clone())?;
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

    /// Resume an interrupted social signup before checking for an orphaned vault
    /// or a locked account. All local keys/metadata are validated before the
    /// staging record is removed; server registration remains pending.
    pub fn resume_provider_setup(&self, vault: Arc<dyn SecretVault>) -> Result<bool, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        let mut engine = self.lock();
        let Some(setup) = crate::provider_auth::ProviderSetup::load(&vault)? else {
            return Ok(false);
        };
        setup.resume(&mut engine, &vault)?;
        Ok(true)
    }

    /// Connects to the relay and keeps the connection (reconnects with backoff).
    pub fn start_sync(&self, listener: Option<Arc<dyn CoreListener>>) -> Result<(), CoreError> {
        let _language = crate::i18n::enter(self.lang);
        if self.lock().provider_setup_pending() {
            return Err(invalid(t(
                "Continue com Apple ou Google para concluir sua entrada.",
                "Continue with Apple or Google to finish signing in.",
            )));
        }
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
        let _language = crate::i18n::enter(self.lang);
        self.link_pickup.retire();
        self.stop_sync();
        {
            let mut e = self.lock();
            e.wipe()?;
            e.net = Default::default();
        }
        vault.delete(VAULT_IDENTITY.into());
        vault.delete(VAULT_DEVICE.into());
        vault.delete(VAULT_AGREEMENT.into());
        vault.delete(crate::provider_auth::VAULT_SETUP.into());
        vault.delete(crate::link::VAULT_PENDING.into());
        Ok(())
    }

    pub fn stop_sync(&self) {
        let _language = crate::i18n::enter(self.lang);
        self.link_pickup.cancel();
        self.cancel_history_receive();
        let net = self.net.lock().unwrap_or_else(|p| p.into_inner()).take();
        {
            let mut engine = self.lock();
            engine.net.poke = None;
            // Historical opens may be awaiting a connection that never started.
            // Stop invalidates them even when there is no Net object to drop.
            engine.net.file_demands.clear();
        }
        if let Some(n) = net {
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
            .map(|(id, dev)| ::mls::leaf_name(&id, &dev))
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

    /// Authorizes this agent identity from the unlocked owner device. First
    /// registration checks this device's enrollment; this grants no tool or budget.
    pub fn authorize_agent(&self, agent_id: String) -> Result<AgentAuthorizationDto, CoreError> {
        let e = self.lock();
        let owner_id = e
            .account()
            .ok_or_else(|| invalid("create or unlock an account first"))?
            .identity
            .clone();
        let proof = e.authorize_agent(&agent_id)?;
        Ok(AgentAuthorizationDto {
            owner_id,
            agent_id,
            device_id: proof.device,
            certificate: proof.cert,
            signature: proof.signature,
        })
    }

    /// A person's profile as this device can read it: name, bio and photo for contacts
    /// (people you share a chat or Space with), only the @handle for everyone else.
    pub fn get_profile(&self, identity_id: String) -> Result<ProfileDto, CoreError> {
        self.lock().profile_view(&identity_id)
    }

    /// Reads only the exact currently authorized profile photo, bounded to 5 MB.
    pub fn profile_photo(
        &self,
        identity_id: String,
        expected_sha256: String,
    ) -> Result<Option<crate::ProfilePhotoBytes>, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock()
            .profile_photo_bytes(&identity_id, &expected_sha256)
    }

    /// Edits your encrypted profile. The relay stores only ciphertext; your contacts
    /// get it through the key you share with them. `photo`: keep, remove or set (≤ 5 MB).
    pub fn update_my_profile(
        &self,
        name: String,
        bio: String,
        photo: PhotoChange,
    ) -> Result<ProfileDto, CoreError> {
        let _language = crate::i18n::enter(self.lang);
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

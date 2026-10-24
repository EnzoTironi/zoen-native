//! Fixed-key prepared account installation and signup validation.

use super::*;

impl Engine {
    /// Only the protected staging identity may repair its unfinished local install.
    /// An existing real account, a different vault, or unrelated local data is never
    /// replaced by recovery of a signup journal.
    pub(crate) fn validate_provider_setup(
        &self,
        setup: &crate::provider_auth::ProviderSetup,
    ) -> R<()> {
        let root = Signer::from_secret(&setup.root);
        let device = Signer::from_secret(&setup.device);
        let identity = root.id();
        let certificate = root.sign(&event_log::device_cert_message(&device.id()));
        let forbidden = || CoreError::Invalid {
            reason:
                "A preparação não pertence a esta conta. Seus dados e chaves foram preservados."
                    .into(),
        };
        if self.store.meta("account")?.is_some() && self.account().is_none() {
            return Err(forbidden());
        }
        if let Some(account) = self.account() {
            if account.registered
                || account.identity != identity
                || account.device != device.id()
                || account.cert != certificate
                || account.relay_url != setup.relay
                || self.store.meta("provider_signup_pending")?.as_deref() != Some("1")
            {
                return Err(forbidden());
            }
        }
        if self.me.as_deref().is_some_and(|me| me != identity)
            || self.identities.values().any(|person| {
                if person.id == identity {
                    person.kind != IdentityKind::Person || person.handle != setup.handle
                } else {
                    person.kind != IdentityKind::Agent
                        || person.owner.as_deref() != Some(identity.as_str())
                        || !event_log::agent_owner::profile_authorized(person)
                }
            })
            || self.logs.values().any(|log| {
                log.events().iter().any(|event| {
                    event.author != identity
                        || event.device.as_deref() != Some(device.id().as_str())
                        || event.cert.as_deref() != Some(certificate.as_str())
                })
            })
            || !self.broken_spaces.is_empty()
            || !self.store.synced_spaces()?.is_empty()
            || self
                .store
                .profile_keys()?
                .iter()
                .any(|row| row.identity != identity)
        {
            return Err(forbidden());
        }
        Ok(())
    }

    pub(crate) fn validate_installed_provider_setup(
        &self,
        setup: &crate::provider_auth::ProviderSetup,
    ) -> R<()> {
        self.validate_provider_setup(setup)?;
        let identity = Signer::from_secret(&setup.root).id();
        let profile = self.store.profile_key(&identity)?;
        if self.account().is_none()
            || !self.is_unlocked()
            || self.me.as_deref() != Some(identity.as_str())
            || !self.identities.contains_key(&identity)
            || profile.as_ref().is_none_or(|row| {
                event_log::profile::ProfileKey::from_hex(&row.key).is_none()
                    || row
                        .fields
                        .as_deref()
                        .and_then(event_log::profile::ProfileFields::from_bytes)
                        .is_none()
            })
        {
            return Err(CoreError::Invalid {
                reason:
                    "A preparação desta conta ainda não terminou. Suas chaves foram preservadas."
                        .into(),
            });
        }
        Ok(())
    }

    /// The journal already owns all three fixed secrets before this transaction.
    /// A crash before RELEASE rolls SQLite back; a crash after it replays the same
    /// account, private profile key and personal spaces without creating duplicates.
    pub(crate) fn install_provider_setup(
        &mut self,
        setup: &crate::provider_auth::ProviderSetup,
    ) -> R<()> {
        self.validate_provider_setup(setup)?;
        self.install_prepared_account(
            PreparedAccount {
                root: &setup.root,
                device: &setup.device,
                agreement: &setup.agreement,
                profile_key: None,
                personal_agent_key: None,
                name: &setup.name,
                handle: &setup.handle,
                relay: &setup.relay,
                agent: None,
            },
            true,
            |engine| engine.validate_installed_provider_setup(setup),
        )
    }

    /// The only initial account writer. Callers validate their own setup scope;
    /// durable setup paths retain all supplied keys before entering this savepoint.
    /// This method never wipes an account or generates a replacement device.
    pub(crate) fn install_prepared_account(
        &mut self,
        setup: PreparedAccount<'_>,
        provider_signup: bool,
        validate: impl FnOnce(&mut Self) -> R<()>,
    ) -> R<()> {
        let root = Signer::from_secret(setup.root);
        let device = Signer::from_secret(setup.device);
        let identity = root.id();
        let author = Author::device(&root, device.clone());
        // Durable setup recovery keeps existing canonical keys. Fresh material is
        // prepared before the first write and is never substituted for a saved key.
        let profile_key = if self.store.profile_key(&identity)?.is_none() {
            Some(match setup.profile_key {
                Some(key) => key.clone(),
                None => event_log::profile::ProfileKey::generate()?,
            })
        } else {
            None
        };
        let has_personal_agent = self.identities.values().any(|person| {
            person.kind == IdentityKind::Agent
                && person.owner.as_deref() == Some(identity.as_str())
                && person.handle == "zoen"
        });
        let personal_agent = if setup.agent.is_none() && !has_personal_agent {
            Some(match setup.personal_agent_key {
                Some(key) => key.clone(),
                None => Signer::generate()?,
            })
        } else {
            None
        };
        let storage_error = |error: rusqlite::Error| CoreError::Storage {
            message: error.to_string(),
        };
        self.store
            .conn()
            .execute_batch("SAVEPOINT prepared_account")
            .map_err(storage_error)?;
        let result = (|| {
            if provider_signup {
                self.store.set_meta("provider_signup_pending", "1")?;
            }
            if !self.identities.contains_key(&identity) {
                let (owner, owner_proof) = setup.agent.map_or((None, None), |(owner, proof)| {
                    (Some(owner.to_string()), Some(Box::new(proof.clone())))
                });
                let me = Identity {
                    id: identity.clone(),
                    kind: if setup.agent.is_some() {
                        IdentityKind::Agent
                    } else {
                        IdentityKind::Person
                    },
                    name: setup.name.into(),
                    handle: setup.handle.into(),
                    tint_hex: tint_for(&identity),
                    glyph: None,
                    owner,
                    bio: String::new(),
                    owner_proof,
                };
                self.store.put_identity(&me, None)?;
                self.identity_order.push(identity.clone());
                self.identities.insert(identity.clone(), me);
            }
            self.store.set_meta("me", &identity)?;
            self.me = Some(identity.clone());
            self.save_account(AccountMeta {
                identity: identity.clone(),
                device: device.id(),
                cert: author.cert.clone().unwrap_or_default(),
                relay_url: setup.relay.into(),
                registered: false,
            })?;
            self.net.author = Some(author);
            let agreement = event_log::profile::AgreementKey::from_secret(*setup.agreement);
            if let Some(profile_key) = profile_key {
                self.init_profile_with_agreement(&identity, setup.name, agreement, profile_key)?;
            } else {
                self.net.profiles.agreement = Some(agreement);
            }
            if setup.agent.is_none() {
                self.seed_personal_agent(&identity, personal_agent)?;
            }
            validate(self)?;
            self.store
                .conn()
                .execute_batch("RELEASE prepared_account")
                .map_err(storage_error)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = self
                .store
                .conn()
                .execute_batch("ROLLBACK TO prepared_account; RELEASE prepared_account");
            self.net = Default::default();
            self.reload()?;
        }
        result
    }

    pub(crate) fn provider_setup_pending(&self) -> bool {
        // A storage failure must not turn unfinished signup into an active account.
        !matches!(self.store.meta("provider_signup_pending"), Ok(None))
    }
}

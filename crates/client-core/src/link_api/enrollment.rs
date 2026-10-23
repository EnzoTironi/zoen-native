//! Enrollment behind the unchanged native Client facade.

use super::*;

impl Client {
    pub(super) fn link_request_impl(
        &self,
        relay_url: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<LinkRequestDto, CoreError> {
        self.link_pickup.with_request(|| {
            if self.lock().account().is_some() {
                return Err(invalid(t(
                    "Este aparelho já tem uma conta.",
                    "This device already has an account.",
                )));
            }
            let device = Signer::generate();
            let (hpke_secret, hpke_public) = link::key_pair();
            let secret = link::random::<32>();
            let code = LinkCode {
                device: device.id(),
                hpke: hpke_public.clone(),
                secret,
            };
            let pending = Pending {
                device_secret: hex::encode(device.secret()),
                hpke_secret: hex::encode(hpke_secret),
                hpke_public: hex::encode(hpke_public),
                secret: hex::encode(secret),
                relay: relay_url,
            };
            if !vault.save(
                link::VAULT_PENDING.into(),
                serde_json::to_vec(&pending).unwrap_or_default(),
            ) {
                return Err(invalid(t(
                    "Não deu para guardar a chave no Keychain.",
                    "Couldn't store the key in the Keychain.",
                )));
            }
            Ok(LinkRequestDto {
                code: code.encode(),
                check: code.check_digits(),
            })
        })
    }

    pub(super) fn link_code_check_impl(&self, code: String) -> Result<String, CoreError> {
        LinkCode::parse(&code)
            .map(|c| c.check_digits())
            .ok_or_else(|| invalid(t("Código inválido.", "Invalid code.")))
    }

    pub(super) async fn drain_link_pickup_impl(&self) {
        self.link_pickup.drain().await;
    }

    pub(super) async fn link_wait_impl(
        &self,
        vault: Arc<dyn SecretVault>,
        timeout_ms: u64,
    ) -> Result<AccountDto, CoreError> {
        self.link_wait_expected(vault, timeout_ms, None).await
    }

    pub(super) async fn link_device_impl(
        &self,
        code: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<LinkedDto, CoreError> {
        let code = LinkCode::parse(&code)
            .ok_or_else(|| invalid(t("Código inválido.", "Invalid code.")))?;
        let owner = owner::LinkOwner::capture(self)?;
        let prepared = enroll_prepare::prepare(self, &owner, &code, &vault)?;
        match owner
            .request(
                self,
                Op::DeliverLink {
                    id: link::box_id(&code.secret, "identity"),
                    sealed: prepared.sealed,
                    device: Some(DeviceCertificate {
                        device: code.device.clone(),
                        cert: prepared.certificate,
                    }),
                },
            )
            .await?
        {
            Reply::Done => {}
            _ => return Err(invalid("the relay answered something else")),
        }
        owner.with(self, |engine| engine.device_linked(&code.device))?;
        Ok(LinkedDto {
            check: code.check_digits(),
            device_id: code.device,
        })
    }

    pub(crate) async fn link_wait_expected(
        &self,
        vault: Arc<dyn SecretVault>,
        timeout_ms: u64,
        expected_identity_id: Option<String>,
    ) -> Result<AccountDto, CoreError> {
        let expected = ExpectedIdentity::parse(expected_identity_id)?;
        let mut pickup = self.link_pickup.begin(
            || {
                let bytes = vault
                    .load(link::VAULT_PENDING.into())
                    .map(zeroize::Zeroizing::new)
                    .ok_or_else(|| invalid(t("Nenhum pedido de vínculo.", "No link request.")))?;
                serde_json::from_slice(&bytes).map_err(|_| invalid("Invalid link request."))
            },
            Duration::from_millis(timeout_ms),
            self.lang,
        )?;
        let sealed = pickup.wait().await?;
        pickup.with_install(|pending, device| {
            let secret = zeroize::Zeroizing::new(
                secret32(hex::decode(&pending.secret).ok())
                    .ok_or_else(|| invalid("link request"))?,
            );
            let hpke_secret =
                zeroize::Zeroizing::new(hex::decode(&pending.hpke_secret).unwrap_or_default());
            let plain = zeroize::Zeroizing::new(
                link::open(&hpke_secret, &secret, "identity", &sealed)
                    .ok_or_else(|| invalid(t("O vínculo não abriu.", "The link didn't open.")))?,
            );
            let b: IdentityBox =
                serde_json::from_slice(&plain).map_err(|e| invalid(e.to_string()))?;
            self.finish_link(pending, device, b, vault, &expected)
        })
    }

    pub(super) fn finish_link(
        &self,
        pending: &Pending,
        device: &Signer,
        b: IdentityBox,
        vault: Arc<dyn SecretVault>,
        expected: &ExpectedIdentity,
    ) -> Result<AccountDto, CoreError> {
        let identity = Signer::from_secret(
            &secret32(hex::decode(&b.identity_secret).ok()).ok_or_else(|| invalid("identity"))?,
        );
        expected.check(&identity.id())?;
        // The certificate must be the account's, for this device.
        if !event_log::verify_sig(
            &identity.id(),
            &event_log::device_cert_message(&device.id()),
            &b.cert,
        ) {
            return Err(invalid(t(
                "O certificado deste aparelho não confere.",
                "This device's certificate doesn't match.",
            )));
        }
        let agreement = hex::decode(&b.agreement_secret).unwrap_or_default();
        {
            let mut e = self.lock();
            e.create_linked_account(&b, &identity, device.clone(), &pending.relay)?;
            let peer = Peer {
                device: b.primary.clone(),
                their_hpke: b.primary_hpke.clone(),
                my_hpke_secret: pending.hpke_secret.clone(),
                secret: pending.secret.clone(),
            };
            let mut peers = load_peers(&vault);
            peers.push(peer.clone());
            if !vault.save(VAULT_IDENTITY.into(), identity.secret().to_vec())
                || !vault.save(VAULT_DEVICE.into(), device.secret().to_vec())
                || !vault.save(VAULT_AGREEMENT.into(), agreement.clone())
                || !save_peers(&vault, &peers)
            {
                let _ = e.wipe();
                let _ = e.store.meta_delete("account");
                return Err(invalid(t(
                    "Não deu para guardar a chave no Keychain.",
                    "Couldn't store the key in the Keychain.",
                )));
            }
            e.add_peer(peer);
            e.unlock_profile(Some(agreement))?;
        }
        vault.delete(link::VAULT_PENDING.into());
        self.account_dto_pub().ok_or_else(|| invalid("account"))
    }
}

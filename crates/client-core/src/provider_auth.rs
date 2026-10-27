//! Social authentication provisions a fresh local identity, then binds its certified
//! device to a verified provider subject. An interrupted request retains those keys
//! and remains on onboarding until an idempotent server confirmation succeeds.

use std::{sync::Arc, time::Duration};

use event_log::{profile::AgreementKey, Signer};
use protocol::provider_auth::{provider_bind_message, ProviderBindRequest, ProviderBindResponse};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::{
    api::invalid,
    profile::VAULT_AGREEMENT,
    sync::{VAULT_DEVICE, VAULT_IDENTITY},
    AccountDto, Client, CoreError, SecretVault,
};

const PENDING: &str = "provider_signup_pending";
pub(crate) const VAULT_SETUP: &str = "zoen.provider-setup.v1";

/// A single atomic vault record precedes every canonical database/key-slot write.
/// It contains no provider proof and is discarded only after the local install is
/// validated. A process can stop after any later write and replay these fixed keys.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProviderSetup {
    pub(crate) version: u8,
    pub(crate) root: [u8; 32],
    pub(crate) device: [u8; 32],
    pub(crate) agreement: [u8; 32],
    pub(crate) name: String,
    pub(crate) handle: String,
    pub(crate) relay: String,
}

impl Drop for ProviderSetup {
    fn drop(&mut self) {
        self.root.zeroize();
        self.device.zeroize();
        self.agreement.zeroize();
    }
}

impl ProviderSetup {
    fn new(name: &str, relay: &str) -> Result<Self, CoreError> {
        let root = Signer::generate()?;
        let device = Signer::generate()?;
        let agreement = AgreementKey::generate()?;
        Ok(Self {
            version: 1,
            root: root.secret(),
            device: device.secret(),
            agreement: agreement.secret(),
            name: name.into(),
            handle: format!("zoen_{}", &root.id()[..16]),
            relay: relay.into(),
        })
    }

    fn validate(&self) -> Result<(), CoreError> {
        if self.version != 1
            || self.name.trim() != self.name
            || self.name.is_empty()
            || self.name.chars().count() > 64
            || self.name.chars().any(char::is_control)
            || self.relay.len() > 2048
            || relay_base(&self.relay)? != self.relay
            || self.handle != format!("zoen_{}", &Signer::from_secret(&self.root).id()[..16])
            || self.root == self.device
        {
            return Err(invalid(
                "A preparação desta conta está inválida. As chaves foram preservadas.",
            ));
        }
        Ok(())
    }

    pub(crate) fn load(vault: &Arc<dyn SecretVault>) -> Result<Option<Self>, CoreError> {
        let Some(bytes) = vault.load(VAULT_SETUP.into()).map(Zeroizing::new) else {
            return Ok(None);
        };
        if bytes.len() > 4096 {
            return Err(invalid(
                "A preparação desta conta está inválida. As chaves foram preservadas.",
            ));
        }
        let setup: Self = serde_json::from_slice(&bytes).map_err(|_| {
            invalid("A preparação desta conta está inválida. As chaves foram preservadas.")
        })?;
        setup.validate()?;
        Ok(Some(setup))
    }

    fn save(&self, vault: &Arc<dyn SecretVault>) -> Result<(), CoreError> {
        let bytes = Zeroizing::new(
            serde_json::to_vec(self)
                .map_err(|_| invalid("Não foi possível preparar as chaves."))?,
        );
        let saved = vault.save(VAULT_SETUP.into(), bytes.to_vec());
        let held = vault.load(VAULT_SETUP.into()).map(Zeroizing::new);
        if !saved || held.as_deref().map(std::vec::Vec::as_slice) != Some(bytes.as_slice()) {
            return Err(invalid(
                "Não foi possível guardar as chaves deste aparelho.",
            ));
        }
        Ok(())
    }

    pub(crate) fn resume(
        &self,
        engine: &mut crate::engine::Engine,
        vault: &Arc<dyn SecretVault>,
    ) -> Result<(), CoreError> {
        self.validate()?;
        // Validate the whole scope before filling even one missing key slot.
        engine.validate_provider_setup(self)?;
        let keys = [
            (VAULT_IDENTITY, &self.root),
            (VAULT_DEVICE, &self.device),
            (VAULT_AGREEMENT, &self.agreement),
        ];
        for (key, secret) in keys {
            let held = vault.load(key.into()).map(Zeroizing::new);
            if held.as_ref().is_some_and(|held| held.as_slice() != secret) {
                return Err(invalid(
                    "Estas chaves pertencem a outra conta. Nenhuma chave foi substituída.",
                ));
            }
        }
        for (key, secret) in keys {
            let held = vault.load(key.into()).map(Zeroizing::new);
            if held.is_none() && !vault.save(key.into(), secret.to_vec()) {
                return Err(invalid(
                    "Não foi possível guardar as chaves deste aparelho. Tente novamente.",
                ));
            }
            let held = vault.load(key.into()).map(Zeroizing::new);
            if held.as_ref().map(|held| held.as_slice()) != Some(secret.as_slice()) {
                return Err(invalid(
                    "Não foi possível conferir as chaves deste aparelho. Tente novamente.",
                ));
            }
        }
        engine.install_provider_setup(self)?;
        engine.validate_installed_provider_setup(self)?;
        for (key, secret) in keys {
            let held = vault.load(key.into()).map(Zeroizing::new);
            if held.as_ref().map(|held| held.as_slice()) != Some(secret.as_slice()) {
                return Err(invalid(
                    "Não foi possível conferir as chaves deste aparelho. Tente novamente.",
                ));
            }
        }
        vault.delete(VAULT_SETUP.into());
        if vault.load(VAULT_SETUP.into()).map(Zeroizing::new).is_some() {
            return Err(invalid(
                "Não foi possível concluir a preparação. Suas chaves foram preservadas.",
            ));
        }
        Ok(())
    }
}

pub(crate) fn relay_base(value: &str) -> Result<String, CoreError> {
    let parsed = reqwest::Url::parse(value).map_err(|_| invalid("Servidor inválido."))?;
    let loopback = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if !(parsed.scheme() == "https" || (parsed.scheme() == "http" && loopback))
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !matches!(parsed.path(), "" | "/")
    {
        return Err(invalid("Use o endereço HTTPS do servidor."));
    }
    Ok(parsed.as_str().trim_end_matches('/').to_string())
}

impl Client {
    pub(crate) async fn provider_sign_in(
        &self,
        relay_url: String,
        proof_token: String,
        name: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<AccountDto, CoreError> {
        if proof_token.len() != 64 || !proof_token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid(
                "Confirme sua entrada com Apple ou Google novamente.",
            ));
        }
        let relay = relay_base(&relay_url)?;
        let name: String = name
            .trim()
            .chars()
            .filter(|c| !c.is_control())
            .take(64)
            .collect();
        let name = if name.trim().is_empty() {
            "Pessoa Zoen"
        } else {
            name.trim()
        };
        let request = {
            let mut e = self.lock();
            if let Some(setup) = ProviderSetup::load(&vault)? {
                if setup.relay != relay {
                    return Err(invalid("Esta entrada pertence a outro servidor."));
                }
                setup.resume(&mut e, &vault)?;
            } else if e.account().is_none() {
                if [VAULT_IDENTITY, VAULT_DEVICE, VAULT_AGREEMENT]
                    .iter()
                    .any(|key| vault.load((*key).into()).map(Zeroizing::new).is_some())
                {
                    return Err(invalid(
                        "Recupere a conta deste aparelho antes de criar outra.",
                    ));
                }
                let setup = ProviderSetup::new(name, &relay)?;
                e.validate_provider_setup(&setup)?;
                setup.save(&vault)?;
                setup.resume(&mut e, &vault)?;
            }
            let account = e
                .account()
                .ok_or_else(|| invalid("Conta indisponível."))?
                .clone();
            if relay_base(&account.relay_url)? != relay {
                return Err(invalid("Esta entrada pertence a outro servidor."));
            }
            let mut profile = e
                .my_profile()
                .ok_or_else(|| invalid("Perfil indisponível."))?;
            // Names and bios of people are encrypted by the existing profile protocol.
            profile.name.clear();
            profile.bio.clear();
            let author = e
                .net
                .author
                .as_ref()
                .ok_or_else(|| invalid("Desbloqueie este aparelho."))?;
            let message =
                provider_bind_message(&proof_token, &profile, &account.device, &account.cert)
                    .map_err(|_| invalid("Não foi possível preparar a entrada."))?;
            ProviderBindRequest {
                proof_token,
                profile,
                device: account.device,
                certificate: account.cert,
                signature: author.key.sign(&message),
            }
        };
        let body = serde_json::to_vec(&request)
            .map_err(|_| invalid("Não foi possível preparar a entrada."))?;
        let http = crate::https::client()?
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| invalid("Não foi possível conectar ao servidor."))?;
        let mut response = http.post(format!("{relay}/v1/auth/bind"))
            .header("content-type", "application/json")
            .body(body).send().await.map_err(|_| invalid("Não foi possível concluir a entrada. Suas chaves foram preservadas; tente novamente."))?;
        if !response.status().is_success() {
            return Err(invalid(
                if response.status() == reqwest::StatusCode::CONFLICT {
                    "Esta conta já existe. Recupere o acesso pelo seu outro aparelho ou backup."
                } else {
                    "Confirme sua entrada com Apple ou Google novamente. Suas chaves foram preservadas."
                },
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| invalid("Resposta de entrada incompleta. Tente novamente."))?
        {
            if bytes.len().saturating_add(chunk.len()) > 8192 {
                return Err(invalid("Resposta de entrada inválida."));
            }
            bytes.extend_from_slice(&chunk);
        }
        let confirmed: ProviderBindResponse =
            serde_json::from_slice(&bytes).map_err(|_| invalid("Resposta de entrada inválida."))?;
        {
            let mut e = self.lock();
            let account = e
                .account()
                .ok_or_else(|| invalid("Esta sessão foi encerrada."))?;
            if account.identity != request.profile.id
                || account.device != request.device
                || confirmed.identity_id != account.identity
                || confirmed.handle != request.profile.handle
            {
                return Err(invalid(
                    "A conta mudou durante a entrada. Abra o Zoen novamente.",
                ));
            }
            e.set_registered(true)?;
            e.store.meta_delete(PENDING)?;
        }
        self.account_dto()
            .ok_or_else(|| invalid("Conta indisponível."))
    }
}

#[cfg(test)]
mod tests;

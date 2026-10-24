//! Local key preparation precedes replacement; the existing install remains canonical.
use super::*;
use event_log::{
    profile::{AgreementKey, ProfileKey},
    CryptoError,
};

pub(super) struct AccountKeys {
    pub(super) device: Signer,
    pub(super) agreement: AgreementKey,
    pub(super) profile: ProfileKey,
    pub(super) personal_agent: Option<Signer>,
}

impl AccountKeys {
    pub(super) fn generate(personal_agent: bool) -> Result<Self, CryptoError> {
        Self::generate_with(
            personal_agent,
            Signer::generate,
            AgreementKey::generate,
            ProfileKey::generate,
            Signer::generate,
        )
    }

    fn generate_with(
        personal_agent: bool,
        device: impl FnOnce() -> Result<Signer, CryptoError>,
        agreement: impl FnOnce() -> Result<AgreementKey, CryptoError>,
        profile: impl FnOnce() -> Result<ProfileKey, CryptoError>,
        agent: impl FnOnce() -> Result<Signer, CryptoError>,
    ) -> Result<Self, CryptoError> {
        Ok(Self {
            device: device()?,
            agreement: agreement()?,
            profile: profile()?,
            personal_agent: if personal_agent { Some(agent()?) } else { None },
        })
    }
}

impl Engine {
    pub(super) fn replace_prepared_account(
        &mut self,
        setup: PreparedAccount<'_>,
        provider_signup: bool,
    ) -> R<()> {
        batch(&self.store, "SAVEPOINT replace_account")?;
        let mut previous_net = std::mem::take(&mut self.net);
        let result = (|| {
            if !self.is_empty() {
                self.clear_local_store()?;
                self.reload()?;
            }
            self.install_prepared_account(setup, provider_signup, |_| Ok(()))?;
            batch(&self.store, "RELEASE replace_account")?;
            Ok(())
        })();
        if result.is_err() {
            let rollback = batch(
                &self.store,
                "ROLLBACK TO replace_account; RELEASE replace_account",
            );
            self.net = previous_net;
            rollback?;
            self.reload()?;
        } else {
            self.net.poke = previous_net.poke.take();
        }
        result
    }
}

fn batch(store: &store::Store, sql: &str) -> R<()> {
    store
        .conn()
        .execute_batch(sql)
        .map_err(|error| CoreError::Storage {
            message: error.to_string(),
        })
}

#[cfg(test)]
mod tests;

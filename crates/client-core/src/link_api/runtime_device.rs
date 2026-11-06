//! Certify a dedicated executor without publishing account or profile secrets.
use super::*;
use zeroize::Zeroizing;
#[cfg(test)]
mod tests;

impl Client {
    /// Trusted service enrollment for an already owner-authorized agent. This
    /// certifies a separate device; it does not transfer the account, history,
    /// profile keys, MLS state, Runtime custody or permission to execute work.
    /// The normal relay transaction admits the sponsor and target certificate.
    pub async fn link_runtime_device(
        &self,
        code: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<LinkedDto, CoreError> {
        let code = Zeroizing::new(code);
        let code = LinkCode::parse(&code).ok_or_else(|| invalid("Invalid runtime device."))?;
        let owner = owner::LinkOwner::capture(self)?;
        let (certificate, proof) = owner.with(self, |engine| {
            let account = engine.account().ok_or_else(|| invalid("account"))?;
            let profile = engine
                .store
                .identities()?
                .into_iter()
                .map(|(profile, _)| profile)
                .find(|profile| profile.id == account.identity)
                .ok_or_else(|| invalid("agent profile unavailable"))?;
            if profile.kind != types::IdentityKind::Agent
                || profile.owner.is_none()
                || !event_log::agent_owner::profile_authorized(&profile)
                || code.device == account.device
                || engine.revoked_devices().contains(&code.device)
            {
                return Err(invalid("Invalid agent runtime enrollment."));
            }
            let bytes = Zeroizing::new(
                vault
                    .load(VAULT_IDENTITY.into())
                    .ok_or_else(|| invalid("agent signing key unavailable"))?,
            );
            let secret: Zeroizing<[u8; 32]> = Zeroizing::new(
                bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| invalid("agent signing key invalid"))?,
            );
            let signer = Signer::from_secret(&secret);
            if signer.id() != account.identity {
                return Err(invalid("agent signing key differs"));
            }
            let certificate = signer.sign(&event_log::device_cert_message(&code.device));
            // Deterministic public bytes make unknown-ack retries exact, even
            // after a profile edit or cold restart. Never seal an IdentityBox.
            let proof = serde_json::to_vec(&(
                "zoen-runtime-device/1",
                &account.identity,
                &code.device,
                hex::encode(&code.hpke),
                &certificate,
            ))?;
            Ok((certificate, proof))
        })?;
        let reply = owner
            .request(
                self,
                Op::DeliverLink {
                    id: link::box_id(&code.secret, "runtime-device/1"),
                    sealed: proof,
                    device: Some(DeviceCertificate {
                        device: code.device.clone(),
                        cert: certificate,
                    }),
                },
            )
            .await?;
        if !matches!(reply, Reply::Done) {
            return Err(invalid("Runtime device enrollment was not confirmed."));
        }
        owner.with(self, |engine| engine.device_linked(&code.device))?;
        Ok(LinkedDto {
            check: code.check_digits(),
            device_id: code.device,
        })
    }
}

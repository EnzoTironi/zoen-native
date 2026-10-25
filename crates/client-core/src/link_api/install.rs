//! Receiver installation: validate shared keys, then persist their exact bytes.
use super::{
    enroll_delivery, enroll_prepare, http_base, invalid, link, secret32, t, AccountDto, Arc,
    Client, CoreError, ExpectedIdentity, IdentityBox, Peer, Pending, SecretVault, Signer,
    VAULT_AGREEMENT, VAULT_DEVICE, VAULT_IDENTITY, VAULT_PEERS,
};
use event_log::profile::{ProfileFields, ProfileKey};
use zeroize::Zeroizing;

fn invalid_box() -> CoreError {
    invalid(t(
        "As chaves deste vínculo não conferem. Atualize o aparelho principal e gere outro código.",
        "This link's keys are invalid. Update the primary device and create another code.",
    ))
}

fn retained_key(vault: &Arc<dyn SecretVault>, key: &str, expected: &[u8]) -> Result<(), CoreError> {
    if let Some(retained) = vault.load(key.into()).map(Zeroizing::new) {
        if retained.as_slice() != expected {
            return Err(invalid(t(
                "Há outra chave guardada neste aparelho. Recupere essa conta antes de vincular outra.",
                "This device retains another key. Recover that account before linking another.",
            )));
        }
    }
    Ok(())
}

pub(super) fn finish(
    client: &Client,
    pending: &Pending,
    device: &Signer,
    boxed: IdentityBox,
    vault: Arc<dyn SecretVault>,
    expected: &ExpectedIdentity,
) -> Result<AccountDto, CoreError> {
    let identity = Signer::from_secret(
        &secret32(hex::decode(&boxed.identity_secret).ok()).ok_or_else(invalid_box)?,
    );
    expected.check(&identity.id())?;
    if !event_log::verify_sig(
        &identity.id(),
        &event_log::device_cert_message(&device.id()),
        &boxed.cert,
    ) {
        return Err(invalid(t(
            "O certificado deste aparelho não confere.",
            "This device's certificate doesn't match.",
        )));
    }
    let agreement = Zeroizing::new(
        secret32(hex::decode(&boxed.agreement_secret).ok()).ok_or_else(invalid_box)?,
    );
    if ProfileKey::from_hex(&boxed.profile_key).is_none() || boxed.profile_key_version == 0 {
        return Err(invalid_box());
    }
    let fields = hex::decode(&boxed.profile_fields).map_err(|_| invalid_box())?;
    if ProfileFields::from_bytes(&fields).is_none()
        || http_base(&boxed.relay) != http_base(&pending.relay)
        || event_log::parse_device_key(&boxed.primary).is_none()
        || boxed.primary == device.id()
        || secret32(hex::decode(&boxed.primary_hpke).ok()).is_none()
        || secret32(hex::decode(&pending.hpke_secret).ok()).is_none()
        || secret32(hex::decode(&pending.secret).ok()).is_none()
    {
        return Err(invalid_box());
    }
    let peer = Peer {
        device: boxed.primary.clone(),
        their_hpke: boxed.primary_hpke.clone(),
        my_hpke_secret: pending.hpke_secret.clone(),
        secret: pending.secret.clone(),
    };
    {
        let mut engine = client.lock();
        // create_linked_account also checks this before its own transaction.
        // Check here before even reading another owner's retained vault material.
        if engine.account().is_some() {
            return Err(invalid(t(
                "Este aparelho já tem uma conta.",
                "This device already has an account.",
            )));
        }
        retained_key(&vault, VAULT_IDENTITY, &identity.secret())?;
        retained_key(&vault, VAULT_DEVICE, &device.secret())?;
        retained_key(&vault, VAULT_AGREEMENT, agreement.as_slice())?;
        let mut peers = enroll_prepare::peers(&vault)?;
        if !peers.is_empty() && vault.load(VAULT_IDENTITY.into()).is_none() {
            return Err(invalid_box());
        }
        if let Some(retained) = peers.iter().find(|value| value.peer.device == peer.device) {
            if retained.peer.their_hpke != peer.their_hpke
                || retained.peer.my_hpke_secret != peer.my_hpke_secret
                || retained.peer.secret != peer.secret
            {
                return Err(invalid_box());
            }
        } else {
            peers.push(enroll_delivery::RetainedPeer {
                peer: peer.clone(),
                enrollment: None,
            });
        }
        let peer_bytes =
            Zeroizing::new(serde_json::to_vec(&peers).map_err(|error| invalid(error.to_string()))?);
        engine.create_linked_account(
            &boxed,
            &identity,
            device.clone(),
            &pending.relay,
            |engine| {
                // A validated supplied agreement must be loaded, never replaced by
                // an unpersisted generated key. Profile work precedes vault writes.
                if engine.unlock_profile(Some(agreement.to_vec()))?.is_some() {
                    return Err(invalid_box());
                }
                if !vault.save(VAULT_IDENTITY.into(), identity.secret().to_vec())
                    || !vault.save(VAULT_DEVICE.into(), device.secret().to_vec())
                    || !vault.save(VAULT_AGREEMENT.into(), agreement.to_vec())
                    || !vault.save(VAULT_PEERS.into(), peer_bytes.to_vec())
                {
                    // Earlier saves may have been accepted. Keep those exact keys
                    // for explicit retry; the SQLite installation rolls back.
                    return Err(invalid(t(
                        "Não deu para guardar a chave no Keychain.",
                        "Couldn't store the key in the Keychain.",
                    )));
                }
                engine.add_peer(peer);
                Ok(())
            },
        )?;
        vault.delete(link::VAULT_PENDING.into());
        crate::api::account_dto_from(&engine).ok_or_else(|| invalid("account"))
    }
}

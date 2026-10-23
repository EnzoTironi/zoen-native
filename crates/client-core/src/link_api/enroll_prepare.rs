//! Prepare one approved enrollment without replacing durable retry key material.
use super::*;
use zeroize::{Zeroize, Zeroizing};

pub(super) struct PreparedEnrollment {
    pub(super) sealed: Vec<u8>,
    pub(super) certificate: String,
}

fn peers(vault: &Arc<dyn SecretVault>) -> Result<Vec<Peer>, CoreError> {
    let Some(bytes) = vault.load(VAULT_PEERS.into()).map(Zeroizing::new) else {
        return Ok(Vec::new());
    };
    let peers: Vec<Peer> = serde_json::from_slice(&bytes)
        .map_err(|_| invalid("The retained linked-device keys are invalid."))?;
    let mut ids = BTreeSet::new();
    if peers.iter().any(|peer| !ids.insert(&peer.device)) {
        return Err(invalid("The retained linked-device keys are ambiguous."));
    }
    Ok(peers)
}

fn sender_key(peer: &Peer) -> Result<Vec<u8>, CoreError> {
    let secret = Zeroizing::new(
        secret32(hex::decode(&peer.my_hpke_secret).ok())
            .ok_or_else(|| invalid("The retained linked-device key is invalid."))?,
    );
    // The existing Curve25519 dependency derives the public X25519 key from
    // exactly the retained private key, rather than minting a replacement.
    Ok(
        curve25519_dalek::montgomery::MontgomeryPoint::mul_base_clamped(*secret)
            .to_bytes()
            .to_vec(),
    )
}

pub(super) fn prepare(
    client: &Client,
    owner: &owner::LinkOwner,
    code: &LinkCode,
    vault: &Arc<dyn SecretVault>,
) -> Result<PreparedEnrollment, CoreError> {
    owner.with(client, |engine| {
        let identity = Zeroizing::new(vault.load(VAULT_IDENTITY.into()).ok_or_else(|| {
            invalid(t(
                "A chave da conta não está aqui.",
                "The account key isn't here.",
            ))
        })?);
        let identity = Signer::from_secret(
            &secret32(Some(identity.to_vec())).ok_or_else(|| invalid("identity"))?,
        );
        let account = engine.account().ok_or_else(|| invalid("account"))?;
        if identity.id() != account.identity {
            return Err(invalid("identity"));
        }
        if code.device == account.device || engine.revoked_devices().contains(&code.device) {
            return Err(invalid("That device cannot be linked with this request."));
        }
        let agreement = Zeroizing::new(
            secret32(vault.load(VAULT_AGREEMENT.into()))
                .ok_or_else(|| invalid("The account agreement key is unavailable or invalid."))?,
        );
        let current_agreement = engine
            .net
            .profiles
            .agreement
            .as_ref()
            .ok_or_else(|| invalid("The account agreement key is not loaded."))?;
        if current_agreement.secret() != *agreement {
            return Err(invalid(
                "The retained agreement key does not match this account session.",
            ));
        }
        let mut peers = peers(vault)?;
        let (peer, public, persist) = match peers.iter().find(|peer| peer.device == code.device) {
            Some(peer) => {
                if peer.their_hpke != hex::encode(&code.hpke)
                    || peer.secret != hex::encode(code.secret)
                {
                    return Err(invalid(
                        "That device already has another retained link request.",
                    ));
                }
                (peer.clone(), sender_key(peer)?, false)
            }
            None => {
                let (secret, public) = link::key_pair();
                (
                    Peer {
                        device: code.device.clone(),
                        their_hpke: hex::encode(&code.hpke),
                        my_hpke_secret: hex::encode(&secret),
                        secret: hex::encode(code.secret),
                    },
                    public,
                    true,
                )
            }
        };
        let me = engine.my_profile().ok_or_else(|| invalid("account"))?;
        let (profile_key, profile_key_version, profile_seen_version, profile_fields) =
            engine.profile_for_link()?;
        if event_log::profile::ProfileKey::from_hex(&profile_key).is_none()
            || profile_key_version == 0
        {
            return Err(invalid(
                "The account profile key is unavailable or invalid.",
            ));
        }
        let mut boxed = IdentityBox {
            identity_secret: hex::encode(identity.secret()),
            agreement_secret: hex::encode(&*agreement),
            cert: identity.sign(&event_log::device_cert_message(&code.device)),
            name: me.name,
            handle: me.handle,
            relay: account.relay_url.clone(),
            primary: account.device.clone(),
            primary_hpke: hex::encode(public),
            profile_key,
            profile_key_version,
            profile_seen_version,
            profile_fields,
        };
        let plain =
            Zeroizing::new(serde_json::to_vec(&boxed).map_err(|error| invalid(error.to_string()))?);
        boxed.identity_secret.zeroize();
        boxed.agreement_secret.zeroize();
        boxed.profile_key.zeroize();
        let sealed = link::seal(&code.hpke, &code.secret, "identity", &plain)
            .ok_or_else(|| invalid("sealing"))?;
        if persist {
            peers.push(peer.clone());
            let encoded = Zeroizing::new(
                serde_json::to_vec(&peers).map_err(|error| invalid(error.to_string()))?,
            );
            if !vault.save(VAULT_PEERS.into(), encoded.to_vec()) {
                // A false/unknown vault result is not an enrollment receipt.
                // Retained bytes, if any, remain usable by an explicit retry.
                return Err(invalid(t(
                    "Não deu para guardar a chave no Keychain.",
                    "Couldn't store the key in the Keychain.",
                )));
            }
        }
        engine.add_peer(peer);
        Ok(PreparedEnrollment {
            sealed,
            certificate: boxed.cert,
        })
    })
}

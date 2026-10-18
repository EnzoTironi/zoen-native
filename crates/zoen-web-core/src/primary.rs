//! Root authority for a browser that creates its own account. This object lives only
//! in the worker. Its seed is encrypted into a separate vault record, never a client
//! snapshot or public command result. Device handover seals it inside this worker.
use crate::LinkedAccount;
use openmls_traits::signatures::Signer;
use roda_log::{
    profile::{AgreementKey, ProfileFields, ProfileKey},
    Signer as RootSigner,
};
use roda_proto::provider_auth::{provider_bind_message, ProviderBindRequest};
use roda_types::{Identity, IdentityKind};
use zeroize::{Zeroize, Zeroizing};

pub(crate) struct SealedLink {
    pub device: String,
    pub cert: String,
    pub box_id: String,
    pub sealed: Vec<u8>,
    pub peer: roda_link::Peer,
}

pub struct Primary {
    seed: Zeroizing<[u8; 32]>,
    account: LinkedAccount,
}

impl Primary {
    pub fn new(device: &str, relay: &str, name: &str) -> Result<Self, String> {
        roda_log::parse_device_key(device).ok_or("Invalid browser device.")?;
        let mut seed = Zeroizing::new([0; 32]);
        getrandom::getrandom(seed.as_mut()).map_err(|_| "Could not create account keys.")?;
        let root = RootSigner::from_secret(&seed);
        let identity = root.id();
        let name: String = name
            .trim()
            .chars()
            .filter(|character| !character.is_control())
            .take(64)
            .collect();
        let name = if name.is_empty() {
            "Pessoa Zoen".into()
        } else {
            name
        };
        let agreement = AgreementKey::generate();
        let profile = ProfileKey::generate();
        let (hpke_secret, hpke_public) = roda_link::key_pair();
        let account = LinkedAccount {
            identity: identity.clone(),
            device: device.into(),
            cert: root.sign(&roda_log::device_cert_message(device)),
            handle: format!("zoen_{}", &identity[..16]),
            name: name.clone(),
            relay: relay.into(),
            primary: device.into(),
            primary_hpke: hex::encode(hpke_public),
            hpke_secret: hex::encode(hpke_secret),
            link_secret: String::new(),
            agreement_secret: hex::encode(agreement.secret()),
            profile_key: profile.to_hex(),
            profile_key_version: 1,
            profile_seen_version: 1,
            profile_fields: hex::encode(
                ProfileFields {
                    name,
                    bio: String::new(),
                    photo: None,
                }
                .to_bytes(),
            ),
        };
        Ok(Self { seed, account })
    }

    pub fn restore(seed: &[u8], account: LinkedAccount, device: &str) -> Result<Self, String> {
        let seed =
            Zeroizing::new(<[u8; 32]>::try_from(seed).map_err(|_| "Invalid browser authority.")?);
        let root = RootSigner::from_secret(&seed);
        if account.identity != root.id()
            || account.device != device
            || account.primary != device
            || !roda_log::verify_sig(
                &root.id(),
                &roda_log::device_cert_message(device),
                &account.cert,
            )
            || ProfileKey::from_hex(&account.profile_key).is_none()
            || hex::decode(&account.agreement_secret).map_or(true, |bytes| bytes.len() != 32)
        {
            return Err("This authority does not belong to the saved browser account.".into());
        }
        Ok(Self { seed, account })
    }

    /// The account seed is encoded only into the authenticated recipient's HPKE
    /// ciphertext. Both temporary plaintext representations are explicitly erased.
    pub(crate) fn seal_link(
        &self,
        code: &roda_link::LinkCode,
        current: &LinkedAccount,
    ) -> Result<SealedLink, String> {
        let root = RootSigner::from_secret(&self.seed);
        if current.identity != root.id()
            || current.device != self.account.device
            || current.primary != current.device
        {
            return Err("Este navegador não tem a autoridade desta conta.".into());
        }
        if code.device == current.device {
            return Err("Este já é o aparelho atual.".into());
        }
        let (private, public) = roda_link::key_pair();
        let private = Zeroizing::new(private);
        let cert = root.sign(&roda_log::device_cert_message(&code.device));
        let mut identity = roda_link::IdentityBox {
            identity_secret: hex::encode(self.seed.as_ref()),
            agreement_secret: current.agreement_secret.clone(),
            cert: cert.clone(),
            name: current.name.clone(),
            handle: current.handle.clone(),
            relay: current.relay.clone(),
            primary: current.device.clone(),
            primary_hpke: hex::encode(public),
            profile_key: current.profile_key.clone(),
            profile_key_version: current.profile_key_version,
            profile_seen_version: current.profile_seen_version,
            profile_fields: current.profile_fields.clone(),
        };
        let encoded = serde_json::to_vec(&identity).map(Zeroizing::new);
        identity.identity_secret.zeroize();
        identity.agreement_secret.zeroize();
        identity.profile_key.zeroize();
        identity.profile_fields.zeroize();
        let plain = encoded.map_err(|_| "Não foi possível preparar o vínculo.")?;
        let sealed = roda_link::seal(&code.hpke, &code.secret, "identity", &plain)
            .ok_or("Não foi possível proteger o vínculo.")?;
        if sealed.len() > 65_536 {
            return Err("Perfil grande demais para vincular este aparelho.".into());
        }
        Ok(SealedLink {
            device: code.device.clone(),
            cert,
            box_id: roda_link::box_id(&code.secret, "identity"),
            sealed,
            peer: roda_link::Peer {
                device: code.device.clone(),
                their_hpke: hex::encode(&code.hpke),
                my_hpke_secret: hex::encode(private.as_slice()),
                secret: hex::encode(code.secret),
            },
        })
    }

    pub fn account(&self) -> &LinkedAccount {
        &self.account
    }
    /// Worker-internal encryption input. The host boundary must return only the
    /// encrypted authority envelope, then immediately clear this temporary copy.
    pub fn authority_bytes(&self) -> Vec<u8> {
        self.seed.to_vec()
    }

    fn binding_profile(&self, proof: &str) -> Result<Identity, String> {
        if proof.len() != 64 || !proof.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Confirme sua entrada com Apple ou Google novamente.".into());
        }
        Ok(Identity {
            id: self.account.identity.clone(),
            kind: IdentityKind::Person,
            name: String::new(),
            handle: self.account.handle.clone(),
            tint_hex: "#4F7CFF".into(),
            glyph: None,
            owner: None,
            bio: String::new(),
            owner_proof: None,
        })
    }
    pub fn binding_message(&self, proof: &str) -> Result<Vec<u8>, String> {
        provider_bind_message(
            proof,
            &self.binding_profile(proof)?,
            &self.account.device,
            &self.account.cert,
        )
        .map_err(|_| "Could not prepare provider binding.".into())
    }
    pub fn finish_binding(
        &self,
        proof: &str,
        signature: &[u8],
    ) -> Result<ProviderBindRequest, String> {
        let profile = self.binding_profile(proof)?;
        let payload = self.binding_message(proof)?;
        let signature = hex::encode(signature);
        if !roda_log::verify_sig(&self.account.device, &payload, &signature) {
            return Err("The browser signer belongs to another device.".into());
        }
        Ok(ProviderBindRequest {
            proof_token: proof.into(),
            profile,
            device: self.account.device.clone(),
            certificate: self.account.cert.clone(),
            signature,
        })
    }
    pub fn bind_request<S: Signer>(
        &self,
        proof: &str,
        signer: &S,
    ) -> Result<ProviderBindRequest, String> {
        let payload = self.binding_message(proof)?;
        let signature = signer
            .sign(&payload)
            .map_err(|_| "Could not authorize provider binding.")?;
        self.finish_binding(proof, &signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openmls_traits::{
        signatures::{Signer, SignerError},
        types::SignatureScheme,
    };
    struct Device(RootSigner);
    impl Signer for Device {
        fn sign(&self, bytes: &[u8]) -> Result<Vec<u8>, SignerError> {
            hex::decode(self.0.sign(bytes)).map_err(|_| SignerError::SigningError)
        }
        fn signature_scheme(&self) -> SignatureScheme {
            roda_mls::SUITE.signature_algorithm()
        }
    }
    #[test]
    fn primary_cold_retry_keeps_identity_and_never_serializes_authority_into_account_or_bind() {
        let device = Device(RootSigner::from_secret(&[81; 32]));
        let primary =
            Primary::new(&device.0.id(), "https://relay.example", " Nome privado ").unwrap();
        let account = serde_json::to_string(primary.account()).unwrap();
        let secret = primary.authority_bytes();
        assert!(!account.contains(&hex::encode(&secret)));
        assert!(!account.contains("identity_secret"));
        let restored = Primary::restore(
            &secret,
            serde_json::from_str(&account).unwrap(),
            &device.0.id(),
        )
        .unwrap();
        assert_eq!(restored.account().identity, primary.account().identity);
        let request = restored.bind_request(&"ab".repeat(32), &device).unwrap();
        let body = serde_json::to_string(&request).unwrap();
        assert!(!body.contains("Nome privado"));
        assert!(!body.contains(&hex::encode(secret)));
        assert!(roda_log::verify_sig(
            &request.profile.id,
            &roda_log::device_cert_message(&request.device),
            &request.certificate
        ));
        assert!(roda_log::verify_sig(
            &request.device,
            &provider_bind_message(
                &request.proof_token,
                &request.profile,
                &request.device,
                &request.certificate
            )
            .unwrap(),
            &request.signature
        ));
        let other = Device(RootSigner::from_secret(&[82; 32]));
        assert!(restored.bind_request(&"ab".repeat(32), &other).is_err());
        assert!(Primary::restore(&[0; 32], restored.account().clone(), &device.0.id()).is_err());
        assert!(Primary::restore(
            &restored.authority_bytes(),
            restored.account().clone(),
            &other.0.id()
        )
        .is_err());
    }
}

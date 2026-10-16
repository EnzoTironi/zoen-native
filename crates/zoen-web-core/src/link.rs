use hpke_rs::{
    hpke_types::{AeadAlgorithm, KdfAlgorithm, KemAlgorithm},
    rustcrypto::HpkeRustCrypto,
    Hpke, HpkePrivateKey, Mode,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const PSK_ID: &[u8] = b"zoen-link-v1";

fn hpke() -> Hpke<HpkeRustCrypto> {
    Hpke::new(
        Mode::Psk,
        KemAlgorithm::DhKem25519,
        KdfAlgorithm::HkdfSha256,
        AeadAlgorithm::ChaCha20Poly1305,
    )
}

/// Only link-agreement material. There is no device signing secret in this record.
#[derive(Serialize, Deserialize, Clone)]
pub struct PendingLink {
    pub device: String,
    pub relay: String,
    pub hpke_secret: String,
    pub secret: String,
    pub code: String,
    pub check: String,
    pub box_id: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LinkedAccount {
    pub identity: String,
    pub device: String,
    pub cert: String,
    pub name: String,
    pub handle: String,
    pub relay: String,
    pub primary: String,
    pub primary_hpke: String,
    pub hpke_secret: String,
    pub link_secret: String,
    pub agreement_secret: String,
    pub profile_key: String,
    pub profile_key_version: u64,
    pub profile_seen_version: u64,
    pub profile_fields: String,
}

/// Native ADR0045 identity box. The identity seed is used to verify its certificate,
/// then erased. The browser's own device private key never crosses this boundary.
#[derive(Deserialize)]
struct IdentityBox {
    identity_secret: String,
    agreement_secret: String,
    cert: String,
    name: String,
    handle: String,
    relay: String,
    primary: String,
    primary_hpke: String,
    profile_key: String,
    profile_key_version: u64,
    profile_seen_version: u64,
    profile_fields: String,
}

pub fn create_link(device: &str, relay: &str) -> Result<PendingLink, String> {
    zoen_log::parse_device_key(device).ok_or("invalid device public key")?;
    let mut ikm = Zeroizing::new([0u8; 32]);
    let mut secret = Zeroizing::new([0u8; 32]);
    getrandom::getrandom(ikm.as_mut()).map_err(|e| e.to_string())?;
    getrandom::getrandom(secret.as_mut()).map_err(|e| e.to_string())?;
    let pair = hpke()
        .derive_key_pair(ikm.as_ref())
        .map_err(|e| format!("{e:?}"))?;
    let (private, public) = pair.into_keys();
    let code = format!(
        "zoen-link:1:{device}{}{}",
        hex::encode(public.as_slice()),
        hex::encode(secret.as_ref())
    );
    let digest = Sha256::digest(code.as_bytes());
    let check = format!(
        "{:06}",
        u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) % 1_000_000
    );
    let box_id = hex::encode(
        Sha256::new()
            .chain_update(b"zoen-link-box-v1\0")
            .chain_update(secret.as_ref())
            .chain_update(b"identity")
            .finalize(),
    );
    Ok(PendingLink {
        device: device.to_string(),
        relay: relay.to_string(),
        hpke_secret: hex::encode(private.as_slice()),
        secret: hex::encode(secret.as_ref()),
        code,
        check,
        box_id,
    })
}

pub fn complete_link(pending: &PendingLink, sealed: &[u8]) -> Result<LinkedAccount, String> {
    if sealed.len() < 48 || sealed.len() > 65_536 {
        return Err("invalid identity box size".into());
    }
    let private = Zeroizing::new(hex::decode(&pending.hpke_secret).map_err(|e| e.to_string())?);
    let secret = Zeroizing::new(hex::decode(&pending.secret).map_err(|e| e.to_string())?);
    if private.len() != 32 || secret.len() != 32 {
        return Err("invalid pending link".into());
    }
    let plain = Zeroizing::new(
        hpke()
            .open(
                &sealed[..32],
                &HpkePrivateKey::new(private.to_vec()),
                b"zoen-link-v1:identity",
                b"",
                &sealed[32..],
                Some(secret.as_ref()),
                Some(PSK_ID),
                None,
            )
            .map_err(|_| "The phone's link could not be opened.")?,
    );
    let mut identity_box: IdentityBox =
        serde_json::from_slice(&plain).map_err(|e| e.to_string())?;
    let encoded_secret = Zeroizing::new(std::mem::take(&mut identity_box.identity_secret));
    let identity_secret =
        Zeroizing::new(hex::decode(encoded_secret.as_str()).map_err(|e| e.to_string())?);
    let seed = Zeroizing::new(
        <[u8; 32]>::try_from(identity_secret.as_slice()).map_err(|_| "invalid identity key")?,
    );
    let identity = zoen_log::Signer::from_secret(&seed).id();
    if !zoen_log::verify_sig(
        &identity,
        &zoen_log::device_cert_message(&pending.device),
        &identity_box.cert,
    ) {
        return Err("The phone's certificate does not match this browser device.".into());
    }
    if identity_box.relay.trim_end_matches('/') != pending.relay.trim_end_matches('/') {
        return Err("The phone uses a different relay. Pair from the same relay.".into());
    }
    Ok(LinkedAccount {
        identity,
        device: pending.device.clone(),
        cert: identity_box.cert,
        name: identity_box.name,
        handle: identity_box.handle,
        relay: pending.relay.clone(),
        primary: identity_box.primary,
        primary_hpke: identity_box.primary_hpke,
        hpke_secret: pending.hpke_secret.clone(),
        link_secret: pending.secret.clone(),
        agreement_secret: identity_box.agreement_secret,
        profile_key: identity_box.profile_key,
        profile_key_version: identity_box.profile_key_version,
        profile_seen_version: identity_box.profile_seen_version,
        profile_fields: identity_box.profile_fields,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hpke_rs::HpkePublicKey;
    use zoen_log::Signer;

    fn phone_box(pending: &PendingLink, certified_device: &str) -> Vec<u8> {
        let identity = Signer::from_secret(&[7; 32]);
        let cert = identity.sign(&zoen_log::device_cert_message(certified_device));
        let payload = serde_json::json!({ "identity_secret": hex::encode(identity.secret()),
            "agreement_secret": hex::encode([9;32]), "cert":cert, "name":"Test", "handle":"test",
            "relay": pending.relay, "primary": identity.id(), "primary_hpke": hex::encode([8;32]),
            "profile_key": hex::encode([2;32]), "profile_key_version":1, "profile_seen_version":1, "profile_fields":"" });
        let code = pending.code.strip_prefix("zoen-link:1:").unwrap();
        let pk = HpkePublicKey::new(hex::decode(&code[64..128]).unwrap());
        let secret = hex::decode(&pending.secret).unwrap();
        let (enc, ct) = hpke()
            .seal(
                &pk,
                b"zoen-link-v1:identity",
                b"",
                &serde_json::to_vec(&payload).unwrap(),
                Some(&secret),
                Some(PSK_ID),
                None,
            )
            .unwrap();
        [enc, ct].concat()
    }

    #[test]
    fn native_psk_box_certifies_only_this_browser_and_retains_no_identity_seed() {
        let device = Signer::from_secret(&[3; 32]).id();
        let pending = create_link(&device, "http://localhost:8080").unwrap();
        assert_eq!(pending.code.len(), 204);
        let linked = complete_link(&pending, &phone_box(&pending, &device)).unwrap();
        assert_eq!(linked.device, device);
        assert!(!serde_json::to_string(&linked)
            .unwrap()
            .contains("identity_secret"));
        assert!(complete_link(
            &pending,
            &phone_box(&pending, &Signer::from_secret(&[4; 32]).id())
        )
        .is_err());
        let mut tampered = phone_box(&pending, &device);
        *tampered.last_mut().unwrap() ^= 1;
        assert!(complete_link(&pending, &tampered).is_err());
    }
}

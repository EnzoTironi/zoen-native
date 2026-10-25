use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

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

pub fn create_link(device: &str, relay: &str) -> Result<PendingLink, String> {
    event_log::parse_device_key(device).ok_or("invalid device public key")?;
    let mut secret = Zeroizing::new([0u8; 32]);
    getrandom::getrandom(secret.as_mut()).map_err(|e| e.to_string())?;
    let (private, public) = device_link::key_pair().map_err(|error| error.to_string())?;
    let code = device_link::LinkCode {
        device: device.into(),
        hpke: public,
        secret: *secret,
    };
    let check = code.check_digits();
    let box_id = device_link::box_id(&secret, "identity");
    let code = code.encode();
    Ok(PendingLink {
        device: device.to_string(),
        relay: relay.to_string(),
        hpke_secret: hex::encode(private),
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
        device_link::open(
            &private,
            &secret
                .as_slice()
                .try_into()
                .map_err(|_| "invalid link secret")?,
            "identity",
            sealed,
        )
        .ok_or("The phone's link could not be opened.")?,
    );
    // The identity seed certifies this browser, then is erased. It never enters
    // LinkedAccount or the browser vault.
    let mut identity_box: device_link::IdentityBox =
        serde_json::from_slice(&plain).map_err(|e| e.to_string())?;
    let encoded_secret = Zeroizing::new(std::mem::take(&mut identity_box.identity_secret));
    let identity_secret =
        Zeroizing::new(hex::decode(encoded_secret.as_str()).map_err(|e| e.to_string())?);
    let seed = Zeroizing::new(
        <[u8; 32]>::try_from(identity_secret.as_slice()).map_err(|_| "invalid identity key")?,
    );
    let identity = event_log::Signer::from_secret(&seed).id();
    if !event_log::verify_sig(
        &identity,
        &event_log::device_cert_message(&pending.device),
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
    use event_log::Signer;

    fn phone_box(pending: &PendingLink, certified_device: &str) -> Vec<u8> {
        let identity = Signer::from_secret(&[7; 32]);
        let cert = identity.sign(&event_log::device_cert_message(certified_device));
        let payload = serde_json::json!({ "identity_secret": hex::encode(identity.secret()),
            "agreement_secret": hex::encode([9;32]), "cert":cert, "name":"Test", "handle":"test",
            "relay": pending.relay, "primary": identity.id(), "primary_hpke": hex::encode([8;32]),
            "profile_key": hex::encode([2;32]), "profile_key_version":1, "profile_seen_version":1, "profile_fields":"" });
        let code = pending.code.strip_prefix("zoen-link:1:").unwrap();
        let pk = hex::decode(&code[64..128]).unwrap();
        let secret = hex::decode(&pending.secret).unwrap();
        device_link::seal(
            &pk,
            &secret.as_slice().try_into().unwrap(),
            "identity",
            &serde_json::to_vec(&payload).unwrap(),
        )
        .unwrap()
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

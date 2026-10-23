//! Exact original delivery, authenticated by its sender and kept beside peer keys.
use super::*;
use crate::sync::AccountMeta;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct RetainedPeer {
    #[serde(flatten)]
    pub(super) peer: Peer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) enrollment: Option<EnrollmentDelivery>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnrollmentDelivery {
    identity: String,
    primary: String,
    primary_certificate: String,
    primary_hpke: String,
    relay: String,
    code_digest: String,
    certificate: String,
    sealed: String,
    signature: String,
}

fn invalid_delivery() -> CoreError {
    invalid("The original device-link delivery cannot be verified. Create another pairing code.")
}
fn code_digest(code: &LinkCode) -> String {
    let encoded = zeroize::Zeroizing::new(code.encode());
    hex::encode(Sha256::digest(encoded.as_bytes()))
}
fn message(value: &EnrollmentDelivery) -> Result<Vec<u8>, CoreError> {
    serde_json::to_vec(&(
        "zoen-link-delivery/1",
        &value.identity,
        &value.primary,
        &value.primary_certificate,
        &value.primary_hpke,
        &value.relay,
        &value.code_digest,
        &value.certificate,
        &value.sealed,
    ))
    .map_err(|_| invalid_delivery())
}

pub(super) fn replay(
    peer: &RetainedPeer,
    account: &AccountMeta,
    code: &LinkCode,
) -> Result<enroll_prepare::PreparedEnrollment, CoreError> {
    let delivery = peer.enrollment.as_ref().ok_or_else(invalid_delivery)?;
    if delivery.sealed.len() > 90 * 1024 {
        return Err(invalid_delivery());
    }
    let sealed = STANDARD
        .decode(&delivery.sealed)
        .map_err(|_| invalid_delivery())?;
    if delivery.identity != account.identity
        || delivery.primary != account.device
        || delivery.primary_certificate != account.cert
        || delivery.primary_hpke != hex::encode(enroll_prepare::sender_key(&peer.peer)?)
        || delivery.relay != account.relay_url
        || delivery.code_digest != code_digest(code)
        || sealed.len() < 32
        || sealed.len() > 65_536
        || STANDARD.encode(&sealed) != delivery.sealed
        || !event_log::verify_sig(
            &account.identity,
            &event_log::device_cert_message(&account.device),
            &delivery.primary_certificate,
        )
        || !event_log::verify_sig(
            &account.identity,
            &event_log::device_cert_message(&code.device),
            &delivery.certificate,
        )
        || !event_log::verify_sig(&account.device, &message(delivery)?, &delivery.signature)
    {
        return Err(invalid_delivery());
    }
    Ok(enroll_prepare::PreparedEnrollment {
        sealed,
        certificate: delivery.certificate.clone(),
    })
}

pub(super) fn retain(
    peer: &mut RetainedPeer,
    account: &AccountMeta,
    code: &LinkCode,
    certificate: String,
    sealed: Vec<u8>,
    sender: &Signer,
) -> Result<enroll_prepare::PreparedEnrollment, CoreError> {
    if sender.id() != account.device {
        return Err(invalid_delivery());
    }
    let mut delivery = EnrollmentDelivery {
        identity: account.identity.clone(),
        primary: account.device.clone(),
        primary_certificate: account.cert.clone(),
        primary_hpke: hex::encode(enroll_prepare::sender_key(&peer.peer)?),
        relay: account.relay_url.clone(),
        code_digest: code_digest(code),
        certificate,
        sealed: STANDARD.encode(sealed),
        signature: String::new(),
    };
    delivery.signature = sender.sign(&message(&delivery)?);
    peer.enrollment = Some(delivery);
    replay(peer, account, code)
}

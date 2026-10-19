//! Native device authorization and directory profile normalization.
pub mod key_packages;
pub mod reads;
pub mod sealed_profiles;
use protocol::{auth_message, normalize_handle};
use types::{Identity, IdentityKind};

pub fn certificate_valid(identity: &str, device: &str, cert: &str) -> bool {
    event_log::verify_sig(identity, &event_log::device_cert_message(device), cert)
}

pub fn enrollment_certificate_valid(identity: &str, device: &str, cert: &str) -> bool {
    valid_hex64(device) && certificate_valid(identity, device, cert)
}

pub fn auth_valid(device: &str, nonce: &str, relay: &str, sig: &str) -> bool {
    event_log::verify_sig(device, &auth_message(nonce, relay), sig)
}

pub fn provisional_allowed(
    enrolled: Option<(bool, bool)>,
    identity: &str,
    device: &str,
    require_device: bool,
) -> bool {
    !require_device
        && enrolled.is_none_or(|(_, revoked)| !revoked)
        && (enrolled.is_none() || identity == device)
}

pub fn request_authorized(enrolled: Option<(bool, bool)>, unregistered: bool) -> bool {
    enrolled == Some((true, false)) || unregistered
}

/// Exact native session profile normalization, shared before persistence.
pub fn prepare_profile(mut profile: Identity, identity: &str) -> Result<Identity, String> {
    if profile.id != identity {
        return Err("profile is for another identity".into());
    }
    profile.handle = normalize_handle(&profile.handle)
        .ok_or("handle must be 3–24 letters, digits, '.' or '_', starting with a letter")?;
    match profile.kind {
        IdentityKind::Person => {
            profile.name.clear();
            profile.bio.clear();
        }
        IdentityKind::Agent => {
            profile.name = profile.name.trim().chars().take(64).collect();
            profile.bio = profile.bio.chars().take(280).collect();
            if profile.name.is_empty() {
                return Err("name is required".into());
            }
            if profile.owner.is_none() {
                return Err("an agent needs an owner".into());
            }
        }
    }
    Ok(profile)
}

fn valid_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

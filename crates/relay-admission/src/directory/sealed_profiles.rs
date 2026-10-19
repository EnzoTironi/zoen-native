//! Native device-bound encrypted profile and agreement signature validation.
use event_log::profile::{DeviceSigned, MAX_CIPHERTEXT};
use protocol::SealedProfile;

pub fn validate_agreement(
    identity: &str,
    device: &str,
    public: &str,
    signed: &DeviceSigned,
) -> Result<(), String> {
    if signed.device != device {
        return Err("agreement key is for another device".into());
    }
    if !event_log::profile::verify_agreement(identity, public, signed) {
        return Err("agreement key isn't signed by this identity".into());
    }
    Ok(())
}

pub fn validate_profile(
    identity: &str,
    device: &str,
    profile: &SealedProfile,
) -> Result<(), String> {
    if profile.identity != identity {
        return Err("profile is for another identity".into());
    }
    if profile.signed.device != device {
        return Err("profile is for another device".into());
    }
    if profile.ciphertext.len() > MAX_CIPHERTEXT {
        return Err("profile too large".into());
    }
    // Native PostgreSQL stores BIGINT. A signed UInt64 must never wrap negative
    // and invert monotonic replacement in the authoritative SQL row.
    if profile.version > i64::MAX as u64 {
        return Err("profile version out of range".into());
    }
    if !event_log::profile::verify_upload(
        &profile.identity,
        profile.version,
        &profile.ciphertext,
        &profile.signed,
    ) {
        return Err("profile isn't signed by this identity".into());
    }
    Ok(())
}

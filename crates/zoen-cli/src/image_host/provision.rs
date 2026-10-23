//! Dedicated device enrollment through the existing native pairing flow.
//! This process never receives/installs an IdentityBox or copies a Client MLS
//! workspace. The sponsor enrolls the exact new certificate in the SQL directory.
use crate::config::Config;
use crate::file_vault::FileVault;
use event_log::{profile::AgreementKey, Signer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::IsTerminal;
use zeroize::{Zeroize, Zeroizing};
use zoen_runtime::RuntimeAuthority;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Enrollment {
    namespace: String,
    agent: String,
    device: [u8; 32],
    hpke: [u8; 32],
    pairing: [u8; 32],
    capsule: [u8; 32],
    key_version: u32,
}
impl Drop for Enrollment {
    fn drop(&mut self) {
        self.device.zeroize();
        self.hpke.zeroize();
        self.pairing.zeroize();
        self.capsule.zeroize();
    }
}
fn random() -> Result<[u8; 32], String> {
    let mut value = [0; 32];
    getrandom::getrandom(&mut value).map_err(|_| "secure enrollment randomness unavailable")?;
    Ok(value)
}
fn retain(vault: &FileVault, key: &str, bytes: &[u8]) -> Result<(), String> {
    match read(vault, key)? {
        Some(existing) if existing.as_slice() != bytes => {
            return Err("retained provisioning differs; nothing replaced".into());
        }
        Some(_) => return Ok(()),
        None => {}
    }
    let saved = vault.write_bytes(key, bytes).is_ok();
    let observed = read(vault, key)?;
    if !saved || observed.as_deref().map(Vec::as_slice) != Some(bytes) {
        return Err("provisioning save uncertain; resume the original retained setup".into());
    }
    Ok(())
}

fn read(vault: &FileVault, key: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    match vault.read_bytes(key) {
        Ok(bytes) => Ok(Some(Zeroizing::new(bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("retained provisioning unreadable; nothing replaced".into()),
    }
}
pub(super) fn run(config: &Config, vault: &FileVault) -> Result<(), String> {
    if !std::io::stdout().is_terminal() {
        return Err("show pairing only in a trusted interactive terminal".into());
    }
    let code = prepare(config, vault)?;
    println!(
        "device: {}\ncode: {}\ncheck: {}",
        code.device,
        code.encode(),
        code.check_digits()
    );
    Ok(())
}
fn prepare(config: &Config, vault: &FileVault) -> Result<device_link::LinkCode, String> {
    RuntimeAuthority::native_capsule_vault_key(&config.namespace, config.key_version)
        .map_err(|_| "invalid provisioning scope")?;
    // Original material is retained before any derived key or code is exposed.
    // A retry, including an ambiguous file acknowledgement, never reseeds it.
    let slot = format!(
        "runtime_enroll_{}",
        hex::encode(Sha256::digest(
            serde_json::to_vec(&(
                config.namespace.as_str(),
                config.agent.as_str(),
                config.key_version
            ))
            .map_err(|_| "invalid enrollment scope")?
        ))
    );
    let original = match read(vault, &slot)? {
        Some(bytes) => bytes,
        None => {
            let record = Enrollment {
                namespace: config.namespace.clone(),
                agent: config.agent.clone(),
                key_version: config.key_version,
                device: random()?,
                hpke: random()?,
                pairing: random()?,
                capsule: random()?,
            };
            let bytes = Zeroizing::new(
                serde_json::to_vec(&record).map_err(|_| "enrollment encoding unavailable")?,
            );
            retain(vault, &slot, &bytes)?;
            bytes
        }
    };
    if original.len() > 2048 {
        return Err("invalid retained enrollment".into());
    }
    let record: Enrollment =
        serde_json::from_slice(&original).map_err(|_| "invalid retained enrollment")?;
    if record.namespace != config.namespace
        || record.agent != config.agent
        || record.key_version != config.key_version
        || [
            &record.device,
            &record.hpke,
            &record.pairing,
            &record.capsule,
        ]
        .iter()
        .any(|key| key.iter().all(|b| *b == 0))
    {
        return Err("retained enrollment scope changed".into());
    }
    let device = Signer::from_secret(&record.device).id();
    if !config.device.is_empty() && config.device != device {
        return Err("configured device differs from original dedicated device".into());
    }
    let capsule_key =
        RuntimeAuthority::native_capsule_vault_key(&config.namespace, config.key_version)
            .map_err(|_| "invalid capsule locator")?;
    let device_key =
        RuntimeAuthority::native_device_vault_key(&config.namespace, &config.agent, &device)
            .map_err(|_| "invalid device locator")?;
    retain(vault, &capsule_key, &record.capsule)?;
    retain(vault, &device_key, &record.device)?;
    let hpke = hex::decode(AgreementKey::from_secret(record.hpke).public_hex())
        .map_err(|_| "invalid pairing key")?;
    Ok(device_link::LinkCode {
        device,
        hpke,
        secret: record.pairing,
    })
}

#[cfg(test)]
#[path = "provision_tests.rs"]
mod tests;

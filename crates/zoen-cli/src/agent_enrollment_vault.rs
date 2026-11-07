//! Read actual private files before a missing SecretVault value can be trusted.
use crate::file_vault::FileVault;
use zeroize::Zeroizing;
const FILE_LIMIT: u64 = 16 * 1024;

pub(crate) fn preflight_slots(vault: &FileVault) -> Result<(), String> {
    for (key, limit, seed) in [
        ("agent_root", 32, true),
        ("zoen.identity.v1", 32, true),
        ("zoen.device.v1", 32, true),
        ("zoen.agreement.v1", 32, true),
        ("agent.enrollment.pending.v1", FILE_LIMIT, false),
        ("zoen.provider-setup.v1", 0, false),
        ("link.pending", 0, false),
    ] {
        let metadata = match std::fs::symlink_metadata(vault.dir.join(key)) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                return Err(String::from(
                    "Existing protected setup could not be read. Nothing was replaced.",
                ));
            }
        };
        if !metadata.is_file()
            || metadata.len() == 0
            || metadata.len() > limit
            || (seed && metadata.len() != 32)
        {
            return Err(String::from(
                "Existing protected setup is incompatible. Nothing was replaced.",
            ));
        }
        let bytes = Zeroizing::new(vault.read_bytes(key).map_err(|_| {
            String::from("Existing protected setup could not be read. Nothing was replaced.")
        })?);
        if (seed && bytes.len() != 32)
            || (!seed
                && (bytes.is_empty()
                    || bytes.len() as u64 > limit
                    || std::str::from_utf8(&bytes).is_err()))
        {
            return Err(String::from(
                "Existing protected setup is malformed. Nothing was replaced.",
            ));
        }
    }
    Ok(())
}

//! Fresh file-backed fixture adapter, never deployment/KMS evidence.
use super::*;
use client_core::SecretVault;
use std::{fs, path::PathBuf};

pub(super) struct Vault(PathBuf);
impl Vault {
    pub(super) fn new() -> std::result::Result<Arc<Self>, Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(types::new_id("runtime-vault-test")?);
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).unwrap();
        Ok(Arc::new(Self(path)))
    }
}
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        fs::read(self.0.join(key)).ok()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        use std::io::Write;
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(self.0.join(key))
            .and_then(|mut f| {
                f.write_all(&value)?;
                f.sync_all()
            })
            .is_ok()
    }
    fn delete(&self, key: String) {
        let _ = fs::remove_file(self.0.join(key));
    }
}
impl Drop for Vault {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

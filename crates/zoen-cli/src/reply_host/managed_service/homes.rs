use super::*;
use std::path::Path;

pub(super) fn private(path: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| "service home unavailable")?;
    if !metadata.is_dir() {
        return Err("invalid service home".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("service home must be private".into());
        }
    }
    Ok(())
}
fn create(path: &Path) -> Result<(), String> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("service home creation unavailable".into()),
    }
    private(path)
}
pub(super) fn retain(vault: &FileVault, key: &str, bytes: &[u8]) -> Result<(), String> {
    match vault.read_bytes(key) {
        Ok(original) if original == bytes => return Ok(()),
        Ok(_) => return Err("retained service configuration differs".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("retained service configuration unreadable".into()),
    }
    vault
        .write_bytes(key, bytes)
        .map_err(|_| "service save uncertain; retained setup preserved")?;
    if vault
        .read_bytes(key)
        .map_err(|_| "service save unconfirmed")?
        != bytes
    {
        return Err("service save unconfirmed".into());
    }
    Ok(())
}

pub(super) struct Homes {
    pub root: PathBuf,
    pub sponsor: PathBuf,
    pub executor: PathBuf,
}
impl Homes {
    pub fn locate(config: &Config, record: &Record) -> Result<Self, String> {
        if !record.valid() {
            return Err("invalid service record".into());
        }
        let request = record.request.as_ref().ok_or("request")?;
        let root = config.home.join(&record.owner).join(&request.operation);
        Ok(Self {
            sponsor: root.join("account"),
            executor: root.join("executor"),
            root,
        })
    }
    pub fn prepare(config: &Config, record: &Record) -> Result<Self, String> {
        let homes = Self::locate(config, record)?;
        private(&config.home)?;
        create(&config.home.join(&record.owner))?;
        create(&homes.root)?;
        create(&homes.sponsor)?;
        create(&homes.executor)?;
        retain(
            &FileVault {
                dir: homes.root.clone(),
            },
            "request.json",
            &serde_json::to_vec(&(&record.owner, &record.request))
                .map_err(|_| "request encoding")?,
        )?;
        Ok(homes)
    }
    pub fn account_vault(&self) -> Arc<FileVault> {
        Arc::new(FileVault {
            dir: self.sponsor.join("vault"),
        })
    }
    pub fn executor_vault(&self) -> Arc<FileVault> {
        Arc::new(FileVault {
            dir: self.executor.join("vault"),
        })
    }
}
